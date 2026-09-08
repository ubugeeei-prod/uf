//! Hierarchical timing spans, thread-local and lock-free.
//!
//! A span is opened by [`ScopeGuard::enter`] and closed when the guard drops.
//! Each open frame remembers how much time its children have taken, so on
//! close it can report two numbers that answer different questions:
//!
//! * **inclusive** — wall clock between enter and drop. "How long does
//!   `uf check` spend in inference?"
//! * **self** — inclusive minus the children's inclusive. "How much of that is
//!   inference itself rather than the parser it calls?"
//!
//! A profile with only the first is a list of everything the program does,
//! sorted by how much of the program each thing contains, and the root is
//! always at the top. Self time is the column that names the thing to fix.
//!
//! # Why thread-local
//!
//! `uf_infra::parallel` fans work out across threads and uf's test runner has
//! a worker pool. A shared span stack would need a lock on the hot path, and a
//! lock on the hot path is a measurement that changes what it measures. Each
//! thread keeps its own stack and its own records; a caller that wants the
//! whole picture drains each thread.
//!
//! # The two gates
//!
//! [`enable`] turns spans on at all. [`enable_detail`] additionally turns on
//! the ones sitting inside per-node or per-token loops, where the guard costs
//! something comparable to the work it brackets. Phase spans stay cheap and
//! honest by default; a `--detail` run trades that accuracy for the full
//! trace, and [`calibrate_overhead_ns`] exists so a report can say how much of
//! what it is showing is the measurement itself.

use std::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::alloc::{AllocCounter, AllocDelta};

static ENABLED: AtomicBool = AtomicBool::new(false);
static DETAIL: AtomicBool = AtomicBool::new(false);

/// Record spans from now on.
pub fn enable() {
    ENABLED.store(true, Ordering::Release);
}

/// Stop recording spans. Guards already open still close; they record nothing.
pub fn disable() {
    ENABLED.store(false, Ordering::Release);
}

/// Whether spans are being recorded.
#[must_use]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// Also record detail spans. No effect unless [`enable`] is on too.
pub fn enable_detail() {
    DETAIL.store(true, Ordering::Release);
}

/// Stop recording detail spans. Phase spans keep recording.
pub fn disable_detail() {
    DETAIL.store(false, Ordering::Release);
}

/// Whether detail spans are being recorded, which needs both gates.
#[must_use]
pub fn is_detail_enabled() -> bool {
    is_enabled() && DETAIL.load(Ordering::Acquire)
}

thread_local! {
    static STATE: RefCell<SpanState> = const { RefCell::new(SpanState::new()) };
}

/// One thread's spans: what is open, and what has closed.
struct SpanState {
    stack: Vec<Frame>,
    records: Vec<ScopeRecord>,
}

impl SpanState {
    const fn new() -> Self {
        Self {
            stack: Vec::new(),
            records: Vec::new(),
        }
    }
}

struct Frame {
    name: &'static str,
    start: Instant,
    /// The inclusive time of every child that has closed under this frame.
    child_time: Duration,
    /// [`None`] when the counting allocator is not counting.
    ///
    /// Reading the counters costs 11.0 ns a span pair, and against a 44.4 ns
    /// guard that is a quarter of it — for four numbers that are all zero when
    /// nothing is counting them. A binary that never installed
    /// [`crate::CountingAllocator`] should not pay for the columns it cannot
    /// fill.
    allocations: Option<AllocCounter>,
}

/// An open span. Closes when dropped.
#[must_use = "a span that is not bound closes immediately and measures nothing"]
pub struct ScopeGuard {
    /// Whether a frame was pushed. False when the gate was shut at `enter`,
    /// which is the case that has to cost nothing.
    active: bool,
}

impl ScopeGuard {
    /// Open a span.
    ///
    /// The name is `'static` so a record can key on it without allocating —
    /// a profiler that allocated per span would be measuring itself.
    #[inline]
    pub fn enter(name: &'static str) -> Self {
        if !is_enabled() {
            return Self { active: false };
        }
        let pushed = STATE.with(|cell| {
            // `try_borrow_mut`, not `borrow_mut`: a `Drop` running inside this
            // thread could re-enter, and a profiler that panics is worse than
            // one that misses a span.
            let Ok(mut state) = cell.try_borrow_mut() else {
                return false;
            };
            state.stack.push(Frame {
                name,
                start: Instant::now(),
                child_time: Duration::ZERO,
                allocations: crate::CountingAllocator::is_enabled().then(AllocCounter::start),
            });
            true
        });
        Self { active: pushed }
    }

