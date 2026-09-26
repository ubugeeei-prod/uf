//! Which declaration file TypeScript reads for a package specifier.
//!
//! `uf check` finds a dependency by the file its *runtime* loads: the `flow` or
//! `import` condition of an `exports` map, or `module` and then `main`.
//! TypeScript finds the declarations for the same specifier by rules of its
//! own, and a package's types are written against those rules — a `types`
//! condition beside `import`, a `types` field beside `main`, a `typesVersions`
//! map that sends one compiler to one directory and another to another, and,
//! for a package that ships no declarations at all, a second package,
//! `@types/<name>`, that describes it. A translation has to start from the file
//! TypeScript would have read, or a consumer is checked against types nobody
//! wrote the package against.
//!
//! Like [`crate::resolve`], nothing here reads a file. Whether a candidate
//! exists is asked of the caller one path at a time, in the order TypeScript
//! would try them, so the first `true` is TypeScript's answer.
//!
//! What is left out is left out on purpose. TypeScript applies `typesVersions`
//! only when there is no `exports` map, under the resolution modes a package
//! consumed as an ES module is resolved in, and so does this. It resolves a
//! `node` or `browser` condition only when the project names one, and `uf
//! check` names no host — `crate::resolve`'s sibling in `uf_check`, which picks
//! the JavaScript, answers `flow` and `import` and no host for the reason its
//! header gives, and the declaration a condition map leads to must be the one
//! for the same graph.

use std::cmp::Ordering;

use serde_json::{Map, Value};
use smallvec::{SmallVec, smallvec};

use crate::resolve::{declarations_for, normalize};

/// The `exports` conditions a declaration is resolved under.
///
/// A set, not a priority order: an `exports` map is matched in the order its
/// keys are written, exactly as Node matches one. `types` is the condition
/// TypeScript adds for itself; `import` is the one `uf check` answers for the
/// JavaScript, because it checks the ES module graph; `default` is the one
/// every resolver matches.
pub const DECLARATION_CONDITIONS: &[&str] = &["types", "import", "default"];

/// The TypeScript release a `typesVersions` range or a `types@` condition is
/// matched against.
///
/// A package that publishes either is naming the compiler each set of
/// declarations is written for, and the set nearest to what this crate reads
/// is the newest one: an older directory exists to spell newer syntax the long
/// way for a compiler that cannot read it, and the long way is where a
/// translation loses precision rather than gains it.
const TYPESCRIPT: Version = Version {
    major: 5,
    minor: 9,
    patch: 0,
};

/// A package manifest, read the way TypeScript reads one.
#[derive(Debug, Clone)]
pub struct Manifest {
    fields: Map<String, Value>,
}

