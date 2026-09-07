//! The dependency ranges a workspace declares, and how uf changes one.
//!
//! # Reading
//!
//! Every `package.json` [`crate::discover_package_manifests`] finds — the root
//! and every workspace package, submodules and `node_modules` excluded — and
//! the four fields a range can be declared in. A monorepo that declares `react`
//! in six packages has six [`Declaration`]s, because six is how many places
//! have to change and a report that said "react" once would be hiding five of
//! them.
//!
//! # Writing
//!
//! A `package.json` is a file a person reads. Rewriting it wholesale would put
//! the whole file in the diff of a change that moved one character, and would
//! quietly restyle a manifest somebody indented the way they wanted it.
//!
//! So a change is a **splice**: the `"name": "range"` pair is located in the
//! original bytes and the range's own quotes are replaced. Nothing else in the
//! file is touched, so a four-space manifest stays four-space and a tab
//! manifest stays tabs, byte for byte.
//!
//! When the pair cannot be located unambiguously — the same name and the same
//! range in two fields, an escaped character, a manifest written on one line —
//! uf falls back to re-serialising the whole file with the indentation it
//! detected. That is a bigger diff, and it is still correct.
//!
//! Either way the result is parsed again and compared against the value the
//! change was supposed to produce, before anything is written. A rewrite that
//! did not produce exactly the intended manifest does not reach the disk.

use std::collections::BTreeMap;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::{CompactString, ToCompactString};
use serde_json::Value;

use crate::{PackageManagerError, discover_package_manifests, is_polluting_json_key};

/// The manifest fields a dependency range can be declared in.
///
/// `bundleDependencies` is absent on purpose: it is a list of names, not a map
/// of ranges. `overrides` and `resolutions` are absent too — they are a
/// statement about somebody else's tree, and moving one is not the same
/// decision as moving your own dependency.
pub const DEPENDENCY_FIELDS: [&str; 4] = [
    "dependencies",
    "devDependencies",
    "optionalDependencies",
    "peerDependencies",
];

/// One dependency, as one manifest declares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The manifest that declares it.
    pub manifest: Utf8PathBuf,
    /// Which of [`DEPENDENCY_FIELDS`].
    pub field: &'static str,
    /// The package name.
    pub name: CompactString,
    /// The range, exactly as written.
    pub range: CompactString,
}

/// Every dependency every manifest in the workspace declares.
///
/// # Errors
///
/// When a manifest cannot be read or is not JSON. A workspace with a broken
/// manifest is a workspace whose install is broken too, so this is the same
/// failure `uf install` would report, at the same point.
pub fn declarations(root: &Utf8Path) -> Result<Vec<Declaration>, PackageManagerError> {
    let mut found = Vec::new();
    for manifest in discover_package_manifests(root)? {
        let source = fs::read_to_string(&manifest).map_err(|source| PackageManagerError::Read {
            path: manifest.clone(),
            source,
        })?;
        let value =
            serde_json::from_str::<Value>(&source).map_err(|source| PackageManagerError::Parse {
                path: manifest.clone(),
                source,
            })?;
        for field in DEPENDENCY_FIELDS {
            let Some(object) = value.get(field).and_then(Value::as_object) else {
                continue;
            };
            for (name, range) in object {
                // Manifest content is untrusted; a `__proto__` key is aimed at
                // a JavaScript consumer and is not a dependency.
                if is_polluting_json_key(name) {
                    continue;
                }
                let Some(range) = range.as_str() else {
                    continue;
                };
                found.push(Declaration {
                    manifest: manifest.clone(),
                    field,
                    name: name.as_str().to_compact_string(),
                    range: range.to_compact_string(),
                });
            }
        }
    }
    Ok(found)
}

