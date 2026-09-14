//! Deciding what order the files run in.
//!
//! Parallelism is the boring half of a fast runner; the schedule is the half
//! that decides how long the run actually takes. With `n` workers and files of
//! wildly different cost, the run finishes when the *last* file finishes, so
//! the only thing that matters is not starting the most expensive file last.
//!
//! This is longest-processing-time-first (LPT), the classic greedy schedule for
//! identical machines: sort by expected cost descending, hand each free worker
//! the next file. LPT's makespan is within 4/3 - 1/(3n) of optimal, and unlike a
//! smarter schedule it needs no lookahead and no coordination between workers.
//!
//! The expected cost comes from [`crate::timings`] when the file has been seen
//! before, and from its size when it has not. The two are put on one scale so a
//! partially warm cache still produces one ordering rather than two tiers.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::timings::TestTimings;

/// Estimated nanoseconds of scanning per source byte on a cold run.
///
/// Discovery and execution are single-pass byte scans that measure in the low
/// hundreds of MB/s, so a byte costs a handful of nanoseconds. The constant only
/// has to rank files against each other and against recorded microseconds; it is
/// not a prediction of wall-clock time.
pub const COLD_NANOS_PER_BYTE: u64 = 5;

/// Where a file's expected cost came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScheduleBasis {
    /// A duration recorded by a previous run.
    Recorded,
    /// The file's size, because no duration was recorded.
    Size,
}

/// One file's place in the schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleEntry {
    /// Index of the file in the caller's slice.
    pub index: usize,
    /// Relative source file path.
    pub file: CompactString,
    /// Expected cost in microseconds.
    pub weight_micros: u64,
    /// Where that expectation came from.
    pub basis: ScheduleBasis,
}

