//! Expressing one project-relative module path relative to another.
//!
//! Resolution happens once, in [`crate::resolve`], and the answer is a
//! project-relative path. Handing that answer back to `uf_rsc` means turning it
//! into the shape a specifier has — `./x.js` or `../y/z.js` — so the RSC graph
//! resolves it to the same module the bundler already chose.

use camino::Utf8Path;

/// A specifier for `to` as written inside `from`.
///
/// Always starts with `./` or `../`, so [`uf_rsc::resolve_specifier`] reads it
/// as a relative specifier rather than a package name.
#[must_use]
pub fn relative_specifier(from: &Utf8Path, to: &Utf8Path) -> String {
    let base: Vec<&str> = from
        .parent()
        .map(Utf8Path::as_str)
        .unwrap_or_default()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let target: Vec<&str> = to
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();

    let shared = base
        .iter()
        .zip(&target)
        .take_while(|(left, right)| left == right)
        .count();

    let mut specifier = String::with_capacity(to.as_str().len() + 8);
    if base.len() == shared {
        specifier.push_str("./");
    } else {
        for _ in shared..base.len() {
            specifier.push_str("../");
        }
    }
    for (position, segment) in target[shared.min(target.len())..].iter().enumerate() {
        if position > 0 {
            specifier.push('/');
        }
        specifier.push_str(segment);
    }
    specifier
}