/// A range to write, keyed by the field and name it is declared under.
pub type Changes = BTreeMap<(&'static str, CompactString), CompactString>;

/// Write `changes` into one manifest.
///
/// Returns how many ranges changed, which is zero when every one of them was
/// already what it was being set to.
///
/// # Errors
///
/// When the manifest cannot be read, is not JSON, or cannot be written. Also
/// when the rewritten text does not parse back to the manifest the change
/// described — which should be impossible, and is checked because the file
/// being rewritten is the one a project cannot install without.
pub fn apply(manifest: &Utf8Path, changes: &Changes) -> Result<usize, PackageManagerError> {
    let source = fs::read_to_string(manifest).map_err(|source| PackageManagerError::Read {
        path: manifest.to_path_buf(),
        source,
    })?;
    let original =
        serde_json::from_str::<Value>(&source).map_err(|source| PackageManagerError::Parse {
            path: manifest.to_path_buf(),
            source,
        })?;

    let mut expected = original.clone();
    let mut spliced = source.clone();
    let mut written = 0;
    let mut all_spliced = true;

    for ((field, name), range) in changes {
        let Some(object) = expected.get_mut(*field).and_then(Value::as_object_mut) else {
            continue;
        };
        let Some(slot) = object.get_mut(name.as_str()) else {
            continue;
        };
        let Some(previous) = slot.as_str().map(str::to_owned) else {
            continue;
        };
        if previous == range.as_str() {
            continue;
        }
        *slot = Value::String(range.to_string());
        written += 1;
        match splice(&spliced, name, &previous, range) {
            Some(next) => spliced = next,
            None => all_spliced = false,
        }
    }

    if written == 0 {
        return Ok(0);
    }

    let text = if all_spliced && parses_to(&spliced, &expected) {
        spliced
    } else {
        reserialize(&source, &expected)
    };
    // The last gate, and the reason either path is safe to take: whatever was
    // produced has to be the manifest the change described.
    if !parses_to(&text, &expected) {
        return Err(PackageManagerError::Write {
            path: manifest.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "the rewritten manifest is not the manifest the update described",
            ),
        });
    }

    fs::write(manifest, &text).map_err(|source| PackageManagerError::Write {
        path: manifest.to_path_buf(),
        source,
    })?;
    Ok(written)
}

/// Replace one `"name": "from"` pair, when there is exactly one of it.
///
/// `None` when there is none or more than one — which is the answer, not a
/// failure: the caller re-serialises instead.
fn splice(source: &str, name: &str, from: &str, to: &str) -> Option<String> {
    let key = format!("\"{name}\"");
    let value = format!("\"{from}\"");
    let mut found = None;

    let mut search = 0;
    while let Some(offset) = source[search..].find(&key) {
        let at = search + offset;
        search = at + key.len();
        let rest = &source[search..];
        // `"name"` then optional whitespace, a colon, optional whitespace, and
        // the range. Anything else is a different key that happens to share a
        // prefix, or a string that happens to contain the name.
        let after_key = rest.trim_start();
        let Some(after_colon) = after_key.strip_prefix(':') else {
            continue;
        };
        let after_colon = after_colon.trim_start();
        if !after_colon.starts_with(&value) {
            continue;
        }
        let start = source.len() - after_colon.len();
        if found.is_some() {
            // Ambiguous: the same name and range in two fields, say.
            return None;
        }
        found = Some(start..start + value.len());
    }

    let span = found?;
    let mut out = String::with_capacity(source.len() + to.len());
    out.push_str(&source[..span.start]);
    out.push('"');
    out.push_str(to);
    out.push('"');
    out.push_str(&source[span.end..]);
    Some(out)
}

/// The whole manifest again, in the indentation it was already in.
fn reserialize(source: &str, value: &Value) -> String {
    let indent = detected_indent(source);
    let mut out = Vec::with_capacity(source.len() + 64);
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
    // A `Value` cannot fail to serialise into a `Vec`, and a manifest that
    // somehow did would be caught by the check the caller makes next.
    if serde::Serialize::serialize(value, &mut serializer).is_err() {
        return source.to_owned();
    }
    let mut text = String::from_utf8(out).unwrap_or_else(|_| source.to_owned());
    if source.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// The first indented line's leading whitespace, or npm's two spaces.
fn detected_indent(source: &str) -> String {
    source
        .lines()
        .skip(1)
        .find_map(|line| {
            let indent: String = line
                .chars()
                .take_while(|character| *character == ' ' || *character == '\t')
                .collect();
            (!indent.is_empty() && indent.len() < line.len()).then_some(indent)
        })
        .unwrap_or_else(|| "  ".to_owned())
}

fn parses_to(text: &str, expected: &Value) -> bool {
    serde_json::from_str::<Value>(text).is_ok_and(|parsed| parsed == *expected)
}

#[cfg(test)]
mod tests;