/// Order files longest-expected-first.
///
/// Ties break on the path, so the order is a pure function of its inputs: two
/// runs over the same suite with the same recorded timings produce the same
/// schedule, which is what makes a parallel run reproducible enough to debug.
pub fn schedule_files(files: &[(&str, &str)], timings: &TestTimings) -> Vec<ScheduleEntry> {
    let mut entries: Vec<ScheduleEntry> = files
        .iter()
        .enumerate()
        .map(|(index, (file, source))| {
            let (weight_micros, basis) = match timings.get(file) {
                Some(micros) => (micros, ScheduleBasis::Recorded),
                None => (cold_weight_micros(source.len()), ScheduleBasis::Size),
            };
            ScheduleEntry {
                index,
                file: CompactString::from(*file),
                weight_micros,
                basis,
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        b.weight_micros
            .cmp(&a.weight_micros)
            .then(a.file.cmp(&b.file))
    });
    entries
}

/// The cold estimate for a file of `bytes` bytes, in microseconds.
///
/// Saturating: a caller cannot overflow the schedule weight by handing over an
/// absurd size, and every file is worth at least one microsecond so that empty
/// files still order deterministically by path.
pub fn cold_weight_micros(bytes: usize) -> u64 {
    let nanos = (bytes as u64).saturating_mul(COLD_NANOS_PER_BYTE);
    (nanos / 1_000).max(1)
}

/// The length of the critical path of a schedule on `workers` workers, in
/// microseconds: the finish time of the last worker under LPT.
///
/// Used by the benchmark and by the tests that assert LPT actually shortens the
/// makespan compared with source order.
pub fn makespan_micros(entries: &[ScheduleEntry], workers: usize) -> u64 {
    lpt_loads(entries.iter().map(|entry| entry.weight_micros), workers).busiest
}

/// How loaded the busiest and the idlest worker end up when `weights`, in
/// the order given, are each handed to the worker that is free soonest.
#[derive(Debug, Clone, Copy)]
struct Loads {
    /// The makespan: when the last worker finishes.
    busiest: u64,
    /// The work the least-loaded worker was handed.
    idlest: u64,
}

/// [`Loads`] for `weights` on `workers` workers.
///
/// A heap of finish times rather than a scan of them, because
/// [`auto_workers`] asks this for several pool sizes over every file in the
/// suite, and a scan made each question as expensive as the core count.
fn lpt_loads(weights: impl Iterator<Item = u64>, workers: usize) -> Loads {
    let workers = workers.max(1);
    let mut finish: BinaryHeap<Reverse<u64>> = (0..workers).map(|_| Reverse(0)).collect();
    for weight in weights {
        if let Some(Reverse(earliest)) = finish.pop() {
            finish.push(Reverse(earliest.saturating_add(weight)));
        }
    }
    let mut loads = Loads {
        busiest: 0,
        idlest: u64::MAX,
    };
    for Reverse(load) in finish {
        loads.busiest = loads.busiest.max(load);
        loads.idlest = loads.idlest.min(load);
    }
    if loads.idlest == u64::MAX {
        loads.idlest = 0;
    }
    loads
}

/// The smallest `workers` in `1..=upper` for which `holds` is true, assuming
/// that once it holds it goes on holding; `upper` when it never does.
fn first_pool_where(upper: usize, holds: impl Fn(usize) -> bool) -> usize {
    let (mut low, mut high) = (1, upper.max(1));
    while low < high {
        let middle = low + (high - low) / 2;
        if holds(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

/// The least a worker's start-up is taken to cost when sizing a pool, in
/// microseconds.
///
/// A recorded start-up is a measurement, and a measurement can come back as
/// zero: a coarse clock, or a host that answered before it had done anything.
/// Dividing work by a start-up of nothing says every core is worth starting for
/// any suite at all — the behaviour sizing exists to replace — so the estimate
/// is never believed below a millisecond.
pub const MIN_WORKER_START_MICROS: u64 = 1_000;

/// How many workers a run should start on `cores` cores, given the schedule it
/// is about to run and what starting one worker cost last time.
///
/// # Why not one per core
///
/// A worker is a process, and starting one is work. A Node worker spends tens of
/// milliseconds booting and loading `@uniflowed/test` before it runs a line of a
/// test, and that time is spent on a core whether or not the worker is ever
/// handed a file. A suite of two-millisecond files is finished by two workers
/// before a third has booted, and on a machine whose cores are partly efficiency
/// cores and partly somebody else's compile the extra workers are not idle —
/// they take the cores the useful ones were running on.
///
/// # The rule
///
/// Two questions, both about the longest-first schedule on a pool of a given
/// size, whose length is [`makespan_micros`]:
///
/// 1. **Which pool keeps every worker busy?** The widest in which the
///    least-loaded worker is still handed a start-up's worth of work. A worker
///    handed less than that spends longer booting than working.
/// 2. **Which pool is worth its workers?** The narrowest that finishes within
///    one start-up of the pool from the first question. Start-ups happen at the
///    same time, so a wider pool is not slower for them; but each one is a
///    core's worth of work, and giving up at most a start-up of wall clock not
///    to do it is what keeps a short suite off every core of a busy machine.
///
/// A suite of many short files stops at two or three. One long file beside
/// many short ones stops at two, because every further worker finishes its
/// share long before the long file does. A suite with seconds of work keeps
/// every core of a laptop, and all but a few of a large machine's.
///
/// The first version of this weighed each worker's start-up against the time
/// that worker saves, and was wrong in the direction that matters: start-ups
/// run side by side, so on sixty-four cores it stopped a twenty-second suite at
/// about twenty workers — a run three times longer than it needed to be.
///
/// # When it does not apply
///
/// It needs durations. With no recorded start-up, or no file in the schedule
/// with a recorded duration, the answer is `cores` — capped at the number of
/// files — exactly as before sizing existed: the size-based weights of a cold
/// schedule rank files against each other and are not times. A file that is new
/// since the last run is costed at the mean of the files that were recorded, so
/// adding one file does not turn a warm suite cold.
pub fn auto_workers(
    schedule: &[ScheduleEntry],
    worker_start_micros: Option<u64>,
    cores: usize,
) -> usize {
    let cap = cores.min(schedule.len()).max(1);
    if cap == 1 {
        return 1;
    }
    let Some(start) = worker_start_micros else {
        return cap;
    };
    let start = start.max(MIN_WORKER_START_MICROS);

    let (recorded_total, recorded) = schedule
        .iter()
        .filter(|entry| entry.basis == ScheduleBasis::Recorded)
        .fold((0u64, 0u64), |(total, count), entry| {
            (total.saturating_add(entry.weight_micros), count + 1)
        });
    if recorded == 0 {
        return cap;
    }
    let mean = recorded_total / recorded;

    let mut weights: Vec<u64> = schedule
        .iter()
        .map(|entry| match entry.basis {
            ScheduleBasis::Recorded => entry.weight_micros,
            ScheduleBasis::Size => mean,
        })
        .collect();
    weights.sort_unstable_by(|a, b| b.cmp(a));
    let loads = |workers: usize| lpt_loads(weights.iter().copied(), workers);

    // The narrowest pool too wide for its idlest worker, less one: the widest
    // in which every worker is handed a start-up's worth of work. Both searches
    // are binary, because a wider pool never hands its idlest worker more nor
    // finishes later — so a suite of a hundred thousand files on sixty-four
    // cores asks for a dozen schedules, not a few thousand.
    let too_wide = first_pool_where(cap + 1, |workers| {
        workers > cap || loads(workers).idlest < start
    });
    let busy = too_wide.saturating_sub(1).max(1);

    let good_enough = loads(busy).busiest.saturating_add(start);
    first_pool_where(busy, |workers| loads(workers).busiest <= good_enough)
}
