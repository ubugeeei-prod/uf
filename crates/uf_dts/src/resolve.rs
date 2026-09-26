//! Which declaration file a specifier names, by TypeScript's rules.
//!
//! A declaration file does not import declaration files. It imports the
//! *JavaScript* its package ships — `export * from "./schemas.cjs"` in zod,
//! `import type { DateArg } from "./types.ts"` in date-fns — and TypeScript
//! finds the declarations by substituting the extension: `.js` and `.ts`
//! become `.d.ts`, `.mjs` and `.mts` become `.d.mts`, `.cjs` and `.cts` become
//! `.d.cts`, and a specifier with no extension is tried as a file and then as
//! a directory with an `index`. This module is that substitution, and nothing
//! else: whether a candidate exists is the caller's question, because only
//! the caller can read.
//!
//! Every path here is relative to the package root and `/`-separated. A
//! specifier that climbs above the package root names nothing this
//! translation reads — a declaration file importing another package by a
//! relative path is not a shape anyone publishes, and following it would read
//! outside the directory the caller handed over.

use smallvec::SmallVec;

/// The declaration file extensions, longest match first.
const DECLARATION_EXTENSIONS: [&str; 3] = [".d.mts", ".d.cts", ".d.ts"];

/// Whether `path` names a TypeScript declaration file.
#[must_use]
pub fn is_declaration(path: &str) -> bool {
    DECLARATION_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(extension))
}

/// The path the Flow translation of `declaration` is filed under.
///
/// The declaration's own path with `.flow` after it: `index.d.cts` becomes
/// `index.d.cts.flow`. Two properties of that spelling are load-bearing.
/// Flow's parser reads a file whose name ends in `.flow` as a declaration
/// file, which is what the translation is, and it reads a file whose name
/// ends in `.d.ts`, `.d.mts` or `.d.cts` in a TypeScript mode that would
/// misread Flow — so the suffix has to be there and has to be last. And a
/// location inside the translation names the declaration file it came from,
/// which a reader can open.
#[must_use]
pub fn flow_path(declaration: &str) -> String {
    let mut path = String::with_capacity(declaration.len() + ".flow".len());
    path.push_str(declaration);
    path.push_str(".flow");
    path
}

/// The declaration files `base` may mean, in the order TypeScript tries them.
///
/// `base` is a path that has already been resolved against its importer — a
/// package's `main`, an `exports` target, or a subpath under the package
/// directory. A path that is already a declaration file means itself.
#[must_use]
pub fn declarations_for(base: &str) -> SmallVec<[String; 4]> {
    declarations_with(base, Flavor::Script)
}

/// Whether `specifier` names a file relative to the importer.
pub(crate) fn is_relative(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

/// The declaration files a relative `specifier` written in `importer` may
/// mean, in TypeScript's order, or nothing when it climbs out of the package.
///
/// An extensionless specifier prefers the importer's own flavour: a `.d.cts`
/// file importing `./util` means `./util.d.cts` when the package ships both,
/// which is what TypeScript resolves for a CommonJS importer.
pub(crate) fn candidates(importer: &str, specifier: &str) -> SmallVec<[String; 4]> {
    let Some(base) = join(importer, specifier) else {
        return SmallVec::new();
    };
    declarations_with(&base, Flavor::of(importer))
}

/// A relative specifier from `importer` to `target`, both package paths.
///
/// Always starts with `./` or `../`, because a bare path would be read as a
/// package name.
pub(crate) fn relative_specifier(importer: &str, target: &str) -> String {
    let from: Vec<&str> = directory(importer)
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let to: Vec<&str> = target.split('/').collect();
    let (to_directory, file) = to.split_at(to.len().saturating_sub(1));
    let shared = from
        .iter()
        .zip(to_directory)
        .take_while(|(left, right)| left == right)
        .count();

    let mut specifier = String::new();
    if shared == from.len() {
        specifier.push_str("./");
    } else {
        for _ in shared..from.len() {
            specifier.push_str("../");
        }
    }
    for segment in &to_directory[shared..] {
        specifier.push_str(segment);
        specifier.push('/');
    }
    specifier.push_str(file.first().copied().unwrap_or_default());
    specifier
}

/// `path` with `.` segments removed and `..` segments applied, or [`None`]
/// when a `..` climbs above the root.
pub(crate) fn normalize(path: &str) -> Option<String> {
    let mut segments: SmallVec<[&str; 8]> = SmallVec::new();
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

/// Which kind of module an importer is, for an extensionless specifier.
#[derive(Clone, Copy)]
enum Flavor {
    /// `.d.ts`: resolved as a module of the package's own `type`.
    Script,
    /// `.d.mts`: an ES module whatever the package says.
    Module,
    /// `.d.cts`: CommonJS whatever the package says.
    CommonJs,
}

impl Flavor {
    fn of(path: &str) -> Self {
        if path.ends_with(".d.mts") {
            Self::Module
        } else if path.ends_with(".d.cts") {
            Self::CommonJs
        } else {
            Self::Script
        }
    }

    /// The declaration extensions tried for an extensionless path, preferred
    /// first.
    fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Script => &[".d.ts"],
            Self::Module => &[".d.mts", ".d.ts"],
            Self::CommonJs => &[".d.cts", ".d.ts"],
        }
    }
}

