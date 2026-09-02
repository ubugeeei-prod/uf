//! `package.json#exports`: subpaths, conditions, and the guard around targets.
//!
//! The map is the package's own statement of what it is willing to expose, so
//! resolving it wrong either breaks a legal import or reaches a file the
//! package deliberately hid. Two rules keep the second from happening: a target
//! must be a relative path starting with `./`, and it must not climb — so
//! `"./secret": "../../../etc/passwd"` resolves to nothing rather than to a
//! file outside the package.

use serde_json::Value;

use crate::limits::{BundlerLimits, LimitError};

/// Export conditions `uf` matches, most specific first.
///
/// `uf` comes first so a package can ship a build made for this toolchain, then
/// `import` because every uf module is an ES module, then `default`. `require`
/// is deliberately absent: nothing uf emits is CommonJS.
pub const CONDITIONS: &[&str] = &["uf", "import", "default"];

/// Resolve `subpath` through an `exports` map.
///
/// `subpath` is `"."` for the package root and `"./name"` for a subpath. The
/// answer is the package-relative target, without its leading `./`.
pub fn resolve_exports(
    exports: &Value,
    subpath: &str,
    package: &str,
    limits: &BundlerLimits,
) -> Result<Option<String>, LimitError> {
    let entry = match exports {
        // A bare string or array only ever describes the package root.
        Value::String(_) | Value::Array(_) => {
            if subpath == "." {
                Some(Wildcard::none(exports))
            } else {
                None
            }
        }
        Value::Object(map) => {
            if map.keys().any(|key| key.starts_with('.')) {
                subpath_entry(map, subpath)
            } else if subpath == "." {
                Some(Wildcard::none(exports))
            } else {
                None
            }
        }
        _ => None,
    };

    let Some(entry) = entry else {
        return Ok(None);
    };
    let Some(target) = select_condition(entry.value, 0, package, limits)? else {
        return Ok(None);
    };
    Ok(expand(&target, entry.matched.as_deref()))
}

/// One matched entry of a subpath map, plus what a `*` captured.
struct Wildcard<'a> {
    value: &'a Value,
    matched: Option<String>,
}

impl<'a> Wildcard<'a> {
    fn none(value: &'a Value) -> Self {
        Self {
            value,
            matched: None,
        }
    }
}

/// Find the entry for `subpath`, preferring an exact key over a pattern.
fn subpath_entry<'a>(
    map: &'a serde_json::Map<String, Value>,
    subpath: &str,
) -> Option<Wildcard<'a>> {
    if let Some(value) = map.get(subpath) {
        return Some(Wildcard::none(value));
    }

    // Node picks the pattern with the longest literal prefix; ties are broken
    // on the suffix for the same reason, so the answer never depends on the
    // order keys happen to appear in the file.
    let mut best: Option<(usize, Wildcard<'a>)> = None;
    for (key, value) in map {
        let Some((prefix, suffix)) = key.split_once('*') else {
            continue;
        };
        if suffix.contains('*') {
            continue;
        }
        let Some(rest) = subpath.strip_prefix(prefix) else {
            continue;
        };
        let Some(captured) = rest.strip_suffix(suffix) else {
            continue;
        };
        if captured.is_empty() && !suffix.is_empty() && rest.len() < suffix.len() {
            continue;
        }
        let score = prefix.len();
        if best.as_ref().is_some_and(|(best, _)| *best >= score) {
            continue;
        }
        best = Some((
            score,
            Wildcard {
                value,
                matched: Some(captured.to_string()),
            },
        ));
    }

    best.map(|(_, entry)| entry)
}

/// Walk condition objects and fallback arrays down to a string target.
fn select_condition(
    value: &Value,
    depth: usize,
    package: &str,
    limits: &BundlerLimits,
) -> Result<Option<String>, LimitError> {
    if depth > limits.max_exports_depth {
        return Err(LimitError::ExportsTooDeep {
            package: compact_str::CompactString::new(package),
            limit: limits.max_exports_depth,
        });
    }

    match value {
        Value::String(target) => Ok(Some(target.clone())),
        // `null` is how a package says "this subpath is not exported".
        Value::Null => Ok(None),
        Value::Array(items) => {
            for item in items {
                if let Some(target) = select_condition(item, depth + 1, package, limits)? {
                    return Ok(Some(target));
                }
            }
            Ok(None)
        }
        Value::Object(map) => {
            for condition in CONDITIONS {
                if let Some(nested) = map.get(*condition)
                    && let Some(target) = select_condition(nested, depth + 1, package, limits)?
                {
                    return Ok(Some(target));
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

/// Substitute a captured `*` and check the target stays inside the package.
fn expand(target: &str, captured: Option<&str>) -> Option<String> {
    let expanded = match captured {
        Some(captured) => target.replace('*', captured),
        None => target.to_string(),
    };
    let relative = expanded.strip_prefix("./")?;

    if relative.is_empty() || relative.contains('\\') {
        return None;
    }
    // A target that climbs, is absolute, or hides a `..` in a segment leaves
    // the package. There is no reading of that which is a legitimate export.
    if relative
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }

    Some(relative.to_string())
}
