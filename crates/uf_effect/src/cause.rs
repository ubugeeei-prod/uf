//! Why an effect did not produce a value.
//!
//! An effect system that collapses every failure into one error value throws
//! away the thing that makes it worth having: a program can fail in the way its
//! type said it might (`Fail`), fail in a way nobody declared (`Die`), be
//! cancelled (`Interrupt`), or — once effects run concurrently — fail in several
//! of those ways at once. [`Cause`] keeps that structure so a report can say
//! what actually happened instead of picking a winner.
//!
//! Modelled on Effect-TS's `Cause`, expressed in Rust so the native runtime and
//! the Flow surface agree on one definition.

use std::fmt;

/// A defect: a failure nobody declared in the effect's error channel.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Defect {
    /// What went wrong, for a human.
    pub message: String,
}

impl Defect {
    /// Build a defect from anything renderable.
    pub fn new(message: impl fmt::Display) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

impl fmt::Display for Defect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Why an effect did not produce a value.
///
/// `E` is the declared error channel. The distinction between the two composite
/// variants is deliberate: [`Cause::Sequential`] records "this failed, and then
/// cleanup also failed", [`Cause::Parallel`] records "these failed at the same
/// time". Collapsing them into one list would lose which is which.
///
/// Both composites are n-ary rather than pairs. `Effect.all` over ten thousand
/// items that all fail produces one `Parallel` with ten thousand children, not a
/// tree ten thousand levels deep — so cloning, comparing, and dropping a cause
/// stay proportional to real nesting rather than to how many things failed. A
/// binary shape makes the *reporting* of a large failure overflow the stack,
/// which is a poor way to end a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cause<E> {
    /// The effect finished without failing. The identity for [`Cause::then`] and
    /// [`Cause::both`].
    Empty,
    /// A declared failure: the error the effect's type said it could produce.
    Fail(E),
    /// An undeclared failure. A bug, not a modelled outcome.
    Die(Defect),
    /// The effect was cancelled.
    Interrupt,
    /// Failures in the order they happened.
    Sequential(Vec<Cause<E>>),
    /// Failures that happened concurrently, in no meaningful order.
    Parallel(Vec<Cause<E>>),
}

impl<E> Cause<E> {
    /// A declared failure.
    pub const fn fail(error: E) -> Self {
        Self::Fail(error)
    }

    /// An undeclared failure.
    pub fn die(message: impl fmt::Display) -> Self {
        Self::Die(Defect::new(message))
    }

    /// Whether nothing failed.
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Whether this cause is interruption and nothing else.
    ///
    /// Cancelling a program is not the same as it failing, and a supervisor that
    /// cannot tell the difference retries work the user asked it to stop.
    pub fn is_interrupted_only(&self) -> bool {
        match self {
            Self::Interrupt => true,
            Self::Empty => false,
            Self::Fail(_) | Self::Die(_) => false,
            Self::Sequential(parts) | Self::Parallel(parts) => {
                parts
                    .iter()
                    .all(|part| part.is_empty() || part.is_interrupted_only())
                    && parts.iter().any(|part| !part.is_empty())
            }
        }
    }

    /// Whether any part of this cause is a defect.
    pub fn has_defect(&self) -> bool {
        self.fold(false, |found, cause| found || matches!(cause, Self::Die(_)))
    }

    /// This cause, then `next`.
    ///
    /// [`Cause::Empty`] is the identity on both sides, so a cleanup step that
    /// succeeded never widens the cause. Composition flattens from both sides,
    /// which makes it genuinely associative rather than merely equivalent:
    /// `a.then(b).then(c)` and `a.then(b.then(c))` are the same value.
    pub fn then(self, next: Self) -> Self {
        match (self, next) {
            (Self::Empty, other) | (other, Self::Empty) => other,
            (Self::Sequential(mut left), Self::Sequential(right)) => {
                left.extend(right);
                Self::Sequential(left)
            }
            (Self::Sequential(mut left), right) => {
                left.push(right);
                Self::Sequential(left)
            }
            (left, Self::Sequential(right)) => {
                let mut parts = Vec::with_capacity(right.len() + 1);
                parts.push(left);
                parts.extend(right);
                Self::Sequential(parts)
            }
            (left, right) => Self::Sequential(vec![left, right]),
        }
    }

