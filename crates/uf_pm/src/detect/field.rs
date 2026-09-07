//! The corepack `"packageManager"` manifest field and the parser for it.
//!
//! The grammar is hand-written as a single forward pass with no backtracking,
//! because a regex over attacker-supplied manifest text is a ReDoS foothold.
//! Every rejection comes back as a typed [`PackageManagerFieldError`] carrying
//! only a length-bounded excerpt of the offending input, so nothing unvalidated
//! is ever echoed onward.

use std::cmp::Ordering;
use std::fmt;

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::MAX_PACKAGE_MANAGER_FIELD_BYTES;
use super::manager::{PackageManager, YarnEdition};

/// Semantic version parsed out of a `"packageManager"` field, or out of a
/// dependency range by [`Version::parse`].
///
/// Ordered by semver precedence rather than by derive: `1.10.0` is newer than
/// `1.9.0`, and `2.0.0-rc.1` is *older* than `2.0.0`. A derived ordering gets
/// both of those wrong, which is why this type carried none until `uf update`
/// needed to ask which of two published versions is newer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    /// Major component.
    pub major: u32,
    /// Minor component.
    pub minor: u32,
    /// Patch component.
    pub patch: u32,
    /// Prerelease segment without its leading `-`, when present.
    pub prerelease: Option<CompactString>,
}

impl Version {
    /// Parse a bare `major.minor.patch[-prerelease]` version.
    ///
    /// What a registry publishes under `versions`, and what the numeric part of
    /// a dependency range is. Build metadata (`+sha`) is accepted and dropped:
    /// semver says it takes no part in precedence, so keeping it would only
    /// give two equal versions two spellings.
    ///
    /// `None` rather than an error, because every caller is asking "is this a
    /// version" about text that is often deliberately not one — `workspace:*`,
    /// `latest`, a `git+ssh://` URL.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        // Length first: the same bound the field parser uses, for the same
        // reason. The input is manifest and registry text.
        if value.is_empty() || value.len() > MAX_PACKAGE_MANAGER_FIELD_BYTES {
            return None;
        }
        let value = value.split('+').next().unwrap_or(value);
        let mut cursor = 0;
        let major = take_number(value, &mut cursor).ok()?;
        expect_dot(value, &mut cursor).ok()?;
        let minor = take_number(value, &mut cursor).ok()?;
        expect_dot(value, &mut cursor).ok()?;
        let patch = take_number(value, &mut cursor).ok()?;

        let prerelease = if value.as_bytes().get(cursor) == Some(&b'-') {
            cursor += 1;
            Some(take_tagged_segment(value, &mut cursor).ok()?)
        } else {
            None
        };
        if cursor != value.len() {
            return None;
        }

        Some(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    /// Whether this version is a prerelease.
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        self.prerelease.is_some()
    }
}

impl Ord for Version {
    /// Semver precedence, section 11.
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.prerelease, &other.prerelease) {
                // "A pre-release version has lower precedence than a normal
                // version" — the rule that makes `2.0.0-rc.1` not the newest
                // 2.0.0 and keeps a release candidate out of a `^1` bump.
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(left), Some(right)) => compare_prerelease(left, right),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compare two prerelease segments by identifier, semver section 11.4.
///
/// Dot-separated: numeric identifiers compare numerically, a numeric identifier
/// is always lower than an alphanumeric one, alphanumerics compare by ASCII, and
/// when everything else is equal the segment with more identifiers wins. So
/// `alpha.2` < `alpha.10` — a plain string compare puts them the other way
/// round, which is exactly the mistake that makes a tool offer `alpha.9` as an
/// upgrade from `alpha.10`.
fn compare_prerelease(left: &str, right: &str) -> Ordering {
    let mut left = left.split('.');
    let mut right = right.split('.');
    loop {
        return match (left.next(), right.next()) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(one), Some(two)) => {
                let ordering = match (one.parse::<u64>(), two.parse::<u64>()) {
                    (Ok(one), Ok(two)) => one.cmp(&two),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => one.cmp(two),
                };
                if ordering == Ordering::Equal {
                    continue;
                }
                ordering
            }
        };
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(prerelease) = &self.prerelease {
            write!(formatter, "-{prerelease}")?;
        }
        Ok(())
    }
}

/// Validated `"packageManager"` field, in corepack's `name@version[+integrity]` shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManagerSpec {
    /// Manager named by the field.
    pub manager: PackageManager,
    /// Version pinned by the field.
    pub version: Version,
    /// Corepack integrity suffix without its leading `+`, when present.
    pub integrity: Option<CompactString>,
}

/// Typed rejection for an invalid `"packageManager"` manifest field.
///
/// The field is attacker-controlled, so every rejection is structured: nothing is
/// echoed into a shell and nothing unvalidated ever reaches an [`crate::Invocation`].
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum PackageManagerFieldError {
    /// The field was present but empty.
    #[error("packageManager field is empty")]
    Empty,
    /// The field was longer than [`MAX_PACKAGE_MANAGER_FIELD_BYTES`].
    #[error("packageManager field is {length} bytes; the limit is {limit}")]
    TooLong {
        /// Length of the rejected field.
        length: usize,
        /// Accepted limit.
        limit: usize,
    },
    /// The field carried no `@` separating the name from the version.
    #[error("packageManager field is missing the `name@version` separator")]
    MissingSeparator,
    /// The name was not one of `npm`, `pnpm`, `yarn`, `bun`.
    #[error("packageManager name `{name}` is not one of npm, pnpm, yarn, bun")]
    UnknownManager {
        /// Rejected manager name.
        name: CompactString,
    },
    /// The version was not `major.minor.patch`.
    #[error("packageManager version `{version}` is not a major.minor.patch version")]
    MalformedVersion {
        /// Rejected version text.
        version: CompactString,
    },
    /// A numeric version component did not fit in a `u32`.
    #[error("packageManager version component `{component}` overflows a 32-bit integer")]
    VersionOverflow {
        /// Rejected component text.
        component: CompactString,
    },
    /// A byte outside the accepted alphabet appeared in the field.
    #[error("packageManager field has the forbidden character `{character}` at byte {offset}")]
    ForbiddenCharacter {
        /// The offending character.
        character: char,
        /// Byte offset of the character within the field.
        offset: usize,
    },
}

