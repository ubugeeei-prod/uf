//! Which module a specifier names, and what its declaration file is called.
//!
//! A Flow library imports its own modules the way it will ship them —
//! `import { parse } from "./internal/parse.js"` — so the walk here follows
//! the specifier as written and stops at anything that is not relative: a
//! bare specifier is somebody else's package, and its types are that
//! package's problem rather than this translation's.
//!
//! # Why an emitted specifier is never rewritten
//!
//! The declarations are written beside the JavaScript, under the same names:
//! `index.js` becomes `dist/index.d.ts` and `internal/parse.js` becomes
//! `dist/internal/parse.d.ts`. So a declaration file sits at exactly the
//! relative position its source did, and `"./internal/parse.js"` — the
//! specifier the author wrote — still resolves, because TypeScript looks for
//! `./internal/parse.d.ts` when it is asked for `./internal/parse.js`. The
//! specifier is copied through untouched, which is one fewer thing that can
//! be wrong and the reason there is no `relative_specifier` here.
//!
//! Every path is relative to the project root and `/`-separated. A specifier
//! that climbs above the root names nothing this translation reads.

/// Whether `specifier` names a module relative to the importer.
pub(crate) fn is_relative(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

/// The declaration file written for the module `path`.
///
/// The extension decides, and it decides the way TypeScript reads it back:
/// `.mjs` is always an ES module and `.cjs` is always CommonJS, so each keeps
/// its own declaration extension rather than collapsing to `.d.ts`, which
/// would be read under the package's `type` instead.
#[must_use]
pub fn declaration_path(path: &str) -> String {
    for (extension, declaration) in [
        (".mjs", ".d.mts"),
        (".cjs", ".d.cts"),
        (".jsx", ".d.ts"),
        (".js", ".d.ts"),
    ] {
        if let Some(stem) = path.strip_suffix(extension) {
            return uf_infra::into_string(uf_infra::cstr!("{stem}{declaration}"));
        }
    }
    uf_infra::into_string(uf_infra::cstr!("{path}.d.ts"))
}

/// The modules a relative `specifier` written in `importer` may mean, in the
/// order they are tried, or nothing when it climbs out of the project.
///
/// Node's own order for an ES module, minus the extensions a Flow library
/// does not have: the specifier as written, then with `.js` added, then as a
/// directory with an `index.js` in it.
pub(crate) fn candidates(importer: &str, specifier: &str) -> Vec<String> {
    let Some(base) = join(importer, specifier) else {
        return Vec::new();
    };
    if has_module_extension(&base) {
        return vec![base];
    }
    let mut found = Vec::with_capacity(4);
    for extension in [".js", ".jsx", ".mjs", ".cjs"] {
        found.push(uf_infra::into_string(uf_infra::cstr!("{base}{extension}")));
    }
    found.push(uf_infra::into_string(uf_infra::cstr!("{base}/index.js")));
    found
}

/// Whether `path` already ends in an extension a module is written with.
fn has_module_extension(path: &str) -> bool {
    let file = path.rsplit_once('/').map_or(path, |(_, file)| file);
    [".js", ".jsx", ".mjs", ".cjs"]
        .iter()
        .any(|extension| file.ends_with(extension))
}

/// `path` with `.` segments removed and `..` segments applied, or [`None`]
/// when a `..` climbs above the root.
pub(crate) fn normalize(path: &str) -> Option<String> {
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            segment => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}

/// `specifier` resolved against the directory `importer` is in.
fn join(importer: &str, specifier: &str) -> Option<String> {
    let directory = directory(importer);
    if directory.is_empty() {
        normalize(specifier)
    } else {
        normalize(uf_infra::cstr!("{directory}/{specifier}").as_str())
    }
}

/// The directory a path is in, or the empty string for the root.
fn directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(head, _)| head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declaration_is_named_after_the_module_it_describes() {
        assert_eq!(declaration_path("index.js"), "index.d.ts");
        assert_eq!(declaration_path("internal/parse.js"), "internal/parse.d.ts");
        assert_eq!(declaration_path("index.mjs"), "index.d.mts");
        assert_eq!(declaration_path("index.cjs"), "index.d.cts");
    }

    #[test]
    fn a_relative_specifier_is_resolved_against_its_importer() {
        assert_eq!(
            candidates("index.js", "./internal/parse.js"),
            ["internal/parse.js"]
        );
        assert_eq!(candidates("internal/parse.js", "../index.js"), ["index.js"]);
    }

    #[test]
    fn an_extensionless_specifier_is_a_file_then_a_directory() {
        assert_eq!(
            candidates("index.js", "./util"),
            [
                "util.js",
                "util.jsx",
                "util.mjs",
                "util.cjs",
                "util/index.js"
            ]
        );
    }

    #[test]
    fn a_specifier_that_leaves_the_project_means_nothing() {
        assert!(candidates("index.js", "../outside.js").is_empty());
        assert!(candidates("a/index.js", "../../outside.js").is_empty());
    }

    #[test]
    fn a_bare_specifier_is_somebody_elses_package() {
        assert!(!is_relative("react"));
        assert!(!is_relative("@scope/pkg/deep"));
        assert!(is_relative("./a.js"));
        assert!(is_relative("../a.js"));
    }
}
