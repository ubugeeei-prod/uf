//! The allow/deny decision, evaluated on canonical paths only.
//!
//! # Threat model
//!
//! A deny list is only as good as the string it is matched against. Every dev
//! server bypass in `docs/security.md` shares one shape: the deny list was
//! consulted with a value that was not the path the server went on to open —
//! a raw request string, a half-decoded URL, a path with the query still glued
//! to the end. This module therefore takes an already-canonical [`Utf8Path`]
//! and nothing else. It cannot be handed a request string, because it does not
//! accept one.
//!
//! Three rules, in this order:
//!
//! 1. **Deny wins.** A path matching any deny pattern is refused even when it
//!    sits inside an allowed root. There is no allow entry that overrides a
//!    deny entry.
//! 2. **[`DEFAULT_DENY`] is not removable.** A project's configured patterns
//!    are *added* to the built-in list, never substituted for it. A deny list
//!    that a config typo can shrink is a deny list that will one day be shrunk;
//!    there is no dev-server request for `.env` that is worth serving.
//! 3. **Allow by table.** What survives the deny pass must live under one of a
//!    fixed list of roots. The list comes from configuration at startup and is
//!    never extended by anything a request carries.
//!
//! # Matching
//!
//! Patterns are matched by a hand-written two-pointer globber with a single
//! backtrack point, at both the segment and the character level. It is `O(n*m)`
//! in the worst case and allocates nothing per comparison. Rule 5 of the threat
//! model forbids a backtracking regex on untrusted input; a naively recursive
//! glob has exactly the same exponential blow-up, so it is avoided the same way.
//!
//! A pattern containing no `/` is matched against the whole relative path *and*
//! against every individual segment, so `.env*` denies `config/.env.local` and
//! `*.pem` denies `certs/server.pem`. This only ever adds denials.
//!
//! Root containment uses [`Utf8Path::starts_with`], which compares whole
//! components. A textual prefix test would let `/srv/project-secrets` pass as
//! being "inside" `/srv/project`.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use smallvec::SmallVec;
use thiserror::Error;

#[cfg(test)]
mod tests;

/// Patterns denied by every policy, whatever a project configures.
pub const DEFAULT_DENY: &[&str] = &[
    ".env*",
    "**/.git/**",
    "*.pem",
    "*.key",
    "*.crt",
    "**/.uf/**",
];

/// Most allow roots a policy will accept.
pub const MAX_ALLOW_ROOTS: usize = 32;

/// Most deny patterns a policy will accept.
pub const MAX_DENY_PATTERNS: usize = 256;

/// Longest accepted deny pattern, in bytes.
pub const MAX_PATTERN_BYTES: usize = 512;

/// Why a policy could not be built.
///
/// These are startup failures. A misconfigured allow root is a hard error
/// rather than a silently dropped entry, because a dropped allow root looks
/// exactly like a working one until the day it does not.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PolicyError {
    /// An allow root does not exist or could not be canonicalized.
    #[error("allow root {root} could not be resolved: {message}")]
    UnresolvableRoot {
        /// The configured root.
        root: Utf8PathBuf,
        /// The underlying I/O failure.
        message: CompactString,
    },
    /// An allow root canonicalized to a non-UTF-8 path.
    #[error("allow root {root} resolved to a non-UTF-8 path")]
    NonUtf8Root {
        /// The configured root.
        root: Utf8PathBuf,
    },
    /// More than [`MAX_ALLOW_ROOTS`] roots were configured.
    #[error("at most {MAX_ALLOW_ROOTS} allow roots are supported, got {count}")]
    TooManyRoots {
        /// How many were configured.
        count: usize,
    },
    /// More than [`MAX_DENY_PATTERNS`] patterns were configured.
    #[error("at most {MAX_DENY_PATTERNS} deny patterns are supported, got {count}")]
    TooManyPatterns {
        /// How many were configured.
        count: usize,
    },
    /// A deny pattern is longer than [`MAX_PATTERN_BYTES`].
    #[error("deny pattern is longer than {MAX_PATTERN_BYTES} bytes: {len}")]
    PatternTooLong {
        /// Length that was supplied.
        len: usize,
    },
}

/// Why the policy refused a canonical path.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PolicyDenial {
    /// The path resolved outside every allowed root.
    #[error("{path} is outside every allowed root")]
    OutsideAllowedRoots {
        /// The canonical path that was checked.
        path: Utf8PathBuf,
    },
    /// The path matched a deny pattern.
    #[error("{path} matches the deny pattern {pattern}")]
    DeniedByPattern {
        /// The path the deny list was evaluated against: the canonical path
        /// from [`FsPolicy::decide`], or the normalized relative path when the
        /// caller ran the monotone pre-filesystem pass.
        path: Utf8PathBuf,
        /// The pattern that matched.
        pattern: CompactString,
    },
}

