use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn cores(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).expect("a positive core count")
}

/// The contract, stated as a test: the parallel result is the serial result.
#[test]
fn a_parallel_map_is_the_serial_map() {
    let items = (0..500u64).collect::<Vec<_>>();
    let double = |item: &u64| Ok::<_, ()>(item * 2);

    let serial = items.iter().map(double).collect::<Result<Vec<_>, ()>>();
    let parallel = map_with_cores(&items, cores(8), double);

    assert_eq!(parallel, serial);
}

#[test]
fn results_come_back_in_input_order_however_the_work_interleaved() {
    // Reversing the cost makes the first items the slowest, so a scheduler
    // that returned completion order would return something close to reversed.
    let items = (0..200usize).collect::<Vec<_>>();

    let out = map_with_cores(&items, cores(8), |item| {
        for _ in 0..(200 - item) * 50 {
            std::hint::black_box(item);
        }
        Ok::<_, ()>(*item)
    })
    .expect("nothing fails");

    assert_eq!(out, items);
}

#[test]
fn every_item_is_visited_exactly_once() {
    let items = (0..1000usize).collect::<Vec<_>>();
    let calls = AtomicUsize::new(0);

    let out = map_with_cores(&items, cores(8), |item| {
        calls.fetch_add(1, Ordering::Relaxed);
        Ok::<_, ()>(*item)
    })
    .expect("nothing fails");

    assert_eq!(calls.load(Ordering::Relaxed), items.len());
    assert_eq!(out.len(), items.len());
}

#[test]
fn an_empty_slice_is_an_empty_result() {
    let items: [u8; 0] = [];

    assert_eq!(map(&items, |item: &u8| Ok::<_, ()>(*item)), Ok(Vec::new()));
}

#[test]
fn a_single_item_does_not_start_a_thread() {
    assert_eq!(threads_for(1, cores(16)), 1);
    assert_eq!(map(&[7u8], |item| Ok::<_, ()>(*item * 2)), Ok(vec![14]));
}

/// The reported error must not depend on which thread happened to finish first.
#[test]
fn the_lowest_indexed_error_is_the_one_reported() {
    let items = (0..500usize).collect::<Vec<_>>();

    for _ in 0..32 {
        let out = map_with_cores(&items, cores(8), |item| {
            if *item == 100 || *item == 300 {
                Err(*item)
            } else {
                Ok(*item)
            }
        });

        assert_eq!(out, Err(100));
    }
}

/// An error far enough along that the other threads stop before claiming the
/// indices after it. Those unclaimed slots must not be mistaken for a bug.
#[test]
fn an_error_leaves_later_items_unclaimed_without_panicking() {
    let items = (0..5000usize).collect::<Vec<_>>();

    let out = map_with_cores(&items, cores(8), |item| {
        if *item == 10 {
            Err("failed")
        } else {
            Ok(*item)
        }
    });

    assert_eq!(out, Err("failed"));
}

/// Once something fails, the remaining work is abandoned rather than run.
#[test]
fn a_failure_stops_the_threads_claiming_more_work() {
    let items = (0..100_000usize).collect::<Vec<_>>();
    let calls = AtomicUsize::new(0);

    let out = map_with_cores(&items, cores(8), |item| {
        calls.fetch_add(1, Ordering::Relaxed);
        if *item == 0 { Err(()) } else { Ok(*item) }
    });

    assert_eq!(out, Err(()));
    assert!(
        calls.load(Ordering::Relaxed) < items.len(),
        "every one of {} items ran even though the first failed",
        items.len()
    );
}

// --- the scheduler -----------------------------------------------------

#[test]
fn small_inputs_stay_on_the_calling_thread() {
    for length in 0..SERIAL_THRESHOLD {
        assert_eq!(
            threads_for(length, cores(16)),
            1,
            "{length} items is not worth a thread"
        );
    }
    assert!(threads_for(SERIAL_THRESHOLD, cores(16)) > 1);
}

#[test]
fn there_are_never_more_threads_than_work_or_cores() {
    assert_eq!(threads_for(10, cores(16)), 10, "no idle threads");
    assert_eq!(threads_for(1000, cores(4)), 4, "no oversubscription");
    assert_eq!(threads_for(1000, cores(1)), 1);
}

#[test]
fn one_core_is_the_serial_path() {
    let items = (0..100u32).collect::<Vec<_>>();

    let out = map_with_cores(&items, cores(1), |item| Ok::<_, ()>(*item + 1));

    assert_eq!(out, Ok((1..101u32).collect::<Vec<_>>()));
}

// --- ordering of collected results -------------------------------------

#[test]
fn collecting_places_results_by_index_rather_than_by_arrival() {
    let claimed = vec![
        (2usize, Ok::<&str, ()>("third")),
        (0, Ok("first")),
        (1, Ok("second")),
    ];

    assert_eq!(
        collect_in_order(claimed, 3),
        Ok(vec!["first", "second", "third"])
    );
}

#[test]
fn collecting_reports_the_lowest_indexed_error_past_an_unclaimed_slot() {
    // Index 1 was never claimed; index 2 failed. The unclaimed slot must not
    // shadow the error.
    let claimed = vec![(0usize, Ok::<u8, &str>(1)), (2, Err("boom"))];

    assert_eq!(collect_in_order(claimed, 4), Err("boom"));
}

#[test]
fn a_panicking_body_propagates() {
    let items = (0..64usize).collect::<Vec<_>>();

    let panicked = std::panic::catch_unwind(|| {
        map_with_cores(&items, cores(4), |item| {
            assert_ne!(*item, 32, "deliberate");
            Ok::<_, ()>(*item)
        })
    });

    assert!(
        panicked.is_err(),
        "a panic in the body must not be swallowed"
    );
}
