use super::*;

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn at(count: u32) -> Attempt {
    Attempt {
        count,
        elapsed: Duration::ZERO,
    }
}

#[test]
fn recurs_runs_a_bounded_number_of_times() {
    let schedule = Schedule::recurs(3);

    assert_eq!(schedule.delays(10).len(), 3);
    assert!(schedule.decide(at(2)).is_continue());
    assert_eq!(schedule.decide(at(3)), Decision::Done);
}

#[test]
fn recurs_zero_never_retries() {
    assert_eq!(Schedule::recurs(0).decide(at(0)), Decision::Done);
    assert!(Schedule::recurs(0).delays(10).is_empty());
}

#[test]
fn spaced_waits_the_same_delay_every_time() {
    let delays = Schedule::spaced(ms(250)).delays(4);

    assert_eq!(delays, vec![ms(250), ms(250), ms(250), ms(250)]);
}

#[test]
fn exponential_doubles_by_default() {
    let delays = Schedule::exponential(ms(100)).delays(5);

    assert_eq!(delays, vec![ms(100), ms(200), ms(400), ms(800), ms(1_600)]);
}

#[test]
fn exponential_accepts_a_fractional_growth_factor() {
    let delays = Schedule::exponential_with(ms(100), 150).delays(4);

    // 337.5ms is not a whole millisecond, and rounding it away would make the
    // schedule drift from its own definition.
    assert_eq!(
        delays,
        vec![ms(100), ms(150), ms(225), Duration::from_micros(337_500)]
    );
}

#[test]
fn exponential_with_a_factor_below_one_shrinks() {
    let delays = Schedule::exponential_with(ms(1_000), 50).delays(3);

    assert_eq!(delays, vec![ms(1_000), ms(500), ms(250)]);
}

#[test]
fn fibonacci_grows_by_the_sequence() {
    let delays = Schedule::fibonacci(ms(100)).delays(6);

    assert_eq!(
        delays,
        vec![ms(100), ms(200), ms(300), ms(500), ms(800), ms(1_300)]
    );
}

#[test]
fn up_to_stops_once_the_budget_is_spent() {
    let schedule = Schedule::up_to(ms(500));

    assert!(
        schedule
            .decide(Attempt {
                count: 0,
                elapsed: ms(499)
            })
            .is_continue()
    );
    assert_eq!(
        schedule.decide(Attempt {
            count: 0,
            elapsed: ms(500)
        }),
        Decision::Done
    );
}

#[test]
fn intersect_stops_as_soon_as_either_side_stops() {
    let schedule = Schedule::spaced(ms(10)).intersect(Schedule::recurs(2));

    assert_eq!(schedule.delays(10).len(), 2);
}

#[test]
fn intersect_waits_the_longer_of_the_two_delays() {
    let schedule = Schedule::spaced(ms(10)).intersect(Schedule::spaced(ms(50)));

    assert_eq!(schedule.decide(at(0)), Decision::Continue(ms(50)));
}

#[test]
fn union_continues_while_either_side_wants_to() {
    let schedule = Schedule::recurs(1).union(Schedule::recurs(4));

    assert_eq!(schedule.delays(10).len(), 4);
}

#[test]
fn union_waits_the_shorter_of_the_two_delays() {
    let schedule = Schedule::spaced(ms(10)).union(Schedule::spaced(ms(50)));

    assert_eq!(schedule.decide(at(0)), Decision::Continue(ms(10)));
}

#[test]
fn union_uses_the_live_side_when_one_has_stopped() {
    let schedule = Schedule::recurs(1).union(Schedule::spaced(ms(30)));

    assert_eq!(schedule.decide(at(5)), Decision::Continue(ms(30)));
}

/// A ceiling, not a floor. `intersect` with `spaced` takes the *longer* of the
/// two delays, so using it to cap turns the cap into a minimum.
#[test]
fn max_delay_caps_exponential_growth() {
    let schedule = Schedule::exponential(ms(100)).max_delay(ms(500));

    assert_eq!(
        schedule.delays(6),
        vec![ms(100), ms(200), ms(400), ms(500), ms(500), ms(500)]
    );
}

#[test]
fn capping_a_schedule_that_stops_still_stops() {
    let schedule = Schedule::recurs(2).max_delay(ms(10));

    assert_eq!(schedule.delays(10).len(), 2);
    assert_eq!(schedule.decide(at(2)), Decision::Done);
}