    /// Open a detail span — one inside a per-node or per-token loop.
    ///
    /// Records only when both gates are open. See the module comment.
    #[inline]
    pub fn enter_detail(name: &'static str) -> Self {
        if !is_detail_enabled() {
            return Self { active: false };
        }
        Self::enter(name)
    }

    /// Whether this guard is recording.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        STATE.with(|cell| {
            let Ok(mut state) = cell.try_borrow_mut() else {
                return;
            };
            let Some(frame) = state.stack.pop() else {
                return;
            };
            let inclusive = frame.start.elapsed();
            // Saturating: a clock that went backwards, or a child whose frame
            // outlived a parent that was force-popped, must not underflow into
            // a self time of nine hours.
            let self_time = inclusive.saturating_sub(frame.child_time);
            let allocations = frame
                .allocations
                .map(|counter| counter.delta())
                .unwrap_or_default();

            // Hand this span's inclusive time to whoever is above, so that
            // when *they* close, their self time excludes it.
            if let Some(parent) = state.stack.last_mut() {
                parent.child_time += inclusive;
            }

            merge(
                &mut state.records,
                frame.name,
                inclusive,
                self_time,
                &allocations,
            );
        });
    }
}

/// Fold one closed span into the records, by name.
///
/// A linear scan rather than a map. A profile has tens of distinct span names,
/// not thousands, and a `FxHashMap` here would allocate on the path this file
/// exists to keep cheap.
fn merge(
    records: &mut Vec<ScopeRecord>,
    name: &'static str,
    inclusive: Duration,
    self_time: Duration,
    allocations: &AllocDelta,
) {
    if let Some(record) = records.iter_mut().find(|record| record.name == name) {
        record.hits += 1;
        record.inclusive += inclusive;
        record.self_time += self_time;
        record.allocations += allocations.allocations;
        record.bytes_allocated += allocations.bytes_allocated;
        record.peak_above_baseline = record
            .peak_above_baseline
            .max(allocations.peak_above_baseline);
        record.slowest = record.slowest.max(inclusive);
        return;
    }
    records.push(ScopeRecord {
        name,
        hits: 1,
        inclusive,
        self_time,
        allocations: allocations.allocations,
        bytes_allocated: allocations.bytes_allocated,
        peak_above_baseline: allocations.peak_above_baseline,
        slowest: inclusive,
    });
}

/// Every hit of one span name, added up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeRecord {
    pub name: &'static str,
    pub hits: u64,
    /// Wall clock inside the span, children included.
    pub inclusive: Duration,
    /// [`Self::inclusive`] minus what its children took.
    pub self_time: Duration,
    pub allocations: u64,
    pub bytes_allocated: u64,
    pub peak_above_baseline: u64,
    /// The slowest single hit, which is what a tail-latency question asks for
    /// and an average hides.
    pub slowest: Duration,
}

impl ScopeRecord {
    /// Mean inclusive time per hit.
    ///
    /// Divided in nanoseconds rather than with `Duration::checked_div`, which
    /// takes a `u32`. `hits` is a `u64`, and a span in a per-node loop is
    /// exactly the kind that gets hit more than four billion times — at which
    /// point the cast truncates, and at exactly `u32::MAX + 1` it truncates to
    /// *zero*, so `checked_div` returns `None` and the mean of a span that ran
    /// for hours reads as zero.
    #[must_use]
    pub fn mean(&self) -> Duration {
        if self.hits == 0 {
            return Duration::ZERO;
        }
        let nanos = self.inclusive.as_nanos() / u128::from(self.hits);
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }

    /// Bytes allocated per hit, which is the number a "does this allocate per
    /// node" question is really asking.
    #[must_use]
    pub fn bytes_per_hit(&self) -> u64 {
        self.bytes_allocated.checked_div(self.hits).unwrap_or(0)
    }
}

/// Take this thread's records, leaving it empty.
#[must_use]
pub fn take_thread_spans() -> Vec<ScopeRecord> {
    STATE.with(|cell| {
        let Ok(mut state) = cell.try_borrow_mut() else {
            return Vec::new();
        };
        std::mem::take(&mut state.records)
    })
}

/// Records handed over by threads that are about to end.
///
/// Uncontended in practice: a thread touches this once, when it is finished,
/// and never on the hot path. The lock is what makes it correct anyway.
static COLLECTED: Mutex<Vec<ScopeRecord>> = Mutex::new(Vec::new());

