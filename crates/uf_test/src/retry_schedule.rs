//! Retry schedule data used by the native test runner.

use std::time::Duration;

/// Largest delay a schedule will ever ask for.
pub const MAX_DELAY: Duration = Duration::from_secs(60 * 60 * 24 * 365);

/// What a retry schedule decides after one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Run again after this delay.
    Continue(Duration),
    /// Stop retrying.
    Done,
}

impl Decision {
    /// The delay, or [`None`] when the schedule is finished.
    pub const fn delay(self) -> Option<Duration> {
        match self {
            Self::Continue(delay) => Some(delay),
            Self::Done => None,
        }
    }

    /// Whether the schedule wants another attempt.
    pub const fn is_continue(self) -> bool {
        matches!(self, Self::Continue(_))
    }
}

/// How many times a test case has already retried, and for how long.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attempt {
    /// Completed retries so far. The first retry decision is made with `0`.
    pub count: u32,
    /// Time since the first test execution started.
    pub elapsed: Duration,
}

impl Attempt {
    /// The state before any retry decision.
    pub const fn first() -> Self {
        Self {
            count: 0,
            elapsed: Duration::ZERO,
        }
    }

    /// The state after one more run that took `taken`.
    pub const fn advance(self, taken: Duration) -> Self {
        Self {
            count: self.count.saturating_add(1),
            elapsed: self.elapsed.saturating_add(taken),
        }
    }
}

/// A retry policy for test cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Schedule {
    /// Run at most `n` more times, immediately.
    Recurs(u32),
    /// Wait a fixed delay, forever.
    Spaced(Duration),
    /// `base * factor^count`, saturating at [`MAX_DELAY`].
    Exponential {
        /// Delay before the first retry.
        base: Duration,
        /// Growth per attempt, in hundredths.
        factor_percent: u32,
    },
    /// `base * fib(count)`, saturating at [`MAX_DELAY`].
    Fibonacci(Duration),
    /// Stop once `elapsed` reaches this.
    UpTo(Duration),
    /// Continue only while both agree; take the longer delay.
    Intersect(Box<Schedule>, Box<Schedule>),
    /// Continue while either agrees; take the shorter delay.
    Union(Box<Schedule>, Box<Schedule>),
    /// Follow the inner schedule, but never wait longer than this.
    Capped(Box<Schedule>, Duration),
    /// Never continue.
    Stop,
    /// Always continue, immediately.
    Forever,
}

impl Schedule {
    /// Run at most `n` more times with no delay.
    pub const fn recurs(n: u32) -> Self {
        Self::Recurs(n)
    }

    /// Wait `delay` between attempts, forever.
    pub const fn spaced(delay: Duration) -> Self {
        Self::Spaced(delay)
    }

    /// Exponential backoff, doubling by default.
    pub const fn exponential(base: Duration) -> Self {
        Self::Exponential {
            base,
            factor_percent: 200,
        }
    }

    /// Exponential backoff with an explicit growth factor in hundredths.
    pub const fn exponential_with(base: Duration, factor_percent: u32) -> Self {
        Self::Exponential {
            base,
            factor_percent,
        }
    }

    /// Fibonacci backoff.
    pub const fn fibonacci(base: Duration) -> Self {
        Self::Fibonacci(base)
    }

    /// Give up once this much time has passed.
    pub const fn up_to(budget: Duration) -> Self {
        Self::UpTo(budget)
    }

    /// Continue only while both schedules agree, waiting the longer delay.
    pub fn intersect(self, other: Self) -> Self {
        Self::Intersect(Box::new(self), Box::new(other))
    }

    /// Continue while either schedule agrees, waiting the shorter delay.
    pub fn union(self, other: Self) -> Self {
        Self::Union(Box::new(self), Box::new(other))
    }

    /// Cap every delay this schedule produces.
    pub fn max_delay(self, cap: Duration) -> Self {
        Self::Capped(Box::new(self), cap)
    }