#[test]
fn a_realistic_policy_composes_from_the_pieces() {
    // Exponential backoff, capped at a second, giving up after five tries.
    let schedule = Schedule::exponential(ms(50))
        .max_delay(ms(1_000))
        .intersect(Schedule::recurs(5));

    let delays = schedule.delays(20);

    assert_eq!(delays.len(), 5);
    assert_eq!(delays[0], ms(50));
    assert!(delays.iter().all(|delay| *delay <= ms(1_000)));
    assert!(delays.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn stop_never_continues_and_forever_always_does() {
    assert_eq!(Schedule::Stop.decide(at(0)), Decision::Done);
    assert_eq!(
        Schedule::Forever.decide(at(u32::MAX)),
        Decision::Continue(Duration::ZERO)
    );
}

#[test]
fn exponential_saturates_instead_of_wrapping() {
    // A schedule is a policy for handling failure; overflowing one turns a
    // recoverable error into a panic, or worse, into a zero delay and a spin.
    let schedule = Schedule::exponential_with(Duration::from_secs(1), u32::MAX);

    for count in [1u32, 2, 10, 1_000, u32::MAX] {
        let decision = schedule.decide(at(count));
        assert_eq!(decision, Decision::Continue(MAX_DELAY), "count {count}");
    }
}

#[test]
fn exponential_from_a_huge_base_saturates() {
    let schedule = Schedule::exponential(Duration::MAX);

    assert_eq!(schedule.decide(at(0)), Decision::Continue(MAX_DELAY));
    assert_eq!(schedule.decide(at(64)), Decision::Continue(MAX_DELAY));
}

#[test]
fn fibonacci_saturates_instead_of_wrapping() {
    let schedule = Schedule::fibonacci(Duration::from_secs(1));

    for count in [100u32, 1_000, 100_000] {
        assert_eq!(schedule.decide(at(count)), Decision::Continue(MAX_DELAY));
    }
}

#[test]
fn spaced_beyond_the_maximum_is_clamped() {
    assert_eq!(
        Schedule::spaced(Duration::MAX).decide(at(0)),
        Decision::Continue(MAX_DELAY)
    );
}

#[test]
fn ten_thousand_attempts_terminate() {
    let schedule = Schedule::exponential(ms(1)).intersect(Schedule::recurs(10_000));

    assert_eq!(schedule.delays(20_000).len(), 10_000);
}

/// `decide` runs on every retry, so it walks an explicit stack rather than
/// recursing. Construction depth is a different matter: a schedule is written by
/// hand from a handful of combinators, never grown from untrusted input, so a
/// thousand levels is far past anything real.
#[test]
fn deeply_nested_combinators_do_not_overflow_the_stack() {
    let mut schedule = Schedule::spaced(ms(1));
    for _ in 0..1_000 {
        schedule = schedule.intersect(Schedule::Forever);
    }

    assert_eq!(schedule.decide(at(0)), Decision::Continue(ms(1)));
}

#[test]
fn an_attempt_advances_without_overflowing() {
    let attempt = Attempt {
        count: u32::MAX,
        elapsed: Duration::MAX,
    };

    let next = attempt.advance(Duration::from_secs(1));

    assert_eq!(next.count, u32::MAX);
    assert_eq!(next.elapsed, Duration::MAX);
}

#[test]
fn the_first_attempt_starts_at_zero() {
    let first = Attempt::first();

    assert_eq!(first.count, 0);
    assert_eq!(first.elapsed, Duration::ZERO);
}

#[test]
fn delays_are_bounded_by_the_requested_limit() {
    assert_eq!(Schedule::Forever.delays(7).len(), 7);
    assert!(Schedule::Forever.delays(0).is_empty());
}

#[test]
fn a_decision_reports_its_delay() {
    assert_eq!(Decision::Continue(ms(5)).delay(), Some(ms(5)));
    assert_eq!(Decision::Done.delay(), None);
    assert!(Decision::Continue(ms(5)).is_continue());
    assert!(!Decision::Done.is_continue());
}

#[test]
fn intersect_is_commutative_in_its_decisions() {
    let left = Schedule::spaced(ms(10)).intersect(Schedule::recurs(3));
    let right = Schedule::recurs(3).intersect(Schedule::spaced(ms(10)));

    for count in 0..6 {
        assert_eq!(left.decide(at(count)), right.decide(at(count)), "{count}");
    }
}

#[test]
fn union_is_commutative_in_its_decisions() {
    let left = Schedule::spaced(ms(10)).union(Schedule::recurs(3));
    let right = Schedule::recurs(3).union(Schedule::spaced(ms(10)));

    for count in 0..6 {
        assert_eq!(left.decide(at(count)), right.decide(at(count)), "{count}");
    }
}
