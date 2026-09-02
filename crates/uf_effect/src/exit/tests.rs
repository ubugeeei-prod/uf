use super::*;
use crate::cause::Cause;

type Run = Exit<u32, String>;

#[test]
fn a_success_carries_its_value() {
    let exit: Run = Exit::Success(7);

    assert!(exit.is_success());
    assert_eq!(exit.value(), Some(&7));
    assert!(exit.cause().is_none());
}

#[test]
fn a_failure_carries_a_whole_cause_not_one_error() {
    let exit: Run =
        Exit::Failure(Cause::fail("first".to_string()).both(Cause::fail("second".to_string())));

    assert!(!exit.is_success());
    assert_eq!(exit.value(), None);
    assert_eq!(exit.cause().expect("cause").failures().len(), 2);
}

#[test]
fn interruption_is_distinguishable_from_failure() {
    let interrupted: Run = Exit::interrupt();
    let failed: Run = Exit::fail("boom".to_string());
    let succeeded: Run = Exit::Success(1);

    assert!(interrupted.is_interrupted());
    assert!(!failed.is_interrupted());
    assert!(!succeeded.is_interrupted());
}

#[test]
fn an_interrupted_run_that_also_failed_is_not_merely_interrupted() {
    let exit: Run = Exit::Failure(Cause::Interrupt.then(Cause::fail("cleanup failed".to_string())));

    assert!(!exit.is_interrupted());
}

#[test]
fn mapping_rewrites_the_value_and_leaves_failures_alone() {
    let success: Run = Exit::Success(2);
    let failure: Run = Exit::fail("boom".to_string());

    assert_eq!(success.map(|value| value * 10), Exit::Success(20));
    assert_eq!(
        failure.clone().map(|value| value * 10),
        Exit::Failure(Cause::fail("boom".to_string()))
    );
}

#[test]
fn mapping_the_error_rewrites_every_failure_in_the_cause() {
    let exit: Run = Exit::Failure(Cause::fail("a".to_string()).then(Cause::fail("bb".to_string())));

    let mapped = exit.map_error(|error| error.len());

    assert_eq!(
        mapped
            .cause()
            .expect("cause")
            .failures()
            .into_iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn a_result_converts_into_an_exit() {
    let ok: Run = Ok(3).into();
    let err: Run = Err("boom".to_string()).into();

    assert_eq!(ok, Exit::Success(3));
    assert_eq!(err, Exit::fail("boom".to_string()));
}

#[test]
fn a_running_fiber_is_active_and_not_done() {
    let state: FiberState<u32, String> = FiberState::Running;

    assert!(state.is_active());
    assert!(!state.is_done());
    assert!(state.exit().is_none());
}

#[test]
fn an_interrupting_fiber_is_still_active() {
    // It is unwinding, and treating that window as "done" is how cancellation
    // leaks resources.
    let state: FiberState<u32, String> = FiberState::Interrupting;

    assert!(state.is_active());
    assert!(!state.is_done());
}

#[test]
fn a_suspended_fiber_is_neither_active_nor_done() {
    let state: FiberState<u32, String> = FiberState::Suspended;

    assert!(!state.is_active());
    assert!(!state.is_done());
}

#[test]
fn a_done_fiber_exposes_its_exit() {
    let state: FiberState<u32, String> = FiberState::Done(Exit::Success(9));

    assert!(state.is_done());
    assert!(!state.is_active());
    assert_eq!(state.exit().and_then(Exit::value), Some(&9));
}