impl Manifest {
    /// Parse a `package.json`, or [`None`] when it is not a JSON object.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match serde_json::from_str(text).ok()? {
            Value::Object(fields) => Some(Self { fields }),
            _ => None,
        }
    }

    /// The name the manifest publishes.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.string("name")
    }

    /// The version the manifest publishes.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.string("version")
    }

    /// The declaration file TypeScript reads for `subpath` of this package, or
    /// [`None`] when it reads none.
    ///
    /// `subpath` is in the shape an `exports` map is keyed by: `.` for the
    /// package itself, `./v4/core` for `zod/v4/core`. The answer is relative
    /// to the package root, and it is the first candidate `exists` accepted.
    ///
    /// [`None`] is also the answer for a subpath an `exports` map does not
    /// list, even when a declaration file sits at that path: the map is what
    /// makes a subpath importable, for TypeScript as for Node, and a consumer
    /// typed against a file its runtime would refuse to load has been told
    /// something false.
    pub fn declaration(
        &self,
        subpath: &str,
        exists: &mut dyn FnMut(&str) -> bool,
    ) -> Option<String> {
        match self.fields.get("exports") {
            Some(exports) => from_exports(exports, subpath, exists),
            None => self.through_fields(subpath, exists),
        }
    }

    fn string(&self, field: &str) -> Option<&str> {
        self.fields.get(field)?.as_str()
    }

    /// A package with no `exports` map: the package itself is its `types`,
    /// then `typings`, then what its `main` means, then its `index`; any other
    /// subpath is a path under the package directory.
    ///
    /// Each field is tried through to the files it may mean before the next
    /// field is read, because that is TypeScript's order: a `types` naming a
    /// file the package forgot to publish falls back to `main` rather than
    /// ending the search.
    fn through_fields(
        &self,
        subpath: &str,
        exists: &mut dyn FnMut(&str) -> bool,
    ) -> Option<String> {
        if subpath != "." {
            return self.first_declaration(subpath, exists);
        }
        ["types", "typings", "main"]
            .into_iter()
            .filter_map(|field| self.string(field))
            .find_map(|written| self.first_declaration(written, exists))
            .or_else(|| self.first_declaration("index", exists))
    }

    /// The first declaration file `written` may mean, after `typesVersions`
    /// has redirected it.
    fn first_declaration(
        &self,
        written: &str,
        exists: &mut dyn FnMut(&str) -> bool,
    ) -> Option<String> {
        self.types_versions(written).into_iter().find_map(|path| {
            declarations_for(&path)
                .into_iter()
                .find(|candidate| exists(candidate))
        })
    }

    /// Where `typesVersions` sends a package path, or the path itself when no
    /// range matches or no pattern in the matching range does.
    ///
    /// The first range that matches wins, in the order the manifest writes
    /// them — TypeScript's rule, and the reason packages write the narrow
    /// ranges first. Inside it, an exact key beats a pattern and a longer
    /// pattern prefix beats a shorter one, and every target the key lists is
    /// tried in order.
    fn types_versions(&self, written: &str) -> SmallVec<[String; 2]> {
        let Some(path) = normalize(written) else {
            return SmallVec::new();
        };
        let Some(Value::Object(ranges)) = self.fields.get("typesVersions") else {
            return smallvec![path];
        };
        let Some(Value::Object(patterns)) = ranges
            .iter()
            .find(|(range, _)| range_matches(range, TYPESCRIPT))
            .map(|(_, patterns)| patterns)
        else {
            return smallvec![path];
        };
        let Some((targets, matched)) = pattern_entry(patterns, &path) else {
            return smallvec![path];
        };
        let redirected: SmallVec<[String; 2]> = match targets {
            Value::Array(targets) => targets
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|target| normalize(&substitute(target, matched)))
                .collect(),
            _ => SmallVec::new(),
        };
        if redirected.is_empty() {
            smallvec![path]
        } else {
            redirected
        }
    }
}

/// The `@types` package that describes `name`, by DefinitelyTyped's naming
/// rule: `@types/lodash` for `lodash`, `@types/babel__core` for
/// `@babel/core`.
#[must_use]
pub fn types_package(name: &str) -> String {
    match name
        .strip_prefix('@')
        .and_then(|scoped| scoped.split_once('/'))
    {
        Some((scope, bare)) => uf_infra::into_string(uf_infra::cstr!("@types/{scope}__{bare}")),
        None => uf_infra::into_string(uf_infra::cstr!("@types/{name}")),
    }
}

/// What a target in an `exports` map came to.
enum Target {
    /// A declaration file that exists.
    Found(String),
    /// `null`: the package says this subpath is not importable under this
    /// condition, and the search stops rather than trying the next one.
    Blocked,
}

/// Resolve `subpath` through an `exports` map.
fn from_exports(
    exports: &Value,
    subpath: &str,
    exists: &mut dyn FnMut(&str) -> bool,
) -> Option<String> {
    let (target, matched) = match exports {
        // Keyed by subpath. Node decides the shape by the first key, and a
        // map mixing the two shapes is one Node refuses; reading it by its
        // first key is the answer that refuses the least.
        Value::Object(map) if map.keys().next().is_some_and(|key| key.starts_with('.')) => {
            pattern_entry(map, subpath)?
        }
        // A string, an array or a map of conditions is the package's own
        // entry written without its `.` key, and names no other subpath.
        _ if subpath == "." => (exports, None),
        _ => return None,
    };
    match resolve_target(target, matched, exists)? {
        Target::Found(path) => Some(path),
        Target::Blocked => None,
    }
}

