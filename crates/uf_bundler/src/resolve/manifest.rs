//! Reading the parts of a `package.json` that decide resolution.
//!
//! A manifest is untrusted: it comes from `node_modules`, which a dependency
//! writes. It is read once per package, bounded in size, and only four fields
//! are ever looked at — everything else in the file is data the bundler has no
//! business acting on.

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde_json::Value;

use crate::limits::{BundlerLimits, LimitError};

/// Keys that must never be treated as data keys.
///
/// A JSON object with a `__proto__` member is the prototype-pollution class
/// (CVE-2018-3721 and the long tail after it). Rust has no prototype chain to
/// pollute, but a manifest that names one of these is not a manifest anyone
/// wrote by accident, so the whole `exports` map is refused rather than
/// half-read.
pub const POISONED_KEYS: &[&str] = &["__proto__", "constructor", "prototype"];

/// What a package says about its own side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SideEffectsField {
    /// The manifest says nothing, so the bundler must decide for itself.
    #[default]
    Unspecified,
    /// `"sideEffects": false` — every module in the package is droppable when
    /// nothing imports anything from it.
    None,
    /// `"sideEffects": true`, or a list of files, which this bundler does not
    /// narrow: the whole package is treated as having side effects.
    Present,
}

/// The fields of a `package.json` that resolution and shaking depend on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    /// Directory the manifest sits in, relative to the project root.
    pub directory: Utf8PathBuf,
    /// The declared package name, when there is one.
    pub name: CompactString,
    /// The `exports` map, when the package has one.
    pub exports: Option<Value>,
    /// The legacy `main` entry, when the package has one.
    pub main: Option<CompactString>,
    /// What the package says about side effects.
    pub side_effects: SideEffectsField,
}

impl PackageManifest {
    /// An empty manifest for a directory with no readable `package.json`.
    #[must_use]
    pub fn empty(directory: Utf8PathBuf) -> Self {
        Self {
            directory,
            name: CompactString::default(),
            exports: None,
            main: None,
            side_effects: SideEffectsField::Unspecified,
        }
    }

    /// Read `<directory>/package.json`, or produce an empty manifest.
    ///
    /// A manifest that cannot be read, is too large, is not JSON, or carries a
    /// poisoned key resolves as if the package had no manifest at all: the
    /// caller falls back to `index.js`, which is what Node does for a directory
    /// without one.
    pub fn read(
        root: &Utf8Path,
        directory: &Utf8Path,
        limits: &BundlerLimits,
    ) -> Result<Self, LimitError> {
        let absolute = root.join(directory).join("package.json");
        let Ok(metadata) = std::fs::symlink_metadata(&absolute) else {
            return Ok(Self::empty(directory.to_path_buf()));
        };
        if metadata.is_symlink() || !metadata.is_file() {
            return Ok(Self::empty(directory.to_path_buf()));
        }
        if metadata.len() > limits.max_manifest_bytes {
            return Err(LimitError::ManifestTooLarge {
                manifest: directory.join("package.json"),
                bytes: metadata.len(),
                limit: limits.max_manifest_bytes,
            });
        }
        let Ok(text) = std::fs::read_to_string(&absolute) else {
            return Ok(Self::empty(directory.to_path_buf()));
        };
        let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&text) else {
            return Ok(Self::empty(directory.to_path_buf()));
        };

        let exports = object.get("exports").cloned();
        if exports.as_ref().is_some_and(has_poisoned_key) {
            return Ok(Self::empty(directory.to_path_buf()));
        }

        Ok(Self {
            directory: directory.to_path_buf(),
            name: object
                .get("name")
                .and_then(Value::as_str)
                .map(CompactString::new)
                .unwrap_or_default(),
            exports,
            main: object
                .get("main")
                .and_then(Value::as_str)
                .map(CompactString::new),
            side_effects: side_effects_of(object.get("sideEffects")),
        })
    }
}

fn side_effects_of(value: Option<&Value>) -> SideEffectsField {
    match value {
        None => SideEffectsField::Unspecified,
        Some(Value::Bool(false)) => SideEffectsField::None,
        Some(_) => SideEffectsField::Present,
    }
}

/// Whether an `exports` map names a key no honest manifest uses.
fn has_poisoned_key(value: &Value) -> bool {
    // Iterative rather than recursive: the map comes from `node_modules`, and a
    // deeply nested one must not be able to overflow the stack before the
    // depth check in `exports` ever runs.
    let mut stack = vec![value];
    let mut visited = 0usize;

    while let Some(current) = stack.pop() {
        visited += 1;
        if visited > MAX_MANIFEST_NODES {
            return true;
        }
        match current {
            Value::Object(object) => {
                for (key, nested) in object {
                    if POISONED_KEYS.contains(&key.as_str()) {
                        return true;
                    }
                    stack.push(nested);
                }
            }
            Value::Array(items) => stack.extend(items.iter()),
            _ => {}
        }
    }

    false
}

/// Most JSON nodes an `exports` map may hold.
const MAX_MANIFEST_NODES: usize = 100_000;