/// One compiled deny pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DenyGlob {
    source: CompactString,
    segments: SmallVec<[GlobSegment; 4]>,
    rooted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GlobSegment {
    /// A `**` segment: matches zero or more whole path segments.
    AnyDepth,
    /// A literal segment, possibly containing `*` and `?`.
    Pattern(CompactString),
}

impl DenyGlob {
    fn compile(source: &str) -> Result<Self, PolicyError> {
        if source.len() > MAX_PATTERN_BYTES {
            return Err(PolicyError::PatternTooLong { len: source.len() });
        }
        // A pattern may be written with either separator; both mean the same
        // thing here for the same reason `\` is a separator in a request path.
        let normalized = source.replace('\\', "/");
        let rooted = normalized.contains('/');
        let segments = normalized
            .split('/')
            .filter(|segment| !segment.is_empty())
            .map(|segment| {
                if segment == "**" {
                    GlobSegment::AnyDepth
                } else {
                    GlobSegment::Pattern(CompactString::new(segment))
                }
            })
            .collect();
        Ok(Self {
            source: CompactString::new(source),
            segments,
            rooted,
        })
    }

    fn matches(&self, relative: &Utf8Path) -> bool {
        let segments: SmallVec<[&str; 16]> = relative.as_str().split('/').collect();
        if match_segments(&self.segments, &segments) {
            return true;
        }
        // An unrooted pattern also applies to each individual segment, so
        // `.env*` reaches `config/.env.local` and `*.pem` reaches `certs/a.pem`.
        match self.segments.as_slice() {
            [GlobSegment::Pattern(pattern)] if !self.rooted => segments
                .iter()
                .any(|segment| match_one_segment(pattern, segment)),
            _ => false,
        }
    }
}

/// The access decision for one dev server.
///
/// Built once at startup from configuration, then consulted per request. It
/// holds canonical roots so that a request-time check never has to touch the
/// filesystem to know what "inside the project" means.
#[derive(Debug, Clone)]
pub struct FsPolicy {
    roots: SmallVec<[Utf8PathBuf; 2]>,
    deny: SmallVec<[DenyGlob; 8]>,
}

impl FsPolicy {
    /// Build a policy over `root` with `extra` additional allow roots and
    /// `deny` patterns *on top of* [`DEFAULT_DENY`].
    ///
    /// Every root is canonicalized here, once, so no request-time code path
    /// ever has to.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] when a root cannot be resolved or a bound is
    /// exceeded.
    pub fn new<E, D>(root: &Utf8Path, extra: E, deny: D) -> Result<Self, PolicyError>
    where
        E: IntoIterator,
        E::Item: AsRef<str>,
        D: IntoIterator,
        D::Item: AsRef<str>,
    {
        let mut roots: SmallVec<[Utf8PathBuf; 2]> = SmallVec::new();
        roots.push(canonical_root(root)?);
        for entry in extra {
            let entry = entry.as_ref();
            let joined = if Utf8Path::new(entry).is_absolute() {
                Utf8PathBuf::from(entry)
            } else {
                root.join(entry)
            };
            let resolved = canonical_root(&joined)?;
            if !roots.contains(&resolved) {
                roots.push(resolved);
            }
            if roots.len() > MAX_ALLOW_ROOTS {
                return Err(PolicyError::TooManyRoots { count: roots.len() });
            }
        }

        // The built-ins go in first and are never consulted for removal: a
        // configured list can only make the policy stricter.
        let mut patterns: SmallVec<[DenyGlob; 8]> = SmallVec::new();
        for pattern in DEFAULT_DENY {
            patterns.push(DenyGlob::compile(pattern)?);
        }
        for pattern in deny {
            let compiled = DenyGlob::compile(pattern.as_ref())?;
            if !patterns.contains(&compiled) {
                patterns.push(compiled);
            }
            if patterns.len() > MAX_DENY_PATTERNS {
                return Err(PolicyError::TooManyPatterns {
                    count: patterns.len(),
                });
            }
        }

        Ok(Self {
            roots,
            deny: patterns,
        })
    }

    /// Build a policy over `root` with only the built-in deny list.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] when `root` cannot be resolved.
    pub fn with_defaults(root: &Utf8Path) -> Result<Self, PolicyError> {
        Self::new(root, Vec::<&str>::new(), Vec::<&str>::new())
    }