/// The entry `subpath` selects from a map keyed by subpaths or by
/// `typesVersions` path patterns, and what its `*` matched.
///
/// An exact key wins. Otherwise the pattern with the longest prefix before its
/// `*` does, and between two of equal prefix the longer key — Node's
/// `PATTERN_KEY_COMPARE`, which TypeScript follows for both maps.
fn pattern_entry<'m, 's>(
    map: &'m Map<String, Value>,
    subpath: &'s str,
) -> Option<(&'m Value, Option<&'s str>)> {
    if !subpath.contains('*')
        && let Some(entry) = map.get(subpath)
    {
        return Some((entry, None));
    }
    let mut best: Option<(&str, &Value, &str)> = None;
    for (key, entry) in map {
        let Some((prefix, trailer)) = key.split_once('*') else {
            continue;
        };
        if trailer.contains('*') || !subpath.starts_with(prefix) || subpath == prefix {
            continue;
        }
        if !trailer.is_empty() && !(subpath.ends_with(trailer) && subpath.len() >= key.len()) {
            continue;
        }
        if best.is_none_or(|(held, _, _)| more_specific(key, held)) {
            let matched = &subpath[prefix.len()..subpath.len() - trailer.len()];
            best = Some((key, entry, matched));
        }
    }
    best.map(|(_, entry, matched)| (entry, Some(matched)))
}

/// Whether pattern `key` is more specific than pattern `held`.
fn more_specific(key: &str, held: &str) -> bool {
    let base = |pattern: &str| pattern.find('*').map_or(pattern.len(), |star| star + 1);
    match base(key).cmp(&base(held)) {
        Ordering::Equal => key.len() > held.len(),
        ordering => ordering == Ordering::Greater,
    }
}

/// Follow one target of an `exports` map to a declaration file.
///
/// [`None`] means this target leads nowhere and the caller should try the
/// next: a condition this resolution does not answer, or a file that is not
/// there. TypeScript continues past a condition whose file is missing, which
/// is how a package that writes `types` first and forgets to publish the file
/// still resolves through `import`.
fn resolve_target(
    target: &Value,
    matched: Option<&str>,
    exists: &mut dyn FnMut(&str) -> bool,
) -> Option<Target> {
    match target {
        Value::String(written) => {
            let written = substitute(written, matched);
            // A target must be a path inside the package, spelled `./…`;
            // anything else is one Node refuses to load.
            let relative = written.strip_prefix("./")?;
            let path = normalize(relative)?;
            declarations_for(&path)
                .into_iter()
                .find(|candidate| exists(candidate))
                .map(Target::Found)
        }
        Value::Array(targets) => {
            targets
                .iter()
                .find_map(|target| match resolve_target(target, matched, exists) {
                    Some(Target::Found(path)) => Some(Target::Found(path)),
                    Some(Target::Blocked) | None => None,
                })
        }
        Value::Object(conditions) => conditions
            .iter()
            .filter(|(condition, _)| answers(condition))
            .find_map(|(_, target)| resolve_target(target, matched, exists)),
        Value::Null => Some(Target::Blocked),
        Value::Bool(_) | Value::Number(_) => None,
    }
}

/// Whether a declaration resolution answers `condition`.
fn answers(condition: &str) -> bool {
    DECLARATION_CONDITIONS.contains(&condition)
        || condition
            .strip_prefix("types@")
            .is_some_and(|range| range_matches(range, TYPESCRIPT))
}

/// `written` with every `*` replaced by what a pattern matched.
fn substitute(written: &str, matched: Option<&str>) -> String {
    match matched {
        Some(matched) => written.replace('*', matched),
        None => written.to_owned(),
    }
}

/// A TypeScript release, for range matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

/// Whether `version` is in a `typesVersions` range.
///
/// The grammar TypeScript accepts: alternatives joined by `||`, each a list of
/// comparisons that must all hold, each an operator (`<`, `<=`, `>`, `>=`, `=`
/// or none) and a version whose missing parts are zero — except with no
/// operator or `=`, where a missing part matches anything, so `4` is every
/// 4.x. A comparison this cannot read matches nothing, which sends the
/// resolution to the declarations the package publishes without a redirect.
fn range_matches(range: &str, version: Version) -> bool {
    range.split("||").any(|alternative| {
        let mut comparisons = alternative.split_whitespace().peekable();
        comparisons.peek().is_some()
            && comparisons.all(|comparison| comparison_matches(comparison, version))
    })
}