/// Hand this thread's records to the collector and clear them.
///
/// Spans are thread-local so the hot path needs no lock, which means a worker
/// thread's spans die with it unless it says otherwise. Most of uf's work
/// happens on such a thread — `uf_fmt::format_source` runs the formatter on
/// one with a bigger stack, `uf_infra::parallel` fans out across a pool, and
/// `uf_test` has a pool of its own — so without this the profiler could
/// profile almost nothing uf actually does.
///
/// Call it at the end of the work, on the thread that did it.
pub fn flush_thread_spans() {
    let records = take_thread_spans();
    if records.is_empty() {
        return;
    }
    let Ok(mut collected) = COLLECTED.lock() else {
        // A poisoned lock means some other thread panicked mid-flush. Losing
        // a profile is not worth a second panic on top of the first.
        return;
    };
    for record in records {
        fold(&mut collected, record);
    }
}

/// Fold a whole record into `records`, by name.
///
/// The counterpart to [`merge`], which folds one closed span. A record
/// arriving from another thread already carries its own hit count, so adding
/// it as a single hit would undercount every span a worker entered twice.
fn fold(records: &mut Vec<ScopeRecord>, record: ScopeRecord) {
    if let Some(slot) = records.iter_mut().find(|slot| slot.name == record.name) {
        slot.hits += record.hits;
        slot.inclusive += record.inclusive;
        slot.self_time += record.self_time;
        slot.allocations += record.allocations;
        slot.bytes_allocated += record.bytes_allocated;
        slot.peak_above_baseline = slot.peak_above_baseline.max(record.peak_above_baseline);
        slot.slowest = slot.slowest.max(record.slowest);
        return;
    }
    records.push(record);
}

/// Take everything every thread has flushed.
#[must_use]
pub fn take_collected_spans() -> Vec<ScopeRecord> {
    COLLECTED
        .lock()
        .map(|mut collected| std::mem::take(&mut *collected))
        .unwrap_or_default()
}

/// Drop everything flushed, without reading it.
pub fn reset_collected_spans() {
    if let Ok(mut collected) = COLLECTED.lock() {
        collected.clear();
    }
}

/// Drop this thread's records without reading them.
///
/// Open frames are left alone: a caller that starts a window with a span still
/// open has a bug this cannot fix, and silently popping it would hide it.
pub fn reset_thread_spans() {
    STATE.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            state.records.clear();
        }
    });
}

/// How deep the open stack is, which a test can assert is zero.
#[must_use]
pub fn open_span_depth() -> usize {
    STATE.with(|cell| cell.try_borrow().map_or(0, |state| state.stack.len()))
}

/// Run `f` inside a span, for callers where a guard binding is awkward.
pub fn span<R>(name: &'static str, f: impl FnOnce() -> R) -> R {
    let _guard = ScopeGuard::enter(name);
    f()
}

/// Roughly what one enabled enter/drop pair costs, in nanoseconds.
///
/// With detail spans on, the guard costs something comparable to the work in
/// the smallest of them, so a report that did not say so would be reporting
/// its own overhead as the program's. Multiply this by a row's hit count to
/// see how much of that row is the measurement.
///
/// Returns `0.0` when spans are off, so the result can be handed straight to a
/// report config. The fastest batch of several is taken rather than the mean:
/// the slow ones are scheduler noise, and the floor is the number that
/// generalises.
#[must_use]
pub fn calibrate_overhead_ns() -> f64 {
    const BATCH: u32 = 1024;
    const ROUNDS: usize = 8;

    if !is_enabled() {
        return 0.0;
    }
    let mut fastest = f64::INFINITY;
    for _ in 0..ROUNDS {
        let start = Instant::now();
        for _ in 0..BATCH {
            let _guard = ScopeGuard::enter("uf_profiler::calibration");
        }
        let per_hit = start.elapsed().as_nanos() as f64 / f64::from(BATCH);
        fastest = fastest.min(per_hit);
    }
    // The calibration's own records must not reach a real window.
    STATE.with(|cell| {
        if let Ok(mut state) = cell.try_borrow_mut() {
            state
                .records
                .retain(|record| record.name != "uf_profiler::calibration");
        }
    });
    if fastest.is_finite() { fastest } else { 0.0 }
}

#[cfg(test)]
mod tests;