    /// The canonical allow roots, project root first.
    pub fn roots(&self) -> &[Utf8PathBuf] {
        &self.roots
    }

    /// The configured deny patterns, in order.
    pub fn deny_patterns(&self) -> impl Iterator<Item = &str> {
        self.deny.iter().map(|glob| glob.source.as_str())
    }

    /// Decide whether `canonical` may be served.
    ///
    /// `canonical` must already be a real, symlink-free path — the contract
    /// this whole crate is built to keep. Deny patterns are evaluated first and
    /// cannot be overridden.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyDenial`] when a deny pattern matches or the path lies
    /// outside every allowed root.
    pub fn decide(&self, canonical: &Utf8Path) -> Result<(), PolicyDenial> {
        let Some(root) = self
            .roots
            .iter()
            .find(|root| canonical.starts_with(root.as_path()))
        else {
            return Err(PolicyDenial::OutsideAllowedRoots {
                path: canonical.to_owned(),
            });
        };
        let relative = canonical.strip_prefix(root).unwrap_or(canonical);
        if let Some(pattern) = self.deny_pattern_for(relative) {
            return Err(PolicyDenial::DeniedByPattern {
                path: canonical.to_owned(),
                pattern: CompactString::new(pattern),
            });
        }
        Ok(())
    }

    /// The deny pattern matching `relative`, if any.
    ///
    /// Exposed so the request pipeline can run the deny list *before* touching
    /// the filesystem, on the lexically normalized relative path. That early
    /// pass is monotone: it can only add denials, never remove one, so it
    /// cannot reintroduce the "checked one string, opened another" bug. Its
    /// purpose is to keep a denied name from becoming an existence oracle —
    /// `/.env` must answer the same way whether or not `.env` is there.
    pub fn deny_pattern_for(&self, relative: &Utf8Path) -> Option<&str> {
        self.deny
            .iter()
            .find(|glob| glob.matches(relative))
            .map(|glob| glob.source.as_str())
    }
}

fn canonical_root(root: &Utf8Path) -> Result<Utf8PathBuf, PolicyError> {
    let resolved = std::fs::canonicalize(root).map_err(|error| PolicyError::UnresolvableRoot {
        root: root.to_owned(),
        message: CompactString::new(error.to_string()),
    })?;
    Utf8PathBuf::from_path_buf(resolved).map_err(|_| PolicyError::NonUtf8Root {
        root: root.to_owned(),
    })
}

/// Segment-level wildcard match with a single backtrack point.
fn match_segments(pattern: &[GlobSegment], path: &[&str]) -> bool {
    let (mut pi, mut si) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while si < path.len() {
        if pi < pattern.len() {
            match &pattern[pi] {
                GlobSegment::AnyDepth => {
                    star = Some((pi + 1, si));
                    pi += 1;
                    continue;
                }
                GlobSegment::Pattern(segment) if match_one_segment(segment, path[si]) => {
                    pi += 1;
                    si += 1;
                    continue;
                }
                GlobSegment::Pattern(_) => {}
            }
        }
        match star {
            Some((resume, consumed)) => {
                pi = resume;
                si = consumed + 1;
                star = Some((resume, consumed + 1));
            }
            None => return false,
        }
    }
    while pi < pattern.len() && matches!(pattern[pi], GlobSegment::AnyDepth) {
        pi += 1;
    }
    pi == pattern.len()
}

/// Character-level wildcard match with a single backtrack point.
///
/// Operates on `char`s rather than bytes so `?` consumes one character of a
/// non-ASCII name instead of one byte of it. A `?` that splits a multi-byte
/// character would make a deny pattern quietly fail to match, which is the
/// dangerous direction for a deny list.
fn match_one_segment(pattern: &str, text: &str) -> bool {
    let pattern: SmallVec<[char; 32]> = pattern.chars().collect();
    let text: SmallVec<[char; 32]> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < text.len() {
        if pi < pattern.len() {
            match pattern[pi] {
                '*' => {
                    star = Some((pi + 1, ti));
                    pi += 1;
                    continue;
                }
                '?' => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                candidate if candidate == text[ti] => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some((resume, consumed)) => {
                pi = resume;
                ti = consumed + 1;
                star = Some((resume, consumed + 1));
            }
            None => return false,
        }
    }
    while pi < pattern.len() && pattern[pi] == '*' {
        pi += 1;
    }
    pi == pattern.len()
}