    /// This cause alongside `other`, with no ordering between them.
    ///
    /// Associative for the same reason [`Cause::then`] is.
    pub fn both(self, other: Self) -> Self {
        match (self, other) {
            (Self::Empty, other) | (other, Self::Empty) => other,
            (Self::Parallel(mut left), Self::Parallel(right)) => {
                left.extend(right);
                Self::Parallel(left)
            }
            (Self::Parallel(mut left), right) => {
                left.push(right);
                Self::Parallel(left)
            }
            (left, Self::Parallel(right)) => {
                let mut parts = Vec::with_capacity(right.len() + 1);
                parts.push(left);
                parts.extend(right);
                Self::Parallel(parts)
            }
            (left, right) => Self::Parallel(vec![left, right]),
        }
    }

    /// Every declared failure in the tree, in left-to-right order.
    pub fn failures(&self) -> Vec<&E> {
        let mut found = Vec::new();
        self.visit(&mut |cause| {
            if let Self::Fail(error) = cause {
                found.push(error);
            }
        });
        found
    }

    /// Every defect in the tree, in left-to-right order.
    pub fn defects(&self) -> Vec<&Defect> {
        let mut found = Vec::new();
        self.visit(&mut |cause| {
            if let Self::Die(defect) = cause {
                found.push(defect);
            }
        });
        found
    }

    /// Rewrite the declared error type, leaving the structure alone.
    pub fn map<F>(self, transform: &mut impl FnMut(E) -> F) -> Cause<F> {
        match self {
            Self::Empty => Cause::Empty,
            Self::Fail(error) => Cause::Fail(transform(error)),
            Self::Die(defect) => Cause::Die(defect),
            Self::Interrupt => Cause::Interrupt,
            Self::Sequential(parts) => {
                Cause::Sequential(parts.into_iter().map(|part| part.map(transform)).collect())
            }
            Self::Parallel(parts) => {
                Cause::Parallel(parts.into_iter().map(|part| part.map(transform)).collect())
            }
        }
    }

    /// Walk every node, parents before children, left before right.
    ///
    /// Iterative rather than recursive: a long chain of `acquireRelease` cleanup
    /// failures builds a deep tree, and blowing the stack while *reporting* a
    /// failure is a poor way to end a program.
    pub fn visit<'a>(&'a self, apply: &mut impl FnMut(&'a Self)) {
        let mut stack = vec![self];
        while let Some(cause) = stack.pop() {
            apply(cause);
            if let Self::Sequential(parts) | Self::Parallel(parts) = cause {
                stack.extend(parts.iter().rev());
            }
        }
    }

    /// Fold every node into an accumulator.
    pub fn fold<'a, A>(&'a self, initial: A, mut combine: impl FnMut(A, &'a Self) -> A) -> A {
        let mut accumulator = Some(initial);
        self.visit(&mut |cause| {
            let current = accumulator.take().expect("accumulator is always present");
            accumulator = Some(combine(current, cause));
        });
        accumulator.expect("accumulator is always present")
    }

    /// How many nodes the tree holds, counting the empty ones.
    pub fn len(&self) -> usize {
        self.fold(0, |count, _| count + 1)
    }
}

impl<E: fmt::Display> fmt::Display for Cause<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("no failure"),
            Self::Fail(error) => write!(f, "{error}"),
            Self::Die(defect) => write!(f, "defect: {defect}"),
            Self::Interrupt => f.write_str("interrupted"),
            Self::Sequential(parts) => write_joined(f, parts, ", then "),
            Self::Parallel(parts) => write_joined(f, parts, ", and concurrently "),
        }
    }
}

fn write_joined<E: fmt::Display>(
    f: &mut fmt::Formatter<'_>,
    parts: &[Cause<E>],
    separator: &str,
) -> fmt::Result {
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            f.write_str(separator)?;
        }
        write!(f, "{part}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
