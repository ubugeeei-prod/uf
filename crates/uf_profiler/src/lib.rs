//! Where uf's time and memory go.
//!
//! uf has thirteen criterion benchmarks and they answer one question: is this
//! faster than it was. They do not answer the next one, which is the only one
//! that leads anywhere — *where* did it go. `docs/roadmap.md` asks for two
//! things that cannot be checked without an answer to it:
//!
//! > Ban `String`, `format!`, and allocation-heavy std helpers in
//! > parser/lint/router/test hot paths.
//!
//! > Audit hot paths for unnecessary `.clone()` calls and replace them with
//! > borrowed or arena-backed flows.
//!
//! Neither is a wall-clock question. A `String` per AST node might cost two
//! percent on a benchmark and be the whole reason a formatter allocates a
//! million times over a corpus; a benchmark says the run got slower and the
//! profile says which line did it.
//!
//! Three layers, each usable alone:
//!
//! 1. [`CountingAllocator`] — a `#[global_allocator]` over `System` that
//!    tallies allocations, bytes, peak live bytes and a power-of-two size
//!    histogram. [`AllocCounter`] scopes a measurement.
//! 2. [`ScopeGuard`] and [`profile_span!`] — thread-local hierarchical spans
//!    with self/inclusive time and per-span allocation deltas, behind two
//!    gates so the hot path pays a relaxed atomic load when profiling is off.
//! 3. [`Report`] — the rows sorted by the column that names the thing to fix,
//!    as a table or as JSON.
//!
//! [`Recorder`] composes them: wrap a workload, get a report whose span
//! timings and allocator deltas came from the same window.
//!
//! # Example
//!
//! ```
//! use uf_profiler::{Recorder, profile_span, scope};
//!
//! scope::enable();
//! let mut recorder = Recorder::new("parse");
//! recorder.record(|| {
//!     profile_span!("tokenise");
//!     std::hint::black_box(0);
//! });
//! let report = recorder.finish();
//! assert_eq!(report.iterations, 1);
//! scope::disable();
//! ```
//!
//! # What it costs when it is off
//!
//! One `Acquire` load of a `static AtomicBool` per span, and a guard that
//! holds a `bool`. Nothing allocates, nothing locks, and the branch predicts
//! perfectly because the answer never changes during a run. That is the price
//! of leaving `profile_span!` in a hot path permanently, which is the point:
//! a profiler you have to add before you can use it is one that is never
//! there when the question arrives.

// The profiler divides durations by counts, turns byte totals into MiB and
// scales a calibration into nanoseconds. None of those need bit-exact
// preservation — they are observed counters on their way to a report — and
// spelling each as a checked conversion would bury the arithmetic.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

pub mod alloc;
pub mod report;
pub mod scope;

pub use alloc::{AllocCounter, AllocDelta, AllocSnapshot, CountingAllocator};
pub use report::{IterationRecord, Report, ReportConfig, SortBy, SpanAggregate};
pub use scope::{ScopeGuard, ScopeRecord};

/// Runs a workload and collects what it cost.
///
/// One per logical thing being measured. Feed it iterations with
/// [`Recorder::record`], then [`Recorder::finish`].
pub struct Recorder {
    label: String,
    iterations: Vec<IterationRecord>,
    config: ReportConfig,
}

impl Recorder {
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            iterations: Vec::new(),
            config: ReportConfig::default(),
        }
    }

    #[must_use]
    pub fn with_config(mut self, config: ReportConfig) -> Self {
        self.config = config;
        self
    }

    /// Run one iteration, capturing its spans and its allocations.
    ///
    /// Returns what the closure returned so a caller can `black_box` it — a
    /// workload whose result is dropped is a workload the optimiser is
    /// entitled to delete.
    pub fn record<R>(&mut self, mut workload: impl FnMut() -> R) -> R {
        // Records first: anything left from a previous window is not this
        // one's, and a span from before the baseline would be attributed
        // here with allocations it did not make.
        scope::reset_thread_spans();
        let before = AllocSnapshot::capture();
        let start = std::time::Instant::now();
        let output = workload();
        let elapsed = start.elapsed();
        let after = AllocSnapshot::capture();
        self.iterations.push(IterationRecord {
            elapsed,
            allocations: after.delta_from(&before),
            spans: scope::take_thread_spans(),
        });
        output
    }

    /// How many iterations have been recorded.
    #[must_use]
    pub fn iterations(&self) -> usize {
        self.iterations.len()
    }

    /// Consume the recorder and produce the [`Report`].
    #[must_use]
    pub fn finish(self) -> Report {
        Report::from_iterations(self.label, self.iterations, self.config)
    }
}

/// Open a span for the rest of the enclosing block.
///
/// ```
/// # use uf_profiler::{profile_span, scope};
/// # scope::enable();
/// fn parse() {
///     profile_span!("parse");
///     // …
/// }
/// # parse();
/// # scope::disable();
/// ```
///
/// `detail:` marks a span inside a per-node or per-token loop, which records
/// only when [`scope::enable_detail`] is on as well — see that function for
/// why the two are separate.
///
/// The binding is named rather than `_`, because `let _ = guard` drops it
/// immediately and would measure nothing at all.
#[macro_export]
macro_rules! profile_span {
    (detail: $name:literal) => {
        let __uf_profile_span = $crate::ScopeGuard::enter_detail($name);
    };
    ($name:literal) => {
        let __uf_profile_span = $crate::ScopeGuard::enter($name);
    };
}

/// The counting allocator, installed for this crate's own test binary.
///
/// Without it `alloc`'s tests would measure `System` through a wrapper nobody
/// installed, every counter would read zero, and the suite would pass by
/// measuring nothing — which is the failure a profiler is least able to notice
/// about itself.
#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: CountingAllocator = CountingAllocator::new();

#[cfg(test)]
mod tests;
