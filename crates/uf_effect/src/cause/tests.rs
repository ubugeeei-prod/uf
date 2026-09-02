use super::*;

fn fail(error: &str) -> Cause<String> {
    Cause::fail(error.to_string())
}

#[test]
fn empty_is_the_identity_for_sequential_composition() {
    let cause = fail("boom");

    assert_eq!(cause.clone().then(Cause::Empty), cause);
    assert_eq!(Cause::Empty.then(cause.clone()), cause);
    assert_eq!(
        Cause::<String>::Empty.then(Cause::Empty),
        Cause::<String>::Empty
    );
}

#[test]
fn empty_is_the_identity_for_parallel_composition() {
    let cause = fail("boom");

    assert_eq!(cause.clone().both(Cause::Empty), cause);
    assert_eq!(Cause::Empty.both(cause.clone()), cause);
}

#[test]
fn sequential_composition_is_associative() {
    let a = fail("a");
    let b = fail("b");
    let c = fail("c");

    let left = a.clone().then(b.clone()).then(c.clone());
    let right = a.then(b.then(c));

    // Flattening from both sides makes this exact rather than merely
    // equivalent: the two are the same value, not two shapes that happen to
    // record the same failures.
    assert_eq!(left, right);
    assert_eq!(
        left,
        Cause::Sequential(vec![fail("a"), fail("b"), fail("c")])
    );
}

#[test]
fn parallel_composition_is_associative() {
    let a = fail("a");
    let b = fail("b");
    let c = fail("c");

    assert_eq!(a.clone().both(b.clone()).both(c.clone()), a.both(b.both(c)));
}

#[test]
fn parallel_composition_records_both_sides() {
    let cause = fail("left").both(fail("right"));

    let failures = cause.failures();
    assert_eq!(failures.len(), 2);
    assert_eq!(failures[0], "left");
    assert_eq!(failures[1], "right");
}

#[test]
fn sequential_and_parallel_stay_distinguishable() {
    let sequential = fail("a").then(fail("b"));
    let parallel = fail("a").both(fail("b"));

    assert_ne!(sequential, parallel);
    assert!(matches!(sequential, Cause::Sequential(_)));
    assert!(matches!(parallel, Cause::Parallel(_)));
}

#[test]
fn a_defect_is_not_a_declared_failure() {
    let cause = Cause::<String>::die("index out of bounds");

    assert!(cause.has_defect());
    assert!(cause.failures().is_empty());
    assert_eq!(cause.defects().len(), 1);
    assert_eq!(cause.defects()[0].message, "index out of bounds");
}

#[test]
fn a_declared_failure_is_not_a_defect() {
    let cause = fail("not found");

    assert!(!cause.has_defect());
    assert_eq!(cause.failures().len(), 1);
    assert!(cause.defects().is_empty());
}

#[test]
fn a_defect_anywhere_in_the_tree_is_found() {
    let cause = fail("a").then(fail("b").both(Cause::die("bug")));

    assert!(cause.has_defect());
}

#[test]
fn interruption_alone_is_recognized() {
    assert!(Cause::<String>::Interrupt.is_interrupted_only());
    assert!(
        Cause::<String>::Interrupt
            .then(Cause::Interrupt)
            .is_interrupted_only()
    );
    assert!(
        Cause::<String>::Interrupt
            .both(Cause::Interrupt)
            .is_interrupted_only()
    );
}

#[test]
fn interruption_mixed_with_failure_is_not_interruption_only() {
    // A supervisor that cannot tell these apart retries work the user cancelled.
    assert!(!Cause::Interrupt.then(fail("boom")).is_interrupted_only());
    assert!(!fail("boom").both(Cause::Interrupt).is_interrupted_only());
    assert!(
        !Cause::<String>::Interrupt
            .then(Cause::die("bug"))
            .is_interrupted_only()
    );
}

#[test]
fn an_empty_cause_is_not_interruption() {
    assert!(!Cause::<String>::Empty.is_interrupted_only());
}

