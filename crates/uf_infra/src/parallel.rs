//! Running one function over a slice on every core.
//!
//! This is the entire shape of uf's data parallelism. `uf_lint` lints each
//! file, `uf_rsc` reads and classifies each module, `uf_bundle` measures each
//! emitted asset: three crates, one operation — map a fallible function over a
//! slice, keep the input's order, stop at the first error. Nothing needs
//! nested parallelism, a global pool, futures, or work-stealing deques.
//!
//! So it is written here rather than depended on. rayon is an excellent crate
//! and it is also a general-purpose parallel iterator framework; sixty lines of
//! scoped threads over an atomic cursor gives uf the one thing it actually
//! uses, with no global thread pool living in the process and nothing to
//! configure. `uf_test` already schedules its own work this way.
//!
//! There is no `unsafe` here. Each thread collects its own results and they are
//! placed by index after every thread has been joined, which is both simpler
//! than sharing the output buffer and, at these sizes, not measurably slower:
//! the work is reading and parsing files, not moving a `Vec` around.
//!
//! # Scheduling
//!
//! Threads pull indices off a shared atomic cursor rather than being handed a
//! contiguous block each. Blocks would be simpler and would be wrong here: lint
//! cost is proportional to file size, and files are not the same size, so a
//! thread handed the block containing the four biggest files finishes long
//! after the rest. Pulling one index at a time costs a single relaxed
//! fetch-add per item and keeps every core busy to the end.

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[cfg(test)]
mod tests;

/// Below this many items, the threads cost more than they save.
///
/// Spawning is on the order of tens of microseconds per thread; a handful of
/// items is done before a pool would have finished starting.
const SERIAL_THRESHOLD: usize = 8;

/// How many threads [`map`] will use for `items` items on `cores` cores.
///
/// Never more threads than there is work, and never more than the machine has
/// cores. One means the caller runs the work on its own thread.
fn threads_for(items: usize, cores: NonZeroUsize) -> usize {
    if items < SERIAL_THRESHOLD {
        return 1;
    }
    cores.get().min(items)
}

/// The number of cores to schedule across.
fn available_cores() -> NonZeroUsize {
    std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN)
}

/// Map `body` over `items` in parallel, in order, stopping at the first error.
///
/// The result is exactly `items.iter().map(body).collect::<Result<Vec<_>, _>>()`
/// — same values, same order, same error — computed on every core. That
/// equivalence is the contract, and it is what lets a caller reach for this
/// without thinking about threads at all.
///
/// # Errors
///
/// Returns the error from the lowest-indexed item that produced one, so a
/// failing run reports the same error every time however the threads
/// interleaved. Once any item fails, threads stop claiming new work; items
/// already in flight run to completion.
///
/// # Panics
///
/// A panic in `body` propagates once every thread has been joined, as
/// [`std::thread::scope`] does.
pub fn map<T, U, E, F>(items: &[T], body: F) -> Result<Vec<U>, E>
where
    T: Sync,
    U: Send,
    E: Send,
    F: Fn(&T) -> Result<U, E> + Sync,
{
    map_with_cores(items, available_cores(), body)
}

/// [`map`], with the core count given rather than detected.
///
/// Separated so the scheduler can be tested at a fixed width: a test that only
/// ran on however many cores the machine happens to have would prove something
/// different on every machine.
fn map_with_cores<T, U, E, F>(items: &[T], cores: NonZeroUsize, body: F) -> Result<Vec<U>, E>
where
    T: Sync,
    U: Send,
    E: Send,
    F: Fn(&T) -> Result<U, E> + Sync,
{
    let threads = threads_for(items.len(), cores);
    if threads == 1 {
        return items.iter().map(body).collect();
    }

    let cursor = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let body = &body;
    let cursor = &cursor;
    let failed = &failed;

    let claimed = std::thread::scope(|scope| {
        let handles = (0..threads)
            .map(|_| {
                scope.spawn(move || {
                    let mut mine: Vec<(usize, Result<U, E>)> = Vec::new();
                    loop {
                        // Relaxed throughout: the cursor only has to hand out
                        // each index once, and the flag only has to be seen
                        // eventually. Every result is published by the join,
                        // which is what orders the writes against the reader.
                        if failed.load(Ordering::Relaxed) {
                            return mine;
                        }
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        if index >= items.len() {
                            return mine;
                        }
                        let outcome = body(&items[index]);
                        if outcome.is_err() {
                            failed.store(true, Ordering::Relaxed);
                        }
                        mine.push((index, outcome));
                    }
                })
            })
            .collect::<Vec<_>>();

        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("a worker thread panicked"))
            .collect::<Vec<_>>()
    });

    collect_in_order(claimed, items.len())
}

/// Put `claimed` results back into input order and unwrap them.
///
/// Returns the error belonging to the lowest index, which is exactly the error
/// the serial equivalent would have returned. Indices are claimed in increasing
/// order, so every index below a failing one was claimed and will have
/// finished; the ones that were never started are all *above* it, and are the
/// same ones a serial run would never have reached.
///
/// The error is looked for before the values are unwrapped, rather than while:
/// unwrapping in order would meet an unclaimed slot first whenever the failure
/// was far enough along for the other threads to have stopped.
fn collect_in_order<U, E>(claimed: Vec<(usize, Result<U, E>)>, length: usize) -> Result<Vec<U>, E> {
    let mut slots: Vec<Option<Result<U, E>>> = (0..length).map(|_| None).collect();
    for (index, outcome) in claimed {
        slots[index] = Some(outcome);
    }

    if let Some(index) = slots.iter().position(|slot| matches!(slot, Some(Err(_)))) {
        match slots.swap_remove(index) {
            Some(Err(error)) => return Err(error),
            _ => unreachable!("the slot at this index was just matched as an error"),
        }
    }

    let results = slots
        .into_iter()
        .map(|slot| match slot {
            Some(Ok(value)) => value,
            // Threads only stop claiming work when something failed, and
            // nothing did, so every index was claimed and succeeded.
            _ => unreachable!("an index was skipped without any item failing"),
        })
        .collect();
    Ok(results)
}