    /// Decide what to do after `attempt`.
    pub fn decide(&self, attempt: Attempt) -> Decision {
        #[derive(Debug)]
        enum Step<'a> {
            Eval(&'a Schedule),
            CombineIntersect,
            CombineUnion,
            Cap(Duration),
        }

        let mut work = vec![Step::Eval(self)];
        let mut results: Vec<Decision> = Vec::new();

        while let Some(step) = work.pop() {
            match step {
                Step::Eval(schedule) => match schedule {
                    Self::Intersect(left, right) => {
                        work.push(Step::CombineIntersect);
                        work.push(Step::Eval(left));
                        work.push(Step::Eval(right));
                    }
                    Self::Union(left, right) => {
                        work.push(Step::CombineUnion);
                        work.push(Step::Eval(left));
                        work.push(Step::Eval(right));
                    }
                    Self::Capped(inner, cap) => {
                        work.push(Step::Cap(*cap));
                        work.push(Step::Eval(inner));
                    }
                    leaf => results.push(leaf.decide_leaf(attempt)),
                },
                Step::CombineIntersect => {
                    let left = results.pop().expect("left decision");
                    let right = results.pop().expect("right decision");
                    results.push(match (left, right) {
                        (Decision::Continue(a), Decision::Continue(b)) => {
                            Decision::Continue(a.max(b))
                        }
                        _ => Decision::Done,
                    });
                }
                Step::CombineUnion => {
                    let left = results.pop().expect("left decision");
                    let right = results.pop().expect("right decision");
                    results.push(match (left, right) {
                        (Decision::Continue(a), Decision::Continue(b)) => {
                            Decision::Continue(a.min(b))
                        }
                        (Decision::Continue(delay), Decision::Done)
                        | (Decision::Done, Decision::Continue(delay)) => Decision::Continue(delay),
                        (Decision::Done, Decision::Done) => Decision::Done,
                    });
                }
                Step::Cap(cap) => {
                    let inner = results.pop().expect("inner decision");
                    results.push(match inner {
                        Decision::Continue(delay) => Decision::Continue(delay.min(cap)),
                        Decision::Done => Decision::Done,
                    });
                }
            }
        }

        results.pop().expect("a decision")
    }

    /// Every delay this schedule would ask for, up to `limit` attempts.
    pub fn delays(&self, limit: usize) -> Vec<Duration> {
        let mut delays = Vec::new();
        let mut attempt = Attempt::first();
        for _ in 0..limit {
            match self.decide(attempt) {
                Decision::Continue(delay) => {
                    delays.push(delay);
                    attempt = attempt.advance(delay);
                }
                Decision::Done => break,
            }
        }
        delays
    }

    fn decide_leaf(&self, attempt: Attempt) -> Decision {
        match self {
            Self::Stop => Decision::Done,
            Self::Forever => Decision::Continue(Duration::ZERO),
            Self::Recurs(limit) => {
                if attempt.count < *limit {
                    Decision::Continue(Duration::ZERO)
                } else {
                    Decision::Done
                }
            }
            Self::Spaced(delay) => Decision::Continue(clamp_delay(*delay)),
            Self::Exponential {
                base,
                factor_percent,
            } => Decision::Continue(exponential_delay(*base, *factor_percent, attempt.count)),
            Self::Fibonacci(base) => Decision::Continue(fibonacci_delay(*base, attempt.count)),
            Self::UpTo(budget) => {
                if attempt.elapsed < *budget {
                    Decision::Continue(Duration::ZERO)
                } else {
                    Decision::Done
                }
            }
            Self::Intersect(_, _) | Self::Union(_, _) | Self::Capped(_, _) => {
                unreachable!("composites are handled by `decide`")
            }
        }
    }
}

fn clamp_delay(delay: Duration) -> Duration {
    if delay > MAX_DELAY { MAX_DELAY } else { delay }
}

fn exponential_delay(base: Duration, factor_percent: u32, count: u32) -> Duration {
    let mut nanos = base.as_nanos();
    let max_nanos = MAX_DELAY.as_nanos();
    let factor = u128::from(factor_percent);

    for _ in 0..count {
        if nanos >= max_nanos {
            return MAX_DELAY;
        }
        nanos = nanos.saturating_mul(factor) / 100;
    }

    if nanos >= max_nanos {
        MAX_DELAY
    } else {
        duration_from_nanos(nanos)
    }
}

fn fibonacci_delay(base: Duration, count: u32) -> Duration {
    let mut previous: u128 = 1;
    let mut current: u128 = 1;
    for _ in 0..count {
        let next = previous.saturating_add(current);
        previous = current;
        current = next;
        if current > u128::from(u64::MAX) {
            return MAX_DELAY;
        }
    }

    let nanos = base.as_nanos().saturating_mul(current);
    if nanos >= MAX_DELAY.as_nanos() {
        MAX_DELAY
    } else {
        duration_from_nanos(nanos)
    }
}

fn duration_from_nanos(nanos: u128) -> Duration {
    const NANOS_PER_SEC: u128 = 1_000_000_000;
    let secs = u64::try_from(nanos / NANOS_PER_SEC).unwrap_or(u64::MAX);
    let subsec = u32::try_from(nanos % NANOS_PER_SEC).unwrap_or(0);
    Duration::new(secs, subsec)
}
