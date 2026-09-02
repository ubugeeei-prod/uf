//! How a fiber ended, and what state it is in while it runs.

use crate::cause::Cause;

/// The result of running an effect to completion.
///
/// A failure carries a whole [`Cause`], not a single error, because by the time
/// an effect finishes it may have failed in several ways at once — see the
/// module docs on [`crate::cause`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit<A, E> {
    /// The effect produced a value.
    Success(A),
    /// The effect did not.
    Failure(Cause<E>),
}

impl<A, E> Exit<A, E> {
    /// An exit that failed with one declared error.
    pub const fn fail(error: E) -> Self {
        Self::Failure(Cause::Fail(error))
    }

    /// An exit that was cancelled.
    pub const fn interrupt() -> Self {
        Self::Failure(Cause::Interrupt)
    }

    /// Whether the effect produced a value.
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Whether the effect ended only because it was cancelled.
    ///
    /// Distinct from failing: a supervisor that cannot tell them apart retries
    /// work the caller asked it to stop.
    pub fn is_interrupted(&self) -> bool {
        match self {
            Self::Success(_) => false,
            Self::Failure(cause) => cause.is_interrupted_only(),
        }
    }

    /// The value, if there is one.
    pub fn value(&self) -> Option<&A> {
        match self {
            Self::Success(value) => Some(value),
            Self::Failure(_) => None,
        }
    }

    /// The cause, if the effect failed.
    pub const fn cause(&self) -> Option<&Cause<E>> {
        match self {
            Self::Success(_) => None,
            Self::Failure(cause) => Some(cause),
        }
    }

    /// Rewrite the success value.
    pub fn map<B>(self, transform: impl FnOnce(A) -> B) -> Exit<B, E> {
        match self {
            Self::Success(value) => Exit::Success(transform(value)),
            Self::Failure(cause) => Exit::Failure(cause),
        }
    }

    /// Rewrite the declared error type.
    pub fn map_error<F>(self, mut transform: impl FnMut(E) -> F) -> Exit<A, F> {
        match self {
            Self::Success(value) => Exit::Success(value),
            Self::Failure(cause) => Exit::Failure(cause.map(&mut transform)),
        }
    }
}

impl<A, E> From<Result<A, E>> for Exit<A, E> {
    fn from(result: Result<A, E>) -> Self {
        match result {
            Ok(value) => Self::Success(value),
            Err(error) => Self::fail(error),
        }
    }
}

/// Where a fiber is in its life.
///
/// `Interrupting` is a state of its own rather than a flag on `Running`: a fiber
/// that has been asked to stop is still executing its cleanup, and treating that
/// window as either "running" or "done" is how cancellation leaks resources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FiberState<A, E> {
    /// Executing.
    Running,
    /// Waiting on something external.
    Suspended,
    /// Asked to stop, still unwinding.
    Interrupting,
    /// Finished.
    Done(Exit<A, E>),
}

impl<A, E> FiberState<A, E> {
    /// Whether the fiber has finished.
    pub const fn is_done(&self) -> bool {
        matches!(self, Self::Done(_))
    }

    /// Whether the fiber can still make progress on its own.
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Interrupting)
    }

    /// The exit, once there is one.
    pub const fn exit(&self) -> Option<&Exit<A, E>> {
        match self {
            Self::Done(exit) => Some(exit),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
