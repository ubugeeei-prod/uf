//! Spans: what nests, what merges, and what the gates cost.

use std::time::Duration;

use super::*;
use crate::tests::{exclusive, with_spans};

/// Busy-wait rather than sleep: a span has to measure work, and `sleep` on a
/// loaded CI box measures the scheduler.
fn spin(at_least: Duration) {
    let start = Instant::now();
    while start.elapsed() < at_least {
        std::hint::black_box(0_u64);
    }
}

#[test]
fn a_childs_time_is_taken_out_of_its_parents_self_time() {
    // The whole reason for two columns. A parent that spends all its time in
    // one child has no self time, and a profile sorted by self time must not
    // put it at the top.
    let records = with_spans(|| {
        {
            let _outer = ScopeGuard::enter("outer");
            {
                let _inner = ScopeGuard::enter("inner");
                spin(Duration::from_millis(4));
            }
        }
        take_thread_spans()
    });

    let outer = records
        .iter()
        .find(|r| r.name == "outer")
        .expect("outer was recorded");
    let inner = records
        .iter()
        .find(|r| r.name == "inner")
        .expect("inner was recorded");

    assert!(outer.inclusive >= inner.inclusive, "{outer:?} {inner:?}");
    // The parent's self time is what is left after the child, which is very
    // little — but the assertion is the relationship, not a number, because a
    // number here would be a flake.
    assert!(outer.self_time < inner.inclusive, "{outer:?} {inner:?}");
    // A leaf's self time is its inclusive time; nothing was under it.
    assert_eq!(inner.self_time, inner.inclusive);
}

#[test]
fn siblings_both_come_out_of_the_parent() {
    let records = with_spans(|| {
        {
            let _outer = ScopeGuard::enter("outer");
            for _ in 0..2 {
                let _child = ScopeGuard::enter("child");
                spin(Duration::from_millis(2));
            }
        }
        take_thread_spans()
    });

    let outer = records.iter().find(|r| r.name == "outer").expect("outer");
    let child = records.iter().find(|r| r.name == "child").expect("child");
    assert_eq!(child.hits, 2, "two hits merge into one record");
    assert!(
        outer.self_time < child.inclusive,
        "both children came out of the parent"
    );
}

#[test]
fn the_same_name_merges_and_keeps_the_slowest_hit() {
    let records = with_spans(|| {
        {
            let _fast = ScopeGuard::enter("step");
        }
        {
            let _slow = ScopeGuard::enter("step");
            spin(Duration::from_millis(3));
        }
        take_thread_spans()
    });

    assert_eq!(records.len(), 1, "one record for one name: {records:?}");
    let step = &records[0];
    assert_eq!(step.hits, 2);
    // A mean would hide the slow hit, which is the one worth looking at.
    assert!(step.slowest >= Duration::from_millis(3), "{step:?}");
    assert!(step.inclusive >= step.slowest);
    assert_eq!(step.mean(), step.inclusive / 2);
}

#[test]
fn a_span_entered_while_disabled_records_nothing_and_is_not_active() {
    let _lock = exclusive();
    crate::scope::reset_thread_spans();
    crate::scope::disable();

    let guard = ScopeGuard::enter("ignored");
    assert!(!guard.is_active());
    drop(guard);

    assert!(take_thread_spans().is_empty());
}

#[test]
fn the_stack_is_empty_again_once_the_guards_are_dropped() {
    // A leaked frame would attribute every later span's time to it, so this
    // is the invariant the rest of the file rests on.
    with_spans(|| {
        assert_eq!(open_span_depth(), 0);
        {
            let _outer = ScopeGuard::enter("outer");
            assert_eq!(open_span_depth(), 1);
            {
                let _inner = ScopeGuard::enter("inner");
                assert_eq!(open_span_depth(), 2);
            }
            assert_eq!(open_span_depth(), 1);
        }
        assert_eq!(open_span_depth(), 0);
        let _ = take_thread_spans();
    });
}

#[test]
fn the_function_form_returns_what_the_body_returned() {
    let value = with_spans(|| {
        let value = span("work", || 7_u32);
        let _ = take_thread_spans();
        value
    });
    assert_eq!(value, 7);
}

#[test]
fn resetting_drops_records_and_leaves_open_frames_alone() {
    with_spans(|| {
        {
            let _closed = ScopeGuard::enter("closed");
        }
        let _open = ScopeGuard::enter("open");
        reset_thread_spans();
        assert!(
            take_thread_spans().is_empty(),
            "the closed record was dropped"
        );
        // The open frame is still open: silently popping it would hide the
        // caller's bug rather than fix it.
        assert_eq!(open_span_depth(), 1);
        drop(_open);
        let _ = take_thread_spans();
    });
}

#[test]
fn calibration_is_zero_when_spans_are_off_and_positive_when_they_are_on() {
    let _lock = exclusive();
    crate::scope::disable();
    assert_eq!(
        calibrate_overhead_ns(),
        0.0,
        "nothing to calibrate when nothing records"
    );

    crate::scope::reset_thread_spans();
    crate::scope::enable();
    let per_hit = calibrate_overhead_ns();
    assert!(per_hit > 0.0, "an enabled guard costs something: {per_hit}");
    // The measurement's own cost must be small next to anything worth
    // measuring. A guard slower than a microsecond would distort every detail
    // span it was put in.
    assert!(
        per_hit < 1_000.0,
        "a guard should not cost a microsecond: {per_hit} ns"
    );
    // And it must not leave its own records behind for a real window to find.
    assert!(
        take_thread_spans()
            .iter()
            .all(|r| r.name != "uf_profiler::calibration"),
        "calibration leaked into the records"
    );
    crate::scope::disable();
}
