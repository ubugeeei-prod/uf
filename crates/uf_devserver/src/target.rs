//! Request-target validation: the grammar gate in front of every other stage.
//!
//! # Threat model
//!
//! [CVE-2025-32395] was a dev server that accepted a request target it could not
//! parse, "fixed it up" into something path-shaped, and then served whatever the
//! fix-up produced. The repair step was the vulnerability: the string the access
//! check later ran against was manufactured by the server, not sent by the
//! client, so no amount of care in the check could make it describe the request.
//!
//! This module therefore has exactly one job and one failure mode. A byte string
//! either *is* a valid origin-form request target, in which case it becomes a
//! [`RequestTarget`], or it is not, in which case it becomes a [`TargetError`]
//! and the connection gets a `400` — never a repaired path. There is no code
//! path in this crate that turns a rejected target into an accepted one.
//!
//! # The grammar
//!
//! RFC 9112 gives a request target four forms. A file-serving dev server needs
//! precisely one of them, so the other three are rejected outright:
//!
//! | Form | Example | Handling |
//! | --- | --- | --- |
//! | origin-form | `/app/main.js?import` | the only accepted form |
//! | absolute-form | `http://evil.test/.env` | rejected |
//! | authority-form | `evil.test:443` | rejected |
//! | asterisk-form | `*` | rejected |
//!
//! On top of the form, every accepted target is plain visible ASCII: no C0
//! controls, no space, no `DEL`, no high bytes, no NUL. A fragment is rejected
//! rather than stripped, because origin-form has no fragment and stripping one
//! is the same "repair the client's input" move that produced CVE-2025-32395.
//!
//! # Loaders
//!
//! [`Loader`] is a closed enum chosen by exact match against a fixed table of
//! `&'static str` keys parsed out of the query. It is never chosen by a suffix
//! match on the raw target: [CVE-2025-30208] slipped `?raw??` past a suffix
//! match, and [CVE-2025-31125] did the same with `?import` and `?inline`. Under
//! exact matching `raw??` is simply not the key `raw`, so it selects nothing.
//!
//! The query is matched as raw bytes and is never percent-decoded. Decoding it
//! would re-introduce the mismatch this crate exists to prevent — a decoded
//! `%72aw` that matches the table while the enforcement stage sees something
//! else. A loader flag is an exact ASCII token or it is not a loader flag.
//!
//! [CVE-2025-30208]: https://github.com/advisories/GHSA-x574-m823-4x7w
//! [CVE-2025-31125]: https://nvd.nist.gov/vuln/detail/CVE-2025-31125
//! [CVE-2025-32395]: https://nvd.nist.gov/vuln/detail/CVE-2025-32395

use thiserror::Error;

#[cfg(test)]
mod tests;

/// Longest accepted request target, in bytes.
///
/// Rule 4 of the threat model: every read has an explicit bound. A target
/// longer than this is rejected before any allocation happens.
pub const MAX_TARGET_BYTES: usize = 4096;

/// How a resolved file is meant to be delivered to the client.
///
/// This is deliberately a closed enum with no `Other(String)` variant. Untrusted
/// query text can select *among* these, and can never name a new one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Loader {
    /// Serve as an ES module. The default when the query names no loader.
    #[default]
    Module,
    /// Serve the bytes as a default-exported string (`?raw`).
    Raw,
    /// Serve as an inlined `data:` URL (`?inline`).
    Inline,
    /// Serve the resolved URL as a string (`?url`).
    Url,
    /// Serve as a worker entry point (`?worker`).
    Worker,
}

impl Loader {
    /// The stable kebab-case name of this loader.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Raw => "raw",
            Self::Inline => "inline",
            Self::Url => "url",
            Self::Worker => "worker",
        }
    }
}

/// The only query keys that select a loader.
///
/// Exhaustive and `&'static str`: no request text ever reaches a position where
/// it could name a loader that is not on this list.
const LOADER_KEYS: &[(&str, Loader)] = &[
    ("inline", Loader::Inline),
    ("raw", Loader::Raw),
    ("url", Loader::Url),
    ("worker", Loader::Worker),
];

/// Query keys that are recognized, carry no loader meaning, and are ignored.
///
/// `?import` is the module-graph marker a bundler appends to its own rewritten
/// specifiers; it selects [`Loader::Module`], which is also the default.
const NEUTRAL_KEYS: &[&str] = &["import", "t", "v"];