fn declarations_with(base: &str, flavor: Flavor) -> SmallVec<[String; 4]> {
    let mut found = SmallVec::new();
    if is_declaration(base) {
        found.push(base.to_owned());
        return found;
    }
    let file = base.rsplit_once('/').map_or(base, |(_, file)| file);
    if let Some((stem_len, extension)) = file.rfind('.').map(|dot| {
        let stem_len = base.len() - file.len() + dot;
        (stem_len, &file[dot + 1..])
    }) {
        let stem = &base[..stem_len];
        let substituted = match extension {
            "js" | "jsx" | "ts" | "tsx" => Some(".d.ts"),
            "mjs" | "mts" => Some(".d.mts"),
            "cjs" | "cts" => Some(".d.cts"),
            // A JSON module has no declaration file; TypeScript types it from
            // the JSON itself, which is not something this crate reads.
            "json" => return found,
            _ => None,
        };
        if let Some(substituted) = substituted {
            found.push(uf_infra::cstr!("{stem}{substituted}").into_string());
            return found;
        }
    }
    for extension in flavor.extensions() {
        found.push(uf_infra::cstr!("{base}{extension}").into_string());
        found.push(uf_infra::cstr!("{base}/index{extension}").into_string());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_javascript_specifier_means_the_declaration_beside_it() {
        assert_eq!(
            candidates("v4/classic/external.d.cts", "./schemas.cjs").as_slice(),
            ["v4/classic/schemas.d.cts"]
        );
        assert_eq!(
            candidates("index.d.ts", "./format/index.js").as_slice(),
            ["format/index.d.ts"]
        );
        assert_eq!(candidates("index.d.mts", "./a.mjs").as_slice(), ["a.d.mts"]);
    }

    #[test]
    fn a_typescript_source_specifier_means_its_declaration() {
        // date-fns writes `from "./types.ts"`.
        assert_eq!(
            candidates("addDays.d.ts", "./types.ts").as_slice(),
            ["types.d.ts"]
        );
        assert_eq!(candidates("a/b.d.cts", "../c.cts").as_slice(), ["c.d.cts"]);
    }

    #[test]
    fn an_extensionless_specifier_is_a_file_then_a_directory_in_the_importers_flavour() {
        assert_eq!(
            candidates("index.d.ts", "./util").as_slice(),
            ["util.d.ts", "util/index.d.ts"]
        );
        assert_eq!(
            candidates("index.d.cts", "./util").as_slice(),
            [
                "util.d.cts",
                "util/index.d.cts",
                "util.d.ts",
                "util/index.d.ts"
            ]
        );
    }

    #[test]
    fn a_declaration_specifier_means_itself() {
        assert_eq!(
            candidates("index.d.ts", "./types.d.ts").as_slice(),
            ["types.d.ts"]
        );
    }

    #[test]
    fn a_specifier_that_leaves_the_package_means_nothing() {
        assert!(candidates("index.d.ts", "../outside.js").is_empty());
        assert!(candidates("a/index.d.ts", "../../outside.js").is_empty());
    }

    #[test]
    fn a_json_module_has_no_declaration_file() {
        assert!(candidates("index.d.ts", "./package.json").is_empty());
    }

    #[test]
    fn a_relative_specifier_climbs_only_as_far_as_it_has_to() {
        assert_eq!(
            relative_specifier("v4/classic/schemas.d.cts", "v4/core/index.d.cts.flow"),
            "../core/index.d.cts.flow"
        );
        assert_eq!(
            relative_specifier("index.d.cts", "v4/classic/external.d.cts.flow"),
            "./v4/classic/external.d.cts.flow"
        );
        assert_eq!(
            relative_specifier("a/b/c.d.ts", "a/b/d.d.ts.flow"),
            "./d.d.ts.flow"
        );
        assert_eq!(
            relative_specifier("a/b/c.d.ts", "e.d.ts.flow"),
            "../../e.d.ts.flow"
        );
    }

    #[test]
    fn the_translation_is_filed_after_the_declaration_it_came_from() {
        assert_eq!(flow_path("index.d.cts"), "index.d.cts.flow");
        assert!(is_declaration("index.d.cts"));
        assert!(is_declaration("a/b.d.mts"));
        assert!(!is_declaration("index.ts"));
        assert!(!is_declaration("index.d.cts.flow"));
    }

    #[test]
    fn a_package_entry_resolves_without_an_importer() {
        assert_eq!(
            declarations_for("build/modern/index.js").as_slice(),
            ["build/modern/index.d.ts"]
        );
        assert_eq!(
            declarations_for("lib").as_slice(),
            ["lib.d.ts", "lib/index.d.ts"]
        );
    }
}
