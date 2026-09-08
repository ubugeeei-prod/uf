//! The counters, the size classes, and what a delta means.

use super::*;
use crate::tests::exclusive;

#[test]
fn a_size_class_is_the_power_of_two_the_allocation_fits_in() {
    // Bucket 0 is the degenerate pair, then one per doubling. Getting this
    // off by one would put every allocation in the wrong column of a
    // histogram that reads fine.
    assert_eq!(size_class(0), 0);
    assert_eq!(size_class(1), 0);
    assert_eq!(size_class(2), 1);
    assert_eq!(size_class(3), 2);
    assert_eq!(size_class(4), 2);
    assert_eq!(size_class(5), 3);
    assert_eq!(size_class(8), 3);
    assert_eq!(size_class(9), 4);
    assert_eq!(size_class(1024), 10);
    assert_eq!(size_class(1025), 11);
}

#[test]
fn a_size_larger_than_the_last_class_lands_in_it_rather_than_out_of_bounds() {
    // The histogram is indexed by this, so an out-of-range answer is a panic
    // inside a global allocator — which is not a failure anybody could debug.
    assert_eq!(size_class(u64::MAX), SIZE_CLASSES - 1);
    assert_eq!(size_class(1 << 62), SIZE_CLASSES - 1);
}

#[test]
fn a_delta_is_the_difference_and_never_a_wrapped_one() {
    // Counters only rise, but a baseline taken while counting was off can be
    // higher than a later reading of a counter that never moved. Saturating
    // is what keeps that from becoming eighteen quintillion bytes.
    let mut baseline_classes = [0_u64; SIZE_CLASSES];
    baseline_classes[3] = 5;
    let baseline = AllocSnapshot {
        allocations: 10,
        bytes_allocated: 4_096,
        live_bytes: 2_048,
        size_classes: baseline_classes,
        ..AllocSnapshot::default()
    };

    let mut later_classes = [0_u64; SIZE_CLASSES];
    later_classes[3] = 9;
    let later = AllocSnapshot {
        allocations: 25,
        bytes_allocated: 10_240,
        bytes_deallocated: 1_024,
        live_bytes: 8_192,
        peak_live_bytes: 9_216,
        size_classes: later_classes,
        ..AllocSnapshot::default()
    };

    let delta = later.delta_from(&baseline);
    assert_eq!(delta.allocations, 15);
    assert_eq!(delta.bytes_allocated, 6_144);
    assert_eq!(delta.size_classes[3], 4);
    // The peak that matters is how far above where the window *started*, not
    // the process-wide high-water mark.
    assert_eq!(delta.peak_above_baseline, 9_216 - 2_048);
    assert_eq!(delta.live_growth, 8_192 - 2_048);
    // The delta's own retained figure: 6 KiB taken in this window minus the
    // 1 KiB given back in it, not the absolute totals.
    assert_eq!(delta.retained_bytes(), 6_144 - 1_024);

    // And backwards, which must not wrap.
    let backwards = baseline.delta_from(&later);
    assert_eq!(backwards.allocations, 0);
    assert_eq!(backwards.bytes_allocated, 0);
    assert_eq!(backwards.peak_above_baseline, 0);
    // Growth is signed on purpose: a window that freed more than it took is a
    // cache being dropped, and reporting that as zero would hide it.
    assert!(backwards.live_growth < 0, "{backwards:?}");
}

#[test]
fn only_the_classes_that_were_used_are_reported() {
    // A histogram is thirty-two rows and almost all of them are zero; a report
    // that printed them all would bury the two that matter.
    let mut classes = [0_u64; SIZE_CLASSES];
    classes[3] = 7;
    classes[10] = 2;
    let delta = AllocDelta {
        size_classes: classes,
        ..AllocDelta::default()
    };

    let occupied: Vec<(u32, u64)> = delta.occupied_classes().collect();
    assert_eq!(occupied, vec![(3, 7), (10, 2)]);
}

#[test]
fn the_counters_move_only_while_counting_is_on() {
    let _lock = exclusive();

    // Whatever the previous test left, start from off.
    CountingAllocator::disable();
    assert!(!CountingAllocator::is_enabled());
    let quiet = AllocSnapshot::capture();
    let held = Vec::<u8>::with_capacity(64 * 1024);
    let still_quiet = AllocSnapshot::capture();
    drop(held);
    assert_eq!(
        still_quiet.delta_from(&quiet).allocations,
        0,
        "an allocation while disabled must not be counted"
    );

    CountingAllocator::enable();
    assert!(CountingAllocator::is_enabled());
    let before = AllocSnapshot::capture();
    let counted = Vec::<u8>::with_capacity(64 * 1024);
    std::hint::black_box(&counted);
    let after = AllocSnapshot::capture();
    CountingAllocator::disable();

    let delta = after.delta_from(&before);
    assert!(
        delta.allocations >= 1,
        "the allocation was counted: {delta:?}"
    );
    assert!(
        delta.bytes_allocated >= 64 * 1024,
        "its size was counted: {delta:?}"
    );
    drop(counted);
}

