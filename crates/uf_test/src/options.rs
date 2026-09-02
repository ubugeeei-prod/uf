//! How a run is allowed to behave: how wide, how impatient, how forgiving.
//!
//! Every knob here is a bound. A test runner that cannot be bounded is a test
//! runner that can hang CI, so the defaults are conservative and each one names
//! the failure it prevents.

use std::num::NonZeroUsize;
use std::time::Duration;

use uf_effect::{Attempt, Decision, Schedule};

use crate::discovery::MAX_SOURCE_BYTES;

/// Default wall-clock budget for one test file.
pub const DEFAULT_FILE_TIMEOUT: Duration = Duration::from_secs(5);

/// Largest per-file budget that can be requested.
///
/// A budget is a guarantee that the run terminates; an unbounded one is not a
/// budget.
pub const MAX_FILE_TIMEOUT: Duration = Duration::from_secs(600);

/// Smallest per-file budget that can be requested.
pub const MIN_FILE_TIMEOUT: Duration = Duration::from_millis(1);

/// Default cap on assertions recorded from one test body.
pub const DEFAULT_MAX_ASSERTIONS_PER_TEST: usize = 1_000;

/// Hard cap on how many times one case is executed, whatever the schedule says.
///
/// A schedule is data, and data can say `Forever`. This is the backstop.
pub const MAX_ATTEMPTS: u32 = 100;

/// Longest a retry will sleep before the next attempt.
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);

/// How many files may run at once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Concurrency {
    /// One file at a time, on the calling thread. Used by the determinism
    /// tests and by anything measuring the serial baseline.
    Serial,
    /// One worker per available core.
    #[default]
    Auto,
    /// An exact worker count.
    Fixed(NonZeroUsize),
}

impl Concurrency {
    /// Resolve to a worker count, never zero.
    pub fn threads(self) -> usize {
        match self {
            Self::Serial => 1,
            Self::Auto => std::thread::available_parallelism()
                .map(NonZeroUsize::get)
                .unwrap_or(1),
            Self::Fixed(threads) => threads.get(),
        }
    }

    /// Whether the run happens on the calling thread.
    pub fn is_serial(self) -> bool {
        matches!(self, Self::Serial) || self.threads() == 1
    }
}

/// When to stop a run early.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Bail {
    /// Run every file.
    #[default]
    Off,
    /// Stop scheduling new files once this many tests have failed.
    ///
    /// Bailing deliberately trades determinism for speed: *which* files were
    /// still unscheduled when the threshold was crossed depends on the
    /// schedule, and the report says so with [`crate::FileStatus::NotRun`].
    After(NonZeroUsize),
}

impl Bail {
    /// A bail threshold from a count; `0` means no bail.
    pub fn after(failures: usize) -> Self {
        NonZeroUsize::new(failures).map_or(Self::Off, Self::After)
    }

    /// Whether `failures` has reached the threshold.
    pub fn is_reached(self, failures: usize) -> bool {
        match self {
            Self::Off => false,
            Self::After(limit) => failures >= limit.get(),
        }
    }
}

/// How a failing case is retried.
///
/// The policy is a [`Schedule`] — the same retry vocabulary the rest of the
/// repository uses — rather than a bare count, so `--retry 3` and an
/// exponential backoff are the same mechanism.
///
/// Re-running a source-level evaluation cannot change its outcome today,
/// because the evaluation is pure. The loop exists so the policy has one home,
/// one set of tests, and one attempt counter that a real engine will inherit
/// unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    schedule: Schedule,
    max_attempts: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::none()
    }
}

impl RetryPolicy {
    /// Never retry.
    pub fn none() -> Self {
        Self {
            schedule: Schedule::Stop,
            max_attempts: 1,
        }
    }

    /// Retry a failing case up to `retries` more times, immediately.
    pub fn retries(retries: u32) -> Self {
        Self {
            schedule: Schedule::recurs(retries),
            max_attempts: retries.saturating_add(1).min(MAX_ATTEMPTS),
        }
    }

    /// Drive retries from an arbitrary schedule, still bounded by
    /// [`MAX_ATTEMPTS`].
    pub fn from_schedule(schedule: Schedule) -> Self {
        Self {
            schedule,
            max_attempts: MAX_ATTEMPTS,
        }
    }

    /// The schedule behind this policy.
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// The most times a case will be executed.
    pub fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// How long to wait before the next retry, or [`None`] when the policy is
    /// finished.
    ///
    /// `attempt.count` is the number of retries already made, matching the
    /// [`Schedule`] convention that the first decision is taken at zero. Total
    /// executions — the first run plus every retry — never exceed
    /// [`Self::max_attempts`].
    pub fn next_delay(&self, attempt: Attempt) -> Option<Duration> {
        if attempt.count.saturating_add(1) >= self.max_attempts {
            return None;
        }
        match self.schedule.decide(attempt) {
            Decision::Continue(delay) => Some(delay.min(MAX_RETRY_DELAY)),
            Decision::Done => None,
        }
    }
}

/// Everything a run is allowed to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOptions {
    /// How many files run at once.
    pub concurrency: Concurrency,
    /// When to stop early.
    pub bail: Bail,
    /// How a failing case is retried.
    pub retry: RetryPolicy,
    /// Wall-clock budget for one file, clamped to
    /// [`MIN_FILE_TIMEOUT`]..=[`MAX_FILE_TIMEOUT`].
    pub file_timeout: Duration,
    /// Largest file the runner will scan.
    pub max_source_bytes: usize,
    /// Cap on assertions recorded from one test body.
    pub max_assertions_per_test: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            concurrency: Concurrency::default(),
            bail: Bail::default(),
            retry: RetryPolicy::default(),
            file_timeout: DEFAULT_FILE_TIMEOUT,
            max_source_bytes: MAX_SOURCE_BYTES,
            max_assertions_per_test: DEFAULT_MAX_ASSERTIONS_PER_TEST,
        }
    }
}

impl RunOptions {
    /// Run every file on the calling thread.
    pub fn serial() -> Self {
        Self {
            concurrency: Concurrency::Serial,
            ..Self::default()
        }
    }

    /// The per-file budget, clamped into the supported range.
    pub fn effective_file_timeout(&self) -> Duration {
        self.file_timeout.clamp(MIN_FILE_TIMEOUT, MAX_FILE_TIMEOUT)
    }

    /// The source size limit, clamped so a caller cannot ask for an unbounded
    /// allocation.
    pub fn effective_max_source_bytes(&self) -> usize {
        self.max_source_bytes.min(MAX_SOURCE_BYTES)
    }

    /// The per-body assertion cap, never zero.
    pub fn effective_max_assertions(&self) -> usize {
        self.max_assertions_per_test
            .clamp(1, DEFAULT_MAX_ASSERTIONS_PER_TEST)
    }
}