/// Why a request target was rejected.
///
/// Every variant is a `400`. None of them is recoverable, and none of them has
/// a "repair" counterpart anywhere in this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TargetError {
    /// The target was empty.
    #[error("request target is empty")]
    Empty,
    /// The target is longer than [`MAX_TARGET_BYTES`].
    #[error("request target is longer than {MAX_TARGET_BYTES} bytes: {len}")]
    TooLong {
        /// Length that was supplied.
        len: usize,
    },
    /// The target is the asterisk-form (`*`), which only `OPTIONS` may use.
    #[error("asterisk-form request target is not served")]
    AsteriskForm,
    /// The target is absolute-form or authority-form: it does not begin with a
    /// single `/`.
    #[error("request target is not origin-form")]
    NotOriginForm,
    /// The target begins with `//`, a network-path reference that some parsers
    /// read as an authority.
    #[error("request target starts with a network-path reference")]
    NetworkPathReference,
    /// The target contains a byte outside printable US-ASCII.
    #[error("request target contains a forbidden byte {byte:#04x} at index {index}")]
    ForbiddenByte {
        /// The offending byte.
        byte: u8,
        /// Where it was found.
        index: usize,
    },
    /// The target contains a `#`, which origin-form has no room for.
    #[error("request target contains a fragment")]
    Fragment,
    /// Two different loader keys appeared in one query.
    #[error("query names conflicting loaders {first} and {second}")]
    ConflictingLoaders {
        /// The loader named first.
        first: &'static str,
        /// The loader that contradicted it.
        second: &'static str,
    },
}

/// A request target that has passed the origin-form grammar gate.
///
/// The path is still percent-encoded here: decoding is [`crate::resolve`]'s job
/// and happens exactly once. Holding an un-decoded path in this type is
/// deliberate — it means no stage can decode twice by accident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestTarget<'a> {
    path: &'a str,
    query: Option<&'a str>,
    loader: Loader,
}

impl<'a> RequestTarget<'a> {
    /// Validate `raw` as an origin-form request target.
    ///
    /// # Errors
    ///
    /// Returns a [`TargetError`] for anything that is not origin-form. The
    /// caller's only correct response is a `400`; there is no repaired value to
    /// fall back to.
    pub fn parse(raw: &'a str) -> Result<Self, TargetError> {
        if raw.is_empty() {
            return Err(TargetError::Empty);
        }
        if raw.len() > MAX_TARGET_BYTES {
            return Err(TargetError::TooLong { len: raw.len() });
        }
        if raw == "*" {
            return Err(TargetError::AsteriskForm);
        }

        // Byte gate first, so later stages never see a control character, a NUL,
        // a raw space, or a high byte. Non-ASCII must arrive percent-encoded.
        for (index, byte) in raw.bytes().enumerate() {
            if byte == b'#' {
                return Err(TargetError::Fragment);
            }
            if !(0x21..=0x7e).contains(&byte) {
                return Err(TargetError::ForbiddenByte { byte, index });
            }
        }

        if !raw.starts_with('/') {
            return Err(TargetError::NotOriginForm);
        }
        if raw.starts_with("//") {
            return Err(TargetError::NetworkPathReference);
        }

        let (path, query) = match raw.find('?') {
            Some(index) => (&raw[..index], Some(&raw[index + 1..])),
            None => (raw, None),
        };
        let loader = parse_loader(query)?;

        Ok(Self {
            path,
            query,
            loader,
        })
    }

    /// The still-encoded path, guaranteed to start with `/` and to contain no
    /// `?`, `#`, or non-printable byte.
    pub fn path(&self) -> &'a str {
        self.path
    }

    /// The raw query, if the target had one, with the `?` removed.
    pub fn query(&self) -> Option<&'a str> {
        self.query
    }

    /// The loader this target selected.
    pub fn loader(&self) -> Loader {
        self.loader
    }
}

/// Choose a loader by exact key match over [`LOADER_KEYS`].
///
/// Ambiguity is an error rather than a precedence rule. Picking a winner from
/// two loader keys is how a request ends up served under a loader the access
/// decision did not model, which is the shape of CVE-2025-30208.
fn parse_loader(query: Option<&str>) -> Result<Loader, TargetError> {
    let Some(query) = query else {
        return Ok(Loader::Module);
    };
    let mut chosen: Option<Loader> = None;
    for pair in query.split('&') {
        let key = match pair.find('=') {
            Some(index) => &pair[..index],
            None => pair,
        };
        if key.is_empty() || NEUTRAL_KEYS.contains(&key) {
            continue;
        }
        let Some(&(_, loader)) = LOADER_KEYS.iter().find(|(name, _)| *name == key) else {
            // Unrecognized keys are inert. `raw??` lands here, and inertness is
            // the whole point: it selects nothing rather than fuzzily matching
            // `raw`.
            continue;
        };
        match chosen {
            None => chosen = Some(loader),
            Some(first) if first == loader => {}
            Some(first) => {
                return Err(TargetError::ConflictingLoaders {
                    first: first.as_str(),
                    second: loader.as_str(),
                });
            }
        }
    }
    Ok(chosen.unwrap_or_default())
}