/// Parse and validate a corepack-style `"packageManager"` field.
///
/// The accepted shape is
/// `^(npm|pnpm|yarn|bun)@\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$`,
/// implemented as a single forward pass with no backtracking. A regex engine here
/// would be a ReDoS foothold on attacker-supplied manifest text (the CVE class of
/// `ansi-regex` CVE-2021-3807 and `semver` CVE-2022-25883), so the grammar is
/// hand-written and every byte is visited at most once.
///
/// # Errors
///
/// Returns [`PackageManagerFieldError`] describing exactly why the field was
/// refused. `uf` is intentionally not accepted: the field is corepack's, and uf
/// projects pin their manager through `uf.config.js`.
pub fn parse_package_manager_field(
    value: &str,
) -> Result<PackageManagerSpec, PackageManagerFieldError> {
    if value.is_empty() {
        return Err(PackageManagerFieldError::Empty);
    }
    if value.len() > MAX_PACKAGE_MANAGER_FIELD_BYTES {
        return Err(PackageManagerFieldError::TooLong {
            length: value.len(),
            limit: MAX_PACKAGE_MANAGER_FIELD_BYTES,
        });
    }

    let bytes = value.as_bytes();
    let Some(separator) = bytes.iter().position(|byte| *byte == b'@') else {
        return Err(PackageManagerFieldError::MissingSeparator);
    };

    let name = &value[..separator];
    let manager = match name {
        "npm" => PackageManager::Npm,
        "pnpm" => PackageManager::Pnpm,
        "bun" => PackageManager::Bun,
        // The edition is decided by the pinned major once the version parses.
        "yarn" => PackageManager::Yarn(YarnEdition::Classic),
        _ => {
            return Err(PackageManagerFieldError::UnknownManager {
                name: name.to_compact_string(),
            });
        }
    };

    let mut cursor = separator + 1;
    let major = take_number(value, &mut cursor)?;
    expect_dot(value, &mut cursor)?;
    let minor = take_number(value, &mut cursor)?;
    expect_dot(value, &mut cursor)?;
    let patch = take_number(value, &mut cursor)?;

    let prerelease = if bytes.get(cursor) == Some(&b'-') {
        cursor += 1;
        Some(take_tagged_segment(value, &mut cursor)?)
    } else {
        None
    };

    let integrity = if bytes.get(cursor) == Some(&b'+') {
        cursor += 1;
        Some(take_tagged_segment(value, &mut cursor)?)
    } else {
        None
    };

    if cursor != bytes.len() {
        return Err(forbidden_character(value, cursor));
    }

    let manager = match manager {
        PackageManager::Yarn(_) if major >= 2 => PackageManager::Yarn(YarnEdition::Berry),
        other => other,
    };

    Ok(PackageManagerSpec {
        manager,
        version: Version {
            major,
            minor,
            patch,
            prerelease,
        },
        integrity,
    })
}

fn take_number(value: &str, cursor: &mut usize) -> Result<u32, PackageManagerFieldError> {
    let bytes = value.as_bytes();
    let start = *cursor;
    while bytes.get(*cursor).is_some_and(u8::is_ascii_digit) {
        *cursor += 1;
    }

    if start == *cursor {
        return Err(PackageManagerFieldError::MalformedVersion {
            version: truncated(&value[start.min(value.len())..]),
        });
    }

    value[start..*cursor]
        .parse::<u32>()
        .map_err(|_| PackageManagerFieldError::VersionOverflow {
            component: truncated(&value[start..*cursor]),
        })
}

fn expect_dot(value: &str, cursor: &mut usize) -> Result<(), PackageManagerFieldError> {
    if value.as_bytes().get(*cursor) == Some(&b'.') {
        *cursor += 1;
        return Ok(());
    }
    Err(PackageManagerFieldError::MalformedVersion {
        version: truncated(&value[(*cursor).min(value.len())..]),
    })
}

/// Consume a `[0-9A-Za-z.-]+` run, stopping before `+` or at the end of input.
fn take_tagged_segment(
    value: &str,
    cursor: &mut usize,
) -> Result<CompactString, PackageManagerFieldError> {
    let bytes = value.as_bytes();
    let start = *cursor;

    while let Some(byte) = bytes.get(*cursor) {
        if byte.is_ascii_alphanumeric() || *byte == b'.' || *byte == b'-' {
            *cursor += 1;
        } else {
            break;
        }
    }

    if start == *cursor {
        return Err(forbidden_character(value, start));
    }
    Ok(value[start..*cursor].to_compact_string())
}

fn forbidden_character(value: &str, offset: usize) -> PackageManagerFieldError {
    let character = value
        .get(offset..)
        .and_then(|rest| rest.chars().next())
        .unwrap_or(char::REPLACEMENT_CHARACTER);
    PackageManagerFieldError::ForbiddenCharacter { character, offset }
}

/// Keep untrusted text out of error messages beyond a fixed budget.
fn truncated(value: &str) -> CompactString {
    const BUDGET: usize = 32;

    let end = value
        .char_indices()
        .map(|(index, character)| index + character.len_utf8())
        .take_while(|end| *end <= BUDGET)
        .last()
        .unwrap_or(0);
    value[..end].to_compact_string()
}
