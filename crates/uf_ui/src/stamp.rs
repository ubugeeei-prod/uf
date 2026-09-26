//! The line a copy carries to say where it came from.
//!
//! ```text
//! // Written by `uf ui add dialog` from uf 0.0.0-alpha.33, sha256 1f3a….
//! ```
//!
//! The crate's header says why the record is a line in the file rather than a
//! lockfile beside it. This module is the line: how it is written, how it is
//! found again, and what its digest is taken over.
//!
//! # What the digest covers
//!
//! The file without the stamp line, with every `\r\n` read as `\n`. The stamp
//! cannot cover itself, and a checkout that converts line endings — Git on
//! Windows does by default — has not edited the component it converted.

use std::fmt::Write as _;

use compact_str::CompactString;
use sha2::{Digest as _, Sha256};

use crate::registry::{REGISTRY_VERSION, is_component_name};

/// Where the line starts, up to the component's name.
const OPENING: &str = "// Written by `uf ui add ";

/// What a copy records about where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    /// The component the copy is of.
    pub component: CompactString,
    /// The uf, and so the registry, that wrote it.
    pub version: CompactString,
    /// The SHA-256 of what was written, in lower-case hex. See the module
    /// header for exactly what that is.
    pub digest: CompactString,
}

impl Stamp {
    /// The stamp this uf puts on `source` when it writes `component`.
    pub fn new(component: &str, source: &str) -> Self {
        Self {
            component: component.into(),
            version: REGISTRY_VERSION.into(),
            digest: digest(source),
        }
    }

    /// The line, with no line ending.
    pub fn line(&self) -> String {
        compact_str::format_compact!(
            "{OPENING}{}` from uf {}, sha256 {}.",
            self.component,
            self.version,
            self.digest
        )
        .into_string()
    }

    /// Read a line as a stamp, or say it is not one.
    ///
    /// Strict, because a comment that happens to start the same way is not a
    /// record of anything: every field has to be the shape [`Stamp::line`]
    /// writes, or the line is ordinary text in somebody's file.
    pub fn parse(line: &str) -> Option<Self> {
        let rest = line.trim_end_matches(['\r', '\n']).strip_prefix(OPENING)?;
        let (component, rest) = rest.split_once("` from uf ")?;
        let (version, rest) = rest.split_once(", sha256 ")?;
        let digest = rest.strip_suffix('.')?;
        let well_formed = is_component_name(component)
            && !version.is_empty()
            && !version.contains(char::is_whitespace)
            && digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        well_formed.then(|| Self {
            component: component.into(),
            version: version.into(),
            digest: digest.into(),
        })
    }
}

/// `source`, stamped as `component`: exactly what `uf ui add` writes.
pub fn stamped(component: &str, source: &str) -> String {
    with_stamp(source, &Stamp::new(component, source))
}

/// `content` with `stamp` as its last line.
///
/// [`stamped`] is the case where the content is the registry's text and the
/// stamp its digest. `uf ui update` writes the other case: a merge, whose
/// content is the project's and whose stamp names the registry text it was
/// merged onto — so the copy then reads as edited against this uf's version,
/// with nothing of the registry's left outstanding.
pub fn with_stamp(content: &str, stamp: &Stamp) -> String {
    let mut out = String::with_capacity(content.len() + 128);
    out.push_str(content);
    if !content.is_empty() && !content.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&stamp.line());
    out.push('\n');
    out
}

/// A file from a project, with its stamp taken off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copy {
    /// The stamp, when the file has one.
    pub stamp: Option<Stamp>,
    /// Everything else in the file, with `\r\n` read as `\n`.
    pub content: String,
}

impl Copy {
    /// Split `text` into its stamp and the rest.
    ///
    /// The stamp is the *last* line that reads as one, wherever it ended up: a
    /// person who added a function below it has not stopped the file being a
    /// copy, only edited it, and the digest says so.
    pub fn read(text: &str) -> Self {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        let found = lines.iter().rposition(|line| Stamp::parse(line).is_some());
        let stamp = found.and_then(|at| Stamp::parse(lines[at]));
        let mut content = String::with_capacity(text.len());
        for (at, line) in lines.iter().enumerate() {
            if Some(at) != found {
                content.push_str(line);
            }
        }
        Self {
            stamp,
            content: content.replace("\r\n", "\n"),
        }
    }

    /// Whether the content is what the stamp says was written.
    pub fn is_untouched(&self) -> bool {
        self.stamp
            .as_ref()
            .is_some_and(|stamp| stamp.digest == digest(&self.content))
    }
}

/// The SHA-256 of `text` with `\r\n` read as `\n`, in lower-case hex.
pub fn digest(text: &str) -> CompactString {
    let normalised = text.replace("\r\n", "\n");
    let hash = Sha256::digest(normalised.as_bytes());
    let mut out = String::with_capacity(64);
    for byte in hash.iter() {
        // Writing to a `String` cannot fail.
        let _ = write!(out, "{byte:02x}");
    }
    out.into()
}
