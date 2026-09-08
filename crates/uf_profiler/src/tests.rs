//! The three layers together, and the lock that lets them be tested at all.

use std::sync::{Mutex, MutexGuard};

use crate::{Recorder, scope};

/// Serialises every test that touches the global gates or the allocator
/// counters.
///
/// Both are process-wide by construction — an allocator is — so two tests
/// running at once would read each other's numbers. `cargo test` runs them on
/// as many threads as it has cores, so without this the suite is a race that
/// passes on a laptop and fails on CI.
static EXCLUSIVE: Mutex<()> = Mutex::new(());

/// Take the lock, surviving a previous test that panicked while holding it.
pub(crate) fn exclusive() -> MutexGuard<'static, ()> {
    EXCLUSIVE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Enable spans for the body, and put the gates back however the body ends.
pub(crate) fn with_spans<R>(body: impl FnOnce() -> R) -> R {
    let _lock = exclusive();
    scope::reset_thread_spans();
    scope::enable();
    let result = body();
    scope::disable();
    scope::disable_detail();
    scope::reset_thread_spans();
    result
}

#[test]
fn a_recorder_reports_one_iteration_per_record() {
    let report = with_spans(|| {
        let mut recorder = Recorder::new("workload");
        for _ in 0..3 {
            recorder.record(|| {
                profile_span!("step");
                std::hint::black_box(1_u32)
            });
        }
        assert_eq!(recorder.iterations(), 3);
        recorder.finish()
    });

    assert_eq!(report.iterations, 3);
    assert_eq!(report.elapsed.len(), 3);
    let step = report
        .spans
        .iter()
        .find(|span| span.name == "step")
        .expect("the span was recorded");
    // One hit per iteration, because the records are drained between them.
    assert_eq!(step.hits, 3);
}

#[test]
fn a_workloads_result_reaches_the_caller() {
    // Not decoration: a workload whose result is dropped is one the optimiser
    // may delete, and then the measurement is of nothing.
    let value = with_spans(|| {
        let mut recorder = Recorder::new("workload");
        recorder.record(|| 41_u32 + 1)
    });
    assert_eq!(value, 42);
}

#[test]
fn a_report_with_no_spans_says_so_rather_than_drawing_an_empty_table() {
    let _lock = exclusive();
    scope::disable();
    let mut recorder = Recorder::new("nothing");
    recorder.record(|| {
        profile_span!("never recorded");
    });
    let table = recorder.finish().render_table();
    assert!(table.contains("no spans recorded"), "{table}");
    assert!(table.contains("scope::enable()"), "{table}");
}

#[test]
fn the_detail_gate_needs_both_switches() {
    let _lock = exclusive();
    scope::reset_thread_spans();

    // Spans on, detail off: the detail span records nothing.
    scope::enable();
    {
        profile_span!(detail: "micro");
    }
    assert!(scope::take_thread_spans().is_empty());

    // Detail on but spans off: still nothing, because detail is an addition
    // to the first gate rather than a second way in.
    scope::disable();
    scope::enable_detail();
    {
        profile_span!(detail: "micro");
    }
    assert!(scope::take_thread_spans().is_empty());

    // Both on.
    scope::enable();
    {
        profile_span!(detail: "micro");
    }
    let recorded = scope::take_thread_spans();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].name, "micro");

    scope::disable();
    scope::disable_detail();
}

/// A worker thread's spans reach the report, once it says so.
///
/// Spans are thread-local so the hot path needs no lock, which means a
/// worker's records die with it by default. Most of what uf does happens on
/// such a thread — `uf_fmt::format_source` runs the formatter on one with a
/// bigger stack, `uf_infra::parallel` fans out across a pool, `uf_test` has a
/// pool of its own — so a profiler that could not see them could profile
/// almost nothing the toolchain actually does.
#[test]
fn a_worker_threads_spans_reach_the_report_when_it_flushes() {
    let report = with_spans(|| {
        let mut recorder = Recorder::new("fans out");
        recorder.record(|| {
            profile_span!("on the caller");
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    {
                        profile_span!("on the worker");
                    }
                    {
                        profile_span!("on the worker");
                    }
                    // Without this the two records above die with the thread.
                    scope::flush_thread_spans();
                });
            });
        });
        recorder.finish()
    });

    let names: Vec<&str> = report.spans.iter().map(|span| span.name).collect();
    assert!(names.contains(&"on the caller"), "{names:?}");
    assert!(names.contains(&"on the worker"), "{names:?}");
    // Two hits, folded rather than counted as one: a record arriving from
    // another thread already carries its own count.
    let worker = report
        .spans
        .iter()
        .find(|span| span.name == "on the worker")
        .expect("worker span");
    assert_eq!(worker.hits, 2, "{worker:?}");
}

/// A worker that does not flush loses its spans, and the caller's survive.
#[test]
fn a_worker_that_does_not_flush_leaves_nothing_behind() {
    // The other half of the contract: flushing is explicit, so this says what
    // happens without it rather than leaving it to be discovered.
    let report = with_spans(|| {
        let mut recorder = Recorder::new("fans out");
        recorder.record(|| {
            profile_span!("on the caller");
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    profile_span!("on the worker");
                });
            });
        });
        recorder.finish()
    });

    let names: Vec<&str> = report.spans.iter().map(|span| span.name).collect();
    assert!(names.contains(&"on the caller"), "{names:?}");
    assert!(!names.contains(&"on the worker"), "{names:?}");
}