#[test]
fn a_counter_measures_from_where_it_started() {
    let _lock = exclusive();
    CountingAllocator::enable();

    let counter = AllocCounter::start();
    let held = Vec::<u8>::with_capacity(32 * 1024);
    std::hint::black_box(&held);
    let delta = counter.delta();
    CountingAllocator::disable();

    assert!(delta.allocations >= 1, "{delta:?}");
    assert!(delta.bytes_allocated >= 32 * 1024, "{delta:?}");
    drop(held);
}

/// A window's peak is the window's, not the biggest thing the process ever did.
///
/// `peak_live_bytes` is a high-water mark, and a mark kept since the process
/// started says the largest thing that ever happened rather than the largest
/// thing that happened here. Subtracting the window's starting live bytes from
/// it does not make it a window figure — it makes it a stale peak with a fresh
/// baseline, which is worse, because it looks like an answer. One heavy
/// iteration used to poison every iteration after it.
#[test]
fn a_window_measures_a_peak_and_a_largest_of_its_own() {
    let _lock = exclusive();
    CountingAllocator::enable();

    // Before the window: something big, then freed. The process-wide marks are
    // now high and live bytes are back down — the exact shape that used to be
    // reported as the next window's peak.
    let heavy = Vec::<u8>::with_capacity(64 * 1024 * 1024);
    std::hint::black_box(&heavy);
    drop(heavy);

    let window = Window::open();
    let before = AllocSnapshot::capture();
    let light = Vec::<u8>::with_capacity(256 * 1024);
    std::hint::black_box(&light);
    let after = AllocSnapshot::capture();
    let delta = after.delta_from(&before);
    drop(light);
    drop(window);
    CountingAllocator::disable();

    // Not to the byte: the harness frees its own odds and ends while this
    // runs, so live bytes dip under the window's own total by a rounding
    // error. The gap this test is about is a factor of 256, not a kilobyte.
    assert!(
        delta.peak_above_baseline >= 200 * 1024,
        "the window's own allocation is in its peak: {delta:?}"
    );
    assert!(
        delta.peak_above_baseline < 16 * 1024 * 1024,
        "the 64 MiB from before the window is not: {delta:?}"
    );
    assert!(
        delta.largest_allocation >= 256 * 1024,
        "the window's own allocation is its largest: {delta:?}"
    );
    assert!(
        delta.largest_allocation < 16 * 1024 * 1024,
        "the 64 MiB from before the window is not: {delta:?}"
    );
}

/// And the marks go back when the window closes, so nesting composes.
///
/// The inner counter measures only its own stretch; the outer one, once the
/// inner has handed the marks back, covers its own stretch *and* the inner's.
/// That is the whole point of restoring with `fetch_max` rather than a store —
/// a store would drop whichever of the two was bigger, and the bigger one is
/// usually the child's.
#[test]
fn a_nested_counter_measures_its_own_stretch_and_the_outer_one_covers_both() {
    let _lock = exclusive();
    CountingAllocator::enable();

    // Again a high mark set before anything opens, so that a counter reading
    // the process-wide peak would be caught doing it.
    let heavy = Vec::<u8>::with_capacity(64 * 1024 * 1024);
    std::hint::black_box(&heavy);
    drop(heavy);

    let outer = AllocCounter::start();
    let small = Vec::<u8>::with_capacity(256 * 1024);
    std::hint::black_box(&small);

    let inner = AllocCounter::start();
    let big = Vec::<u8>::with_capacity(4 * 1024 * 1024);
    std::hint::black_box(&big);
    let inner_delta = inner.delta();
    drop(big);

    let outer_delta = outer.delta();
    drop(small);
    CountingAllocator::disable();

    assert!(
        inner_delta.peak_above_baseline >= 4 * 1000 * 1024,
        "the inner counter saw its own 4 MiB: {inner_delta:?}"
    );
    assert!(
        inner_delta.peak_above_baseline < 16 * 1024 * 1024,
        "and not the 64 MiB from before it opened: {inner_delta:?}"
    );
    assert!(
        outer_delta.peak_above_baseline >= 4 * 1024 * 1024 + 200 * 1024,
        "the outer counter covers the inner's peak as well as its own: \
         {outer_delta:?}"
    );
    assert!(
        outer_delta.peak_above_baseline < 16 * 1024 * 1024,
        "and still not the 64 MiB: {outer_delta:?}"
    );
}
