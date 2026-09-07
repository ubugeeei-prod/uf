//! The part of a dependency range `uf update` has to understand.
//!
//! # Why this is not a semver library
//!
//! node-semver's grammar is large — `||`, hyphen ranges, `x` wildcards,
//! `1.2.x - 2.x`, whitespace as intersection — and implementing all of it means
//! owning every one of its edge cases forever, in a crate whose job is to run
//! other people's package managers. uf does not need all of it. `uf update`
//! asks exactly two questions:
//!
//! 1. does this range already allow the newest published version, or not?
//! 2. if not, what would the range have to say instead?
//!
//! Both are answerable for the shapes that a manifest actually contains:
//! `^1.2.3`, `~1.2.3`, `1.2.3`, `>=1.2.3`. Those four are what `npm install`
//! writes, what `pnpm add` writes, and what a person types. Everything else —
//! `workspace:*`, `catalog:default`, `npm:@scope/other@^1`, `file:../x`, a git
//! URL, a compound range with `||`, an `x` wildcard — is **reported and left
//! alone** rather than guessed at.
//!
//! Leaving it alone is the honest answer, not a limitation to apologise for. A
//! `workspace:*` that uf rewrote to `^1.4.0` would have silently unlinked a
//! local package; a `||` that uf rewrote to a single range would have thrown
//! away a compatibility statement somebody wrote deliberately. `uf update`
//! prints those rows with the reason, and the reader decides.
//!
//! # Prereleases
//!
//! Excluded, unless the range's own base is one. A project on `^1.2.3` is not
//! asking to be moved to `2.0.0-rc.1`, and a tool that offered it would be
//! offering an upgrade nobody can act on. A project already on `2.0.0-rc.1`
//! *is* asking, so for that project prereleases are candidates.

use std::fmt;

use compact_str::{CompactString, ToCompactString};

use crate::detect::Version;

/// How far apart two versions are, in the terms a person decides with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Same major and minor.
    Patch,
    /// Same major.
    Minor,
    /// A different major, which is where a changelog has to be read.
    Major,
}

impl Level {
    /// Every level, lowest first — the order `uf update --patch` implies.
    pub const ALL: [Self; 3] = [Self::Patch, Self::Minor, Self::Major];

    /// The word `uf update` prints and accepts.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Minor => "minor",
            Self::Major => "major",
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// The comparator a range opens with, which is what a rewrite has to preserve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prefix {
    /// `^1.2.3` — up to but not including the next left-most non-zero bump.
    Caret,
    /// `~1.2.3` — up to but not including the next minor.
    Tilde,
    /// `1.2.3` or `=1.2.3` — that version and no other.
    Exact,
    /// `>=1.2.3` — that version and anything above it.
    AtLeast,
}

impl Prefix {
    /// The text this prefix is written as.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Caret => "^",
            Self::Tilde => "~",
            Self::Exact => "",
            Self::AtLeast => ">=",
        }
    }
}

/// A dependency range uf can both read and rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    /// The comparator, preserved across a rewrite: a project that pinned
    /// exactly is not asking to be moved to a caret.
    pub prefix: Prefix,
    /// The version the comparator is about.
    pub base: Version,
}

impl Range {
    /// Parse a range, or decline.
    ///
    /// `None` for every shape in the module docs that uf reports rather than
    /// rewrites. It is not an error: a manifest full of `workspace:*` is a
    /// correct manifest.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        // A compound range is a statement, not a version. Splitting one and
        // keeping a half would change what it says.
        if text.contains("||") || text.contains(char::is_whitespace) {
            return None;
        }
        let (prefix, rest) = if let Some(rest) = text.strip_prefix(">=") {
            (Prefix::AtLeast, rest)
        } else if let Some(rest) = text.strip_prefix('^') {
            (Prefix::Caret, rest)
        } else if let Some(rest) = text.strip_prefix('~') {
            (Prefix::Tilde, rest)
        } else if let Some(rest) = text.strip_prefix('=') {
            (Prefix::Exact, rest)
        } else {
            (Prefix::Exact, text)
        };
        // `Version::parse` is the wildcard filter as well: `1.x`, `*`, `latest`
        // and `workspace:*` all fail it, which is the answer for all of them.
        Some(Self {
            prefix,
            base: Version::parse(rest)?,
        })
    }

    /// Whether the range already permits `candidate`.
    ///
    /// This is the line between "the manager's own update moves it" and "the
    /// range has to change first", which is the whole distinction `uf update`
    /// exists to draw.
    #[must_use]
    pub fn allows(&self, candidate: &Version) -> bool {
        // npm's prerelease rule: a prerelease only satisfies a range whose own
        // base is a prerelease of the same `major.minor.patch`. Otherwise every
        // `^1.0.0` in the world would match `1.1.0-alpha.1`.
        if candidate.is_prerelease()
            && !(self.base.is_prerelease()
                && (self.base.major, self.base.minor, self.base.patch)
                    == (candidate.major, candidate.minor, candidate.patch))
        {
            return false;
        }
        if *candidate < self.base {
            return false;
        }
        match self.prefix {
            Prefix::Exact => *candidate == self.base,
            Prefix::AtLeast => true,
            Prefix::Tilde => {
                (candidate.major, candidate.minor) == (self.base.major, self.base.minor)
            }
            // `^0.2.3` is `<0.3.0` and `^0.0.3` is `<0.0.4`: below 1.0.0 the
            // left-most non-zero component is the breaking one, which is the
            // rule people are most often surprised by.
            Prefix::Caret => {
                if self.base.major > 0 {
                    candidate.major == self.base.major
                } else if self.base.minor > 0 {
                    (candidate.major, candidate.minor) == (0, self.base.minor)
                } else {
                    (candidate.major, candidate.minor, candidate.patch) == (0, 0, self.base.patch)
                }
            }
        }
    }

    /// The same comparator, about `version`.
    #[must_use]
    pub fn rewritten_to(&self, version: &Version) -> CompactString {
        format!("{}{version}", self.prefix.as_str()).to_compact_string()
    }
}

/// How big a step it is from `from` to `to`, or `None` when `to` is not newer.
#[must_use]
pub fn step(from: &Version, to: &Version) -> Option<Level> {
    if to <= from {
        return None;
    }
    Some(if to.major != from.major {
        Level::Major
    } else if to.minor != from.minor {
        Level::Minor
    } else {
        Level::Patch
    })
}

/// The newest of `candidates` that is at most `level` away from `from`.
///
/// `Level::Major` means "the newest there is", which is what `--latest` asks
/// for; `Level::Minor` holds the major still, and `Level::Patch` holds the
/// minor too. `None` when nothing published is newer than what is declared.
#[must_use]
pub fn best<'a>(candidates: &'a [Version], from: &Version, level: Level) -> Option<&'a Version> {
    candidates
        .iter()
        .filter(|candidate| {
            // A prerelease is a candidate only for a project already on one —
            // the same rule `Range::allows` applies, stated once more because
            // this path does not go through a range.
            !candidate.is_prerelease() || from.is_prerelease()
        })
        .filter(|candidate| step(from, candidate).is_some_and(|step| step <= level))
        .max()
}

#[cfg(test)]
mod tests;