#[test]
fn mapping_rewrites_errors_and_keeps_the_shape() {
    let cause = fail("a").then(fail("b").both(Cause::die("bug")));

    let mapped = cause.map(&mut |error| error.len());

    assert_eq!(
        mapped.failures().into_iter().copied().collect::<Vec<_>>(),
        vec![1, 1]
    );
    assert_eq!(mapped.defects().len(), 1);
    assert!(matches!(mapped, Cause::Sequential(_)));
}

#[test]
fn mapping_preserves_interruption_and_defects() {
    let cause = Cause::<String>::Interrupt.both(Cause::die("bug"));

    let mapped = cause.map(&mut |_| 0u8);

    assert!(mapped.is_interrupted_only() || mapped.has_defect());
    assert_eq!(mapped.defects()[0].message, "bug");
}

#[test]
fn visiting_reaches_every_node_left_to_right() {
    let cause = fail("a").then(fail("b")).both(fail("c"));

    let mut seen = Vec::new();
    cause.visit(&mut |node| {
        if let Cause::Fail(error) = node {
            seen.push(error.clone());
        }
    });

    assert_eq!(seen, vec!["a", "b", "c"]);
}

#[test]
fn folding_counts_every_node() {
    let cause = fail("a").then(fail("b"));

    // One `Sequential` plus two `Fail`.
    assert_eq!(cause.len(), 3);
}

/// A hundred thousand failures is a wide cause, not a deep one.
///
/// `Effect.all` over a large collection that fails produces exactly this, and
/// with a binary tree the *reporting* of that failure would overflow the stack —
/// on clone, on comparison, and on drop, none of which an iterative walker
/// helps with. Flattening is what makes it safe.
#[test]
fn a_hundred_thousand_failures_stay_shallow() {
    let mut cause = fail("root");
    for index in 0..100_000 {
        cause = cause.then(fail(&format!("cleanup {index}")));
    }

    assert_eq!(cause.failures().len(), 100_001);
    assert!(!cause.has_defect());
    assert!(!cause.is_interrupted_only());

    // The operations a binary shape would have blown the stack on.
    let cloned = cause.clone();
    assert_eq!(cloned, cause);
    let mapped = cause.map(&mut |error| error.len());
    assert_eq!(mapped.failures().len(), 100_001);
}

#[test]
fn a_wide_parallel_cause_stays_shallow_too() {
    let mut cause = fail("root");
    for _ in 0..50_000 {
        cause = cause.both(fail("concurrent"));
    }

    assert_eq!(cause.failures().len(), 50_001);
    assert_eq!(cause.len(), 50_002, "one Parallel node plus its children");
}

#[test]
fn nesting_sequential_inside_parallel_keeps_both_shapes() {
    let cause = fail("a").then(fail("b")).both(fail("c").then(fail("d")));

    assert!(matches!(cause, Cause::Parallel(_)));
    assert_eq!(
        cause.failures().into_iter().cloned().collect::<Vec<_>>(),
        vec!["a", "b", "c", "d"]
    );
}

#[test]
fn rendering_says_what_happened() {
    assert_eq!(Cause::<String>::Empty.to_string(), "no failure");
    assert_eq!(fail("boom").to_string(), "boom");
    assert_eq!(Cause::<String>::die("bug").to_string(), "defect: bug");
    assert_eq!(Cause::<String>::Interrupt.to_string(), "interrupted");
    assert_eq!(fail("a").then(fail("b")).to_string(), "a, then b");
    assert_eq!(
        fail("a").then(fail("b")).then(fail("c")).to_string(),
        "a, then b, then c"
    );
    assert_eq!(
        fail("a").both(fail("b")).to_string(),
        "a, and concurrently b"
    );
}

#[test]
fn an_empty_cause_reports_itself_as_empty() {
    assert!(Cause::<String>::Empty.is_empty());
    assert!(!fail("boom").is_empty());
}