fn comparison_matches(comparison: &str, version: Version) -> bool {
    if matches!(comparison, "*" | "x" | "X") {
        return true;
    }
    let (operator, operand) = ["<=", ">=", "<", ">", "="]
        .into_iter()
        .find_map(|operator| {
            comparison
                .strip_prefix(operator)
                .map(|operand| (operator, operand))
        })
        .unwrap_or(("=", comparison));
    let mut parts = operand.split('.');
    let mut part = || -> Option<Option<u64>> {
        match parts.next() {
            None | Some("*" | "x" | "X") => Some(None),
            Some(digits) => digits.parse().ok().map(Some),
        }
    };
    let (Some(major), Some(minor), Some(patch)) = (part(), part(), part()) else {
        return false;
    };
    let Some(major) = major else {
        return true;
    };
    let bound = Version {
        major,
        minor: minor.unwrap_or(0),
        patch: patch.unwrap_or(0),
    };
    match operator {
        "<" => version < bound,
        "<=" => version <= bound,
        ">" => version > bound,
        ">=" => version >= bound,
        _ => {
            version.major == major
                && minor.is_none_or(|minor| version.minor == minor)
                && patch.is_none_or(|patch| version.patch == patch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolve `subpath` of the manifest `json` against a package holding
    /// exactly `files`.
    fn declaration(json: &str, subpath: &str, files: &[&str]) -> Option<String> {
        let manifest = Manifest::parse(json).expect("the manifest parses");
        manifest.declaration(subpath, &mut |path| files.contains(&path))
    }

    #[test]
    fn a_types_field_is_the_package_entry() {
        let json = r#"{ "main": "./lib/index.js", "types": "./lib/types.d.ts" }"#;
        assert_eq!(
            declaration(json, ".", &["lib/index.d.ts", "lib/types.d.ts"]).as_deref(),
            Some("lib/types.d.ts")
        );
    }

    #[test]
    fn typings_is_read_when_there_is_no_types_field() {
        let json = r#"{ "typings": "dist/index.d.ts" }"#;
        assert_eq!(
            declaration(json, ".", &["dist/index.d.ts"]).as_deref(),
            Some("dist/index.d.ts")
        );
    }

    #[test]
    fn main_means_the_declaration_beside_it_when_no_field_names_one() {
        let json = r#"{ "main": "./lib/index.cjs" }"#;
        assert_eq!(
            declaration(json, ".", &["lib/index.d.cts"]).as_deref(),
            Some("lib/index.d.cts")
        );
    }

    #[test]
    fn a_types_field_naming_a_missing_file_falls_back_to_main() {
        let json = r#"{ "main": "./index.js", "types": "./missing.d.ts" }"#;
        assert_eq!(
            declaration(json, ".", &["index.d.ts"]).as_deref(),
            Some("index.d.ts")
        );
    }

    #[test]
    fn a_package_that_names_nothing_is_its_index() {
        assert_eq!(
            declaration(r#"{ "name": "bare" }"#, ".", &["index.d.ts"]).as_deref(),
            Some("index.d.ts")
        );
    }

    #[test]
    fn a_subpath_of_a_package_with_no_exports_map_is_a_path_under_it() {
        let json = r#"{ "types": "index.d.ts" }"#;
        assert_eq!(
            declaration(json, "./fp", &["fp/index.d.ts"]).as_deref(),
            Some("fp/index.d.ts")
        );
    }

    #[test]
    fn the_types_condition_is_matched_where_the_map_writes_it() {
        let json = r#"{
          "exports": {
            ".": {
              "require": { "types": "./dist/index.d.cts", "default": "./dist/index.cjs" },
              "import": { "types": "./dist/index.d.mts", "default": "./dist/index.mjs" }
            }
          }
        }"#;
        assert_eq!(
            declaration(json, ".", &["dist/index.d.cts", "dist/index.d.mts"]).as_deref(),
            Some("dist/index.d.mts"),
            "`require` is not a condition uf check answers, so its `types` is never reached"
        );
    }

    #[test]
    fn an_import_target_means_the_declaration_beside_it() {
        let json = r#"{ "exports": { ".": { "import": "./esm/index.js" } } }"#;
        assert_eq!(
            declaration(json, ".", &["esm/index.d.ts"]).as_deref(),
            Some("esm/index.d.ts")
        );
    }

    #[test]
    fn a_condition_whose_file_is_missing_falls_through_to_the_next() {
        let json = r#"{ "exports": { "types": "./missing.d.ts", "default": "./index.js" } }"#;
        assert_eq!(
            declaration(json, ".", &["index.d.ts"]).as_deref(),
            Some("index.d.ts")
        );
    }

    #[test]
    fn a_null_target_blocks_the_subpath() {
        let json = r#"{ "exports": { "./internal": null, "./*": "./*.js" } }"#;
        assert_eq!(declaration(json, "./internal", &["internal.d.ts"]), None);
    }

    #[test]
    fn a_subpath_the_map_does_not_list_has_no_declaration() {
        let json = r#"{ "exports": { ".": "./index.js" } }"#;
        assert_eq!(declaration(json, "./hidden", &["hidden.d.ts"]), None);
    }

    #[test]
    fn a_pattern_substitutes_what_its_star_matched() {
        let json = r#"{ "exports": { "./locales/*": { "types": "./dist/locales/*.d.ts" } } }"#;
        assert_eq!(
            declaration(json, "./locales/en", &["dist/locales/en.d.ts"]).as_deref(),
            Some("dist/locales/en.d.ts")
        );
    }

    #[test]
    fn the_pattern_with_the_longest_prefix_wins() {
        let json = r#"{
          "exports": {
            "./*": "./generic/*.js",
            "./locales/*": "./locales/*.js"
          }
        }"#;
        assert_eq!(
            declaration(
                json,
                "./locales/en",
                &["generic/locales/en.d.ts", "locales/en.d.ts"]
            )
            .as_deref(),
            Some("locales/en.d.ts")
        );
    }

    #[test]
    fn a_string_or_a_condition_map_is_the_package_entry_and_nothing_else() {
        assert_eq!(
            declaration(r#"{ "exports": "./main.js" }"#, ".", &["main.d.ts"]).as_deref(),
            Some("main.d.ts")
        );
        let conditions = r#"{ "exports": { "types": "./main.d.ts" } }"#;
        assert_eq!(
            declaration(conditions, ".", &["main.d.ts"]).as_deref(),
            Some("main.d.ts")
        );
        assert_eq!(declaration(conditions, "./other", &["other.d.ts"]), None);
    }

    #[test]
    fn a_versioned_types_condition_is_matched_against_the_typescript_release() {
        let json = r#"{
          "exports": {
            ".": { "types@<5.0": "./ts4/index.d.ts", "types": "./index.d.ts" }
          }
        }"#;
        assert_eq!(
            declaration(json, ".", &["ts4/index.d.ts", "index.d.ts"]).as_deref(),
            Some("index.d.ts")
        );
    }

    #[test]
    fn types_versions_redirects_through_the_first_range_that_matches() {
        let json = r#"{
          "types": "index.d.ts",
          "typesVersions": {
            "<5.0": { "*": ["ts4/*"] },
            ">=5.0": { "*": ["ts5/*"] }
          }
        }"#;
        let files = [
            "index.d.ts",
            "ts4/index.d.ts",
            "ts5/index.d.ts",
            "ts5/fp.d.ts",
        ];
        assert_eq!(
            declaration(json, ".", &files).as_deref(),
            Some("ts5/index.d.ts")
        );
        assert_eq!(
            declaration(json, "./fp", &files).as_deref(),
            Some("ts5/fp.d.ts")
        );
    }

    #[test]
    fn types_versions_is_not_read_beside_an_exports_map() {
        let json = r#"{
          "exports": { ".": { "types": "./index.d.ts" } },
          "typesVersions": { "*": { "*": ["redirected/*"] } }
        }"#;
        assert_eq!(
            declaration(json, ".", &["index.d.ts", "redirected/index.d.ts"]).as_deref(),
            Some("index.d.ts")
        );
    }

    #[test]
    fn a_range_reads_the_grammar_typescript_accepts() {
        let release = TYPESCRIPT;
        assert!(range_matches("*", release));
        assert!(range_matches(">=4.2", release));
        assert!(range_matches("<4.0 || >=5.0", release));
        assert!(range_matches(">=5 <6", release));
        assert!(range_matches("5", release));
        assert!(!range_matches("<5.0", release));
        assert!(!range_matches("4", release));
        assert!(!range_matches("", release));
        assert!(
            !range_matches("~5.9", release),
            "unreadable comparisons match nothing"
        );
    }

    #[test]
    fn a_scoped_package_is_described_by_a_double_underscore_types_package() {
        assert_eq!(types_package("lodash"), "@types/lodash");
        assert_eq!(types_package("@babel/core"), "@types/babel__core");
    }

    #[test]
    fn a_manifest_that_is_not_an_object_does_not_parse() {
        assert!(Manifest::parse("[]").is_none());
        assert!(Manifest::parse("{ not json").is_none());
        let manifest =
            Manifest::parse(r#"{ "name": "zod", "version": "4.6.5" }"#).expect("an object parses");
        assert_eq!(manifest.name(), Some("zod"));
        assert_eq!(manifest.version(), Some("4.6.5"));
    }
}
