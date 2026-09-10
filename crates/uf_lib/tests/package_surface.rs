//! Invariants of the JavaScript surface shipped from `packages`.
//!
//! Shipped JavaScript weight is a product requirement, so these are structural
//! tests over the files themselves rather than tests of Rust code:
//!
//! - no `.js.flow` (or any other `.flow`) declaration file exists,
//! - no module re-exports with `export *`,
//! - no module runs anything when it is imported,
//! - every module opens with the `// @flow` pragma,
//! - every `exports` subpath resolves and every shipped module is reachable,
//! - no test file is published, by the allowlist or through `exports`,
//! - every shipped `package.json` declares `"sideEffects": false`,
//! - the Rust registry in `uf_lib` and the shipped subpaths agree,
//! - every `@uniflowed/*` a package imports is declared in its manifest.
//!
//! Every one of those is about a *shipped* module, and since the JavaScript
//! suite moved beside the code it tests that is narrower than "a `.js` under
//! `packages/`": a `.test.js` sits in the tree and is never published. Which
//! files those are is not a second list kept here — it is the `"!*.test.js"`
//! the manifests end with, read by [`is_test_file`]. A file npm would publish
//! is held to everything below; a file it would not is not a shipped module.
//!
//! The names a re-export carries are checked next door, in `uf_lib`'s unit
//! tests: `a_barrel_re_export_names_something_its_source_has` asks whether the
//! module a `from` names really exports the name beside it, and it needs the
//! Flow parser to tell a type from a value, which the scanners here do not use.
//!
//! One package is exempt from the Flow rules: `@uniflowed/vite` is executed
//! by the JavaScript host *before* any transform exists — it is how the
//! transform is reached — so it is plain JavaScript by necessity, and its
//! entry points (`register.js`, `bun-preload.js`, `driver.js`) run at import
//! time by design. Everything else about it is held to the same bar.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;
use uf_lib::{StdStatus, builtin_modules, std_module_descriptors};
use walkdir::WalkDir;

/// Directory holding an implementation detail that is deliberately kept out of
/// `package.json#exports`, so `@uniflowed/core/internal/*` is unresolvable.
const INTERNAL_DIR: &str = "internal";

/// The `files` entry every shipped manifest ends with, which subtracts a test
/// file from whatever the entries before it added.
///
/// One string, read two ways, and that is deliberate. It is what
/// [`a_shipped_package_never_publishes_a_test_file`] requires of a manifest,
/// and it is what [`is_test_file`] derives "this file is not shipped" from —
/// so "a test file is not a shipped module" is not a second opinion that could
/// drift from the first. A package that stopped excluding test files would
/// fail that test rather than quietly widening every invariant here.
///
/// No `/`, so npm reads it the way `.gitignore` does — matching at any depth,
/// which is what closes the bare-`internal` hole as well as the top-level
/// `*.js` one.
const TEST_FILE_NEGATION: &str = "!*.test.js";

/// Whether `path` is a test file: one the allowlist subtracts rather than
/// publishes.
///
/// The suffix comes from [`TEST_FILE_NEGATION`] rather than being written out
/// again, because the two questions have to have one answer. A file that npm
/// would publish is held to every invariant below; a file it would not is not
/// a shipped module and is held to none of them, and it is the *manifest* that
/// decides which a file is.
fn is_test_file(path: &Utf8Path) -> bool {
    let suffix = TEST_FILE_NEGATION
        .strip_prefix("!*")
        .expect("the negation is `!` and `*` before the suffix it subtracts");
    path.file_name().is_some_and(|name| name.ends_with(suffix))
}

/// The internal modules that are nonetheless exported, and why.
///
/// Two, and both for the same structural reason: a sibling package is a
/// different npm package and cannot reach another's internals by a relative
/// path. `core`'s is the shared native-runtime bridge every `@uniflowed/*`
/// raises through; `host`'s is the Node loader hook, which `@uniflowed/vite`
/// hands to `node:module`'s `register()` by specifier.
const EXPORTED_INTERNALS: &[&str] = &[
    "core/internal/native-runtime.js",
    "host/internal/node-hooks.js",
];

/// Packages the host runs directly, before any Flow transform exists. See the
/// module docs for why they are plain JavaScript.
///
/// `@uniflowed/host` is the loader itself — the hooks that make Flow run on
/// Node or Bun — so it cannot be written in the language it exists to load.
/// `@uniflowed/vite` is executed by Vite before any transform is reachable.
const PLAIN_JAVASCRIPT_PACKAGES: &[&str] = &["host", "vite"];

/// Individual modules that are entry points, and so run when they are loaded
/// because that is what running them means. Everything else in their package is
/// held to the ordinary bar — including the Flow pragma: an entry point is
/// still Flow, it just does something when it loads.
///
/// Three of them, and each is the beginning of a run rather than a module
/// something imports for its exports:
///
/// * `test/worker.js` — the process `uf test` starts to run a file on Node.
/// * `test/browser-worker.js` — the same for `uf test --browser`, which holds
///   the browser's handle and serves the page its modules.
/// * `test/internal/browser/page.js` — the module the *page* loads. It takes
///   over `console` on load for the reason the Node worker does, and it is an
///   entry in the only sense a page has: nothing imports it for a name.
///
/// The list is named files rather than a directory or a suffix on purpose. An
/// exemption that matched a pattern would quietly cover the next module written
/// beside these, and the whole value of this rule is that a shipped module
/// doing work on import is a decision somebody made once, in writing.
const ENTRY_POINT_MODULES: &[&str] = &[
    "test/worker.js",
    "test/browser-worker.js",
    "test/internal/browser/page.js",
];

/// Whether `module` (relative to `packages/`) is plain JavaScript by necessity.
///
/// Kept apart from [`runs_at_import`] because the two exemptions answer
/// different questions. Reaching for one predicate for both is how
/// `packages/test/worker.js` — Flow, and a process entry point — ended up
/// exempt from the `// @flow` pragma it in fact carries.
fn is_plain_javascript(module: &Utf8Path) -> bool {
    module
        .iter()
        .next()
        .is_some_and(|package| PLAIN_JAVASCRIPT_PACKAGES.contains(&package))
}

/// Whether `module` is allowed to run something when it is imported.
fn runs_at_import(module: &Utf8Path) -> bool {
    ENTRY_POINT_MODULES.contains(&module.as_str()) || is_plain_javascript(module)
}

/// Packages that exist to hand back somebody else's library under uf's name.
///
/// These may `export *`, because the alternative is a hand-maintained copy of
/// an export surface uf does not control. Everything else lists its names: uf's
/// own domains collide, and a star between them cannot say which `graphql` or
/// which `Text` was meant.
const RE_EXPORT_PACKAGES: &[&str] = &["react", "relay"];

/// Keywords a top-level statement in a shipped module may begin with. Anything
/// else runs when the module is imported.
const DECLARATION_KEYWORDS: &[&str] = &[
    "async",
    "class",
    "component",
    "const",
    "declare",
    "enum",
    "export",
    "function",
    "hook",
    "import",
    "interface",
    "let",
    "opaque",
    "type",
    "var",
];

fn lib_root() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages")
}

fn crate_root() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Every file under `packages`, relative to that directory.
fn shipped_files() -> Vec<Utf8PathBuf> {
    let root = lib_root();
    let mut files = WalkDir::new(&root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let path =
                Utf8PathBuf::from_path_buf(entry.into_path()).expect("lib paths are valid UTF-8");
            path.strip_prefix(&root)
                .expect("walked under lib root")
                .to_path_buf()
        })
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Every module `packages` *ships*: the `.js` files under it that npm would
/// publish, which is every `.js` file except the test files beside them.
///
/// A test file sits under `packages/` and is not a shipped module, and every
/// invariant below is written about shipped modules. Holding a co-located test
/// to them would be wrong three times over: its top-level `describe(...)` is
/// exactly the import-time side effect [`shipped_modules_have_no_import_time_side_effects`]
/// forbids, [`every_shipped_module_is_reachable_through_exports`] would demand
/// an `exports` subpath for `alert.test.js` — the opposite of what a package
/// wants — and [`every_uniflowed_import_is_declared`] would make
/// `@uniflowed/test` a dependency of `@uniflowed/ui`.
///
/// Which files those are is [`is_test_file`]'s answer, and it is the manifest's
/// own: the same entry that keeps a test file out of the tarball is what keeps
/// it out of this list. Not a second list to maintain, and not a judgement this
/// file makes on its own — if a file is published it is held to the invariants,
/// and if it is held to none of them it is because npm would not publish it.
///
/// [`shipped_files`] is deliberately left whole: a test file is still a file in
/// the tree, so it is still a resolution target for
/// [`every_relative_import_resolves_to_a_shipped_file`] and still has to be a
/// `.js` or a `package.json` under [`shipped_package_contains_only_modules_and_manifests`].
fn shipped_modules() -> Vec<Utf8PathBuf> {
    shipped_files()
        .into_iter()
        .filter(|path| path.extension() == Some("js") && !is_test_file(path))
        .collect()
}

fn read(relative: &Utf8Path) -> String {
    let path = lib_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn assert_exports(relative: &str, names: &[&str]) {
    let source = code_only(&read(Utf8Path::new(relative)));
    let listed = exported_in_a_list(&source);
    for name in names {
        assert!(
            source.contains(&format!("export function {name}"))
                || source.contains(&format!("export hook {name}"))
                || source.contains(&format!("export const {name}"))
                || source.contains(&format!("export opaque type {name}"))
                || source.contains(&format!("export type {name}"))
                || listed.contains(*name),
            "{relative} must export {name}"
        );
    }
}

/// Every name an `export { … }` or `export type { … }` list carries.
///
/// A package whose surface is one file declares its exports where it defines
/// them; a package split by subject re-exports them from an `index.js` that
/// defines nothing. Both are exports, and the second is the shape this
/// repository is moving to — `@uniflowed/validator` and `@uniflowed/query`
/// are already there.
fn exported_in_a_list(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (index, _) in source.match_indices("export ") {
        let rest = source[index + "export ".len()..].trim_start();
        let rest = rest.strip_prefix("type ").map_or(rest, str::trim_start);
        let Some(open) = rest.strip_prefix('{') else {
            continue;
        };
        let Some(close) = open.find('}') else {
            continue;
        };
        for entry in open[..close].split(',') {
            // `a as b` exports `b`; a bare `a` exports itself.
            let name = entry
                .split_whitespace()
                .last()
                .unwrap_or_default()
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if !name.is_empty() {
                names.insert(name.to_owned());
            }
        }
    }
    names
}

/// Blank out comments and the bodies of string and template literals, keeping
/// their delimiters, so brace depth and token scanning see code only.
///
/// Regular-expression literals are not modelled: `/` only starts a comment when
/// it is followed by `/` or `*`, and the shipped surface contains no regex
/// literal. A nested template inside a `${...}` substitution is likewise out of
/// scope; the surface contains none.
fn code_only(source: &str) -> String {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Code,
        LineComment,
        BlockComment,
        Single,
        Double,
        Template,
    }

    let chars = source.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(source.len());
    let mut state = State::Code;
    // Brace depth inside a `${ ... }` substitution; 0 means the template is in
    // its literal part.
    let mut substitution = 0usize;
    let mut index = 0usize;

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        match state {
            State::Code => match (current, next) {
                ('/', Some('/')) => {
                    state = State::LineComment;
                    out.push(' ');
                    index += 2;
                }
                ('/', Some('*')) => {
                    state = State::BlockComment;
                    out.push(' ');
                    index += 2;
                }
                ('\'', _) => {
                    state = State::Single;
                    out.push('\'');
                    index += 1;
                }
                ('"', _) => {
                    state = State::Double;
                    out.push('"');
                    index += 1;
                }
                ('`', _) => {
                    state = State::Template;
                    substitution = 0;
                    out.push('`');
                    index += 1;
                }
                _ => {
                    out.push(current);
                    index += 1;
                }
            },
            State::LineComment => {
                if current == '\n' {
                    state = State::Code;
                    out.push('\n');
                }
                index += 1;
            }
            State::BlockComment => {
                if current == '*' && next == Some('/') {
                    state = State::Code;
                    out.push(' ');
                    index += 2;
                } else {
                    if current == '\n' {
                        out.push('\n');
                    }
                    index += 1;
                }
            }
            State::Single | State::Double => {
                let quote = if state == State::Single { '\'' } else { '"' };
                if current == '\\' {
                    index += 2;
                } else if current == quote {
                    state = State::Code;
                    out.push(quote);
                    index += 1;
                } else {
                    index += 1;
                }
            }
            State::Template => {
                if current == '\\' {
                    index += 2;
                } else if substitution == 0 {
                    if current == '`' {
                        state = State::Code;
                        out.push('`');
                        index += 1;
                    } else if current == '$' && next == Some('{') {
                        substitution = 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                } else if current == '{' {
                    substitution += 1;
                    index += 1;
                } else if current == '}' {
                    substitution -= 1;
                    index += 1;
                } else {
                    index += 1;
                }
            }
        }
    }

    out
}

/// First token of every top-level statement in `code`.
///
/// A statement is recognised where formatted source puts one: at the start of a
/// line whose bracket depth is zero and whose predecessor closed a statement,
/// or straight after a depth-zero `;` on the same line. Both are enough to
/// catch an import-time side effect, which is what these tests look for. A
/// deliberately mis-indented statement would slip through, and the formatter
/// never produces one.
///
/// `<` and `>` are not bracket-like here, so a multi-line type argument list is
/// recognised through the "predecessor closed a statement" rule instead.
fn top_level_statement_tokens(code: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut depth = 0usize;
    let mut previous_line_closed_a_statement = true;

    for line in code.lines() {
        let mut expecting = previous_line_closed_a_statement && depth == 0;
        let mut chars = line.chars().peekable();

        while let Some(current) = chars.next() {
            match current {
                '{' | '(' | '[' => {
                    depth += 1;
                    expecting = false;
                }
                '}' | ')' | ']' => {
                    depth = depth.saturating_sub(1);
                }
                ';' => {
                    expecting = depth == 0;
                }
                _ if current.is_whitespace() => {}
                _ => {
                    if expecting && depth == 0 && is_identifier_start(current) {
                        let mut token = String::from(current);
                        while let Some(&candidate) = chars.peek() {
                            if is_identifier_part(candidate) {
                                token.push(candidate);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        tokens.push(token);
                    }
                    expecting = false;
                }
            }
        }

        // A blank line (or one that held only comments) carries the previous
        // verdict forward; otherwise a statement is closed by `;` or by the `}`
        // or `{` that ends a body.
        if let Some(last) = line.trim_end().chars().next_back() {
            previous_line_closed_a_statement = matches!(last, ';' | '}' | '{');
        }
    }

    tokens
}

fn is_identifier_start(value: char) -> bool {
    value.is_alphabetic() || value == '_' || value == '$'
}

fn is_identifier_part(value: char) -> bool {
    value.is_alphanumeric() || value == '_' || value == '$'
}

/// Every module specifier `source` imports or re-exports from.
///
/// Deliberately not `code_only` + a substring search: that helper blanks string
/// *bodies* and keeps their delimiters, which is exactly the half a specifier
/// lives in. So this is its own scanner over the raw text, skipping comments and
/// recording a string literal whenever the last word before it was `from` or
/// `import` — which covers `import x from "s"`, `export { x } from "s"`,
/// `import type { T } from "s"`, the side-effect `import "s"`, and the dynamic
/// `import("s")`.
///
/// Template literals cannot be static specifiers, so they are skipped rather
/// than recorded; a dynamic `import(`./${name}.js`)` is not resolvable here and
/// the shipped surface contains none.
fn module_specifiers(source: &str) -> Vec<String> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        Code,
        LineComment,
        BlockComment,
    }

    let chars = source.chars().collect::<Vec<_>>();
    let mut specifiers = Vec::new();
    let mut state = State::Code;
    let mut last_word = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        let current = chars[index];
        let next = chars.get(index + 1).copied();
        match state {
            State::Code => match (current, next) {
                ('/', Some('/')) => {
                    state = State::LineComment;
                    index += 2;
                }
                ('/', Some('*')) => {
                    state = State::BlockComment;
                    index += 2;
                }
                ('"' | '\'', _) => {
                    let quote = current;
                    let mut literal = String::new();
                    index += 1;
                    while index < chars.len() && chars[index] != quote {
                        if chars[index] == '\\' {
                            index += 1;
                        }
                        if index < chars.len() {
                            literal.push(chars[index]);
                            index += 1;
                        }
                    }
                    index += 1;
                    if last_word == "from" || last_word == "import" {
                        specifiers.push(literal);
                    }
                    last_word.clear();
                }
                ('`', _) => {
                    // Skip the whole template, substitutions and all: brace
                    // depth is not needed because no shipped template contains
                    // a nested backtick.
                    index += 1;
                    while index < chars.len() && chars[index] != '`' {
                        if chars[index] == '\\' {
                            index += 1;
                        }
                        index += 1;
                    }
                    index += 1;
                    last_word.clear();
                }
                _ if is_identifier_start(current) => {
                    let start = index;
                    while index < chars.len() && is_identifier_part(chars[index]) {
                        index += 1;
                    }
                    last_word = chars[start..index].iter().collect();
                }
                _ => {
                    // `(` is transparent so `import("./m.js")` still reads as an
                    // import; every other non-space character ends the word, which
                    // is what keeps `const text = "./m.js"` from looking like one.
                    if !current.is_whitespace() && current != '(' {
                        last_word.clear();
                    }
                    index += 1;
                }
            },
            State::LineComment => {
                if current == '\n' {
                    state = State::Code;
                }
                index += 1;
            }
            State::BlockComment => {
                if current == '*' && next == Some('/') {
                    state = State::Code;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        }
    }

    specifiers
}

/// Resolve `specifier` against the directory holding `module`, both relative to
/// `packages/`, collapsing `.` and `..` without touching the filesystem.
///
/// Returns `None` when the specifier climbs above `packages/`, which no shipped
/// module may do.
fn resolve_relative(module: &Utf8Path, specifier: &str) -> Option<Utf8PathBuf> {
    let mut segments: Vec<&str> = module
        .parent()
        .unwrap_or(Utf8Path::new(""))
        .as_str()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();

    for segment in specifier.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other),
        }
    }

    Some(segments.join("/").into())
}

/// Every `package.json` under `packages`, relative to that directory.
fn shipped_manifests() -> Vec<Utf8PathBuf> {
    shipped_files()
        .into_iter()
        .filter(|path| path.file_name() == Some("package.json"))
        .collect()
}

fn manifest(relative: &Utf8Path) -> Value {
    let source = read(relative);
    serde_json::from_str(&source).unwrap_or_else(|error| panic!("parse {relative}: {error}"))
}

/// Flatten an `exports` map into `subpath -> target`, following conditional
/// objects down to their string leaves.
fn exports_targets(exports: &Value) -> BTreeMap<String, String> {
    fn walk(subpath: &str, node: &Value, out: &mut BTreeMap<String, String>) {
        match node {
            Value::String(target) => {
                out.insert(subpath.to_string(), target.clone());
            }
            Value::Object(conditions) => {
                for (key, value) in conditions {
                    if key.starts_with('.') {
                        walk(key, value, out);
                    } else {
                        walk(subpath, value, out);
                    }
                }
            }
            Value::Array(candidates) => {
                for candidate in candidates {
                    walk(subpath, candidate, out);
                }
            }
            _ => {}
        }
    }

    let mut out = BTreeMap::new();
    walk(".", exports, &mut out);
    out
}

#[test]
fn shipped_package_contains_no_flow_declaration_files() {
    let offenders = shipped_files()
        .into_iter()
        .filter(|path| {
            let name = path.file_name().unwrap_or_default();
            name.ends_with(".js.flow") || name.ends_with(".flow")
        })
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty(),
        "user-authored and shipped Flow code is `.js` with `// @flow`; found {offenders:?}"
    );
}

#[test]
fn shipped_package_contains_only_modules_and_manifests() {
    let offenders = shipped_files()
        .into_iter()
        .filter(|path| path.extension() != Some("js") && path.file_name() != Some("package.json"))
        .collect::<Vec<_>>();

    assert!(
        offenders.is_empty(),
        "unexpected shipped files: {offenders:?}"
    );
}

#[test]
fn shipped_modules_start_with_the_flow_pragma() {
    for module in shipped_modules() {
        if is_plain_javascript(&module) {
            continue;
        }
        let source = read(&module);
        assert!(
            source.starts_with("// @flow\n"),
            "{module} must open with the `// @flow` pragma"
        );
    }
}

/// A module the host runs before any transform exists must say so in its own
/// docblock, not only in this file's exemption list.
///
/// `@noflow` is Flow's declaration that a file is plain JavaScript, and it is
/// the one uf reads: `uf check` runs inference over every `.js` in a project,
/// because uf is Flow-first and a file with no pragma is still a file uf owns.
/// Without this, `@uniflowed/vite` was exempt from the pragma rule here and
/// nowhere else — so `uf check` type-checked it anyway and reported 258 errors
/// against source that is plain JavaScript on purpose.
#[test]
fn plain_javascript_modules_declare_themselves_plain_javascript() {
    for module in shipped_modules() {
        if !is_plain_javascript(&module) {
            continue;
        }
        let source = read(&module);
        assert!(
            source.starts_with("// @noflow\n"),
            "{module} is exempt from the `// @flow` pragma, so it must open with \
             `// @noflow` — the exemption has to be in the file the checker reads, \
             not only in this test"
        );
    }
}

#[test]
fn shipped_modules_never_use_star_re_exports() {
    for module in shipped_modules() {
        let code = code_only(&read(&module));
        let mut rest = code.as_str();
        while let Some(offset) = rest.find("export") {
            rest = &rest[offset + "export".len()..];
            let next = rest.trim_start();
            if !next.starts_with('*') {
                continue;
            }
            // A star is allowed where the package *is* the re-export.
            //
            // The ban is about uf's own barrels: several domains legitimately
            // export the same name — `graphql`, `Image`, `Markdown`, `plan`,
            // `Text`, `contract` — so a star between them would collide, and
            // hand-listing is the only way to say which one is meant.
            //
            // A package whose whole job is handing back somebody else's
            // library has the opposite problem. The list is a second copy of an
            // export surface uf does not control, kept in step by hand, and
            // falling behind shows up as an `undefined` an application finds at
            // runtime long after the name was added upstream.
            assert!(
                RE_EXPORT_PACKAGES.contains(&module.iter().next().unwrap_or_default()),
                "{module} re-exports with `export *`, which collides between \
                 uf's own packages; list the names instead"
            );
            let specifier = next
                .split_once("from")
                .and_then(|(_, rest)| rest.trim_start().split('"').nth(1))
                .unwrap_or_default();
            assert!(
                !specifier.starts_with("@uniflowed/") && !specifier.starts_with('.'),
                "{module} stars a uf module ({specifier}); the exemption is for \
                 re-exporting somebody else's library"
            );
        }
    }
}

#[test]
fn shipped_modules_have_no_import_time_side_effects() {
    for module in shipped_modules() {
        if runs_at_import(&module) {
            continue;
        }
        let code = code_only(&read(&module));
        for token in top_level_statement_tokens(&code) {
            assert!(
                DECLARATION_KEYWORDS.contains(&token.as_str()),
                "{module} runs `{token}` at import time; a shipped module may \
                 only declare, import and export at its top level"
            );
        }
    }
}

/// A placeholder module — one that reaches for `nativeRuntimeRequired` —
/// raises only through that helper, so the "native runtime required" message
/// has one shape. A module that implements its surface raises its own errors,
/// and those are its own business.
#[test]
fn placeholder_modules_raise_only_through_the_shared_helper() {
    for module in shipped_modules() {
        if module == Utf8Path::new("core/internal/native-runtime.js") {
            continue;
        }
        let code = code_only(&read(&module));
        if !code.contains("nativeRuntimeRequired(") {
            continue;
        }
        assert!(
            !code.contains("throw new"),
            "{module} raises its own error beside nativeRuntimeRequired; the \
             message format lives in core/internal/native-runtime.js and nowhere else"
        );
    }
}

#[test]
fn native_runtime_message_is_defined_in_exactly_one_place() {
    let definitions = shipped_modules()
        .into_iter()
        .filter(|module| read(module).contains("requires the uf native runtime"))
        .collect::<Vec<_>>();

    assert_eq!(
        definitions,
        vec![Utf8PathBuf::from("core/internal/native-runtime.js")],
        "the native-runtime message must be defined once"
    );
}

#[test]
fn native_runtime_message_names_the_module_and_the_export() {
    let source = read(Utf8Path::new("core/internal/native-runtime.js"));

    assert!(
        source.contains("`${moduleSpecifier}: ${binding}() requires the uf native runtime`"),
        "the message must name both the subpath and the binding, so a caller \
         reading it sees `@uniflowed/core/effect: effect() requires the uf \
         native runtime`"
    );
}

#[test]
fn validator_exports_valibot_style_strict_flow_combinators() {
    assert_exports(
        "validator/index.js",
        &[
            "Infer",
            "brand",
            "date",
            "email",
            "enum_",
            "instance",
            "nullable",
            "parse",
            "partial",
            "strictObject",
            "transform",
            "tuple",
            "union",
        ],
    );
}

#[test]
fn state_exports_jotai_style_atoms_without_a_native_binding() {
    assert_exports(
        "state/index.js",
        &[
            "Atom",
            "ReadonlyAtom",
            "atom",
            "atomWithStorage",
            "selector",
            "useAtom",
        ],
    );
}

#[test]
fn every_shipped_module_imports_the_helper_it_raises_with() {
    for module in shipped_modules() {
        let code = code_only(&read(&module));
        if !code.contains("nativeRuntimeRequired(") {
            continue;
        }
        assert!(
            code.contains("nativeRuntimeRequired } from")
                || module == Utf8Path::new("core/internal/native-runtime.js"),
            "{module} calls nativeRuntimeRequired without importing it"
        );
    }
}

#[test]
fn every_shipped_package_declares_no_side_effects() {
    let manifests = shipped_manifests();
    assert!(
        !manifests.is_empty(),
        "expected at least one shipped package"
    );

    for relative in manifests {
        let manifest = manifest(&relative);
        assert_eq!(
            manifest.get("sideEffects"),
            Some(&Value::Bool(false)),
            "{relative} must declare \"sideEffects\": false so bundlers may \
             drop unreferenced modules"
        );
    }
}

#[test]
fn shipped_packages_never_publish_flow_declaration_files() {
    for relative in shipped_manifests() {
        let manifest = manifest(&relative);
        let files = manifest
            .get("files")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{relative} must list published files"));

        for entry in files {
            let entry = entry.as_str().unwrap_or_default();
            assert!(
                !entry.contains(".flow"),
                "{relative} publishes {entry}; the product has no `.flow` files"
            );
        }
    }
}

/// A published package must not ship a test file.
///
/// This is a rule about the *allowlist*, not about the files that happen to be
/// on disk today, and it has to be: there are no test files under `packages/`
/// yet, so a test that only walked the tree would pass while every manifest
/// was wide open. What is checked is whether a test file placed beside the
/// module it tests — which is where this repository wants them, and where
/// `crates/*/src/tests.rs` already puts the Rust half — would reach npm.
///
/// Twenty-two of the forty-nine manifests would have published one. Ten
/// allowlist `*.js`, which matches `alert.test.js` as readily as `alert.js`;
/// the other twelve name `internal` as a bare directory, and a directory entry
/// takes everything under it. Neither is visible by reading the entry.
///
/// The negation must be the **last** entry, and that is the whole reason this
/// is enforced rather than written down. npm applies `files` in order, so
///
/// ```json
/// "files": ["!*.test.js", "*.js", "internal"]
/// ```
///
/// publishes every test file — the negation subtracts from nothing, because
/// nothing has been added yet — while the same three entries in the other
/// order do not. Both read as if they exclude tests. `npm pack --dry-run` is
/// the only way to tell them apart, and nobody runs it on a manifest they did
/// not think they had changed: sorting the array alphabetically is enough to
/// move the negation to the front and start publishing tests silently.
#[test]
fn a_shipped_package_never_publishes_a_test_file() {
    const NEGATION: &str = TEST_FILE_NEGATION;

    for relative in shipped_manifests() {
        let manifest = manifest(&relative);
        let files = manifest
            .get("files")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{relative} must list published files"));

        let entries = files
            .iter()
            .map(|entry| entry.as_str().unwrap_or_default())
            .collect::<Vec<_>>();

        assert_eq!(
            entries.last(),
            Some(&NEGATION),
            "{relative} must end its `files` with {NEGATION:?} so a test file \
             beside the module it tests is not published; found {entries:?}"
        );
    }
}

/// What [`shipped_modules`] leaves out, and that it leaves something out.
///
/// Every invariant in this file now says "except a test file", and an exemption
/// is only as good as the predicate behind it. Two halves, and the second is
/// the one that would rot: that `packages/` really does hold co-located tests,
/// so the exclusion is doing work rather than describing a case that never
/// arises. Without it, `is_test_file` could stop matching anything — a rename
/// to `alert.spec.js`, a manifest that dropped the negation — and every
/// assertion here would go on passing over a list that had quietly grown.
#[test]
fn a_test_file_is_exactly_what_the_allowlist_subtracts() {
    assert!(is_test_file(Utf8Path::new("ui/alert.test.js")));
    assert!(is_test_file(Utf8Path::new("cell/internal/store.test.js")));
    assert!(!is_test_file(Utf8Path::new("ui/alert.js")));
    // The suffix is `.test.js` and not `test.js`: a module actually called
    // `test.js` is a shipped module, and `@uniflowed/test/index.js` is one.
    assert!(!is_test_file(Utf8Path::new("ui/test.js")));

    let colocated = shipped_files()
        .into_iter()
        .filter(|path| is_test_file(path))
        .count();
    assert!(
        colocated > 0,
        "no test file sits under `packages/`, so every exemption {TEST_FILE_NEGATION:?} \
         grants is exempting nothing — either the suite moved back out or the negation \
         stopped naming what a test file is called"
    );

    assert!(
        shipped_modules().iter().all(|module| !is_test_file(module)),
        "a test file reached the shipped modules, which are what the invariants below are about"
    );
}

/// And that no test file is reachable through an `exports` subpath either.
///
/// The allowlist is what npm packs, but `exports` is what a consumer can
/// `import`. A package that named a test file as a subpath would be asking for
/// it back even with the allowlist closed, so the two halves are checked
/// separately — this one is cheap and it is the half a reviewer would assume
/// was covered by the other.
#[test]
fn no_exports_subpath_names_a_test_file() {
    for relative in shipped_manifests() {
        let manifest = manifest(&relative);
        let exports = manifest
            .get("exports")
            .unwrap_or_else(|| panic!("{relative} must declare exports"));

        for (subpath, target) in exports_targets(exports) {
            assert!(
                !target.ends_with(".test.js"),
                "{relative} exports {subpath} as {target}, which is a test file"
            );
        }
    }
}

#[test]
fn every_exports_subpath_resolves_to_a_shipped_file() {
    for relative in shipped_manifests() {
        let package_dir = relative.parent().unwrap_or(Utf8Path::new("")).to_path_buf();
        let manifest = manifest(&relative);
        let exports = manifest
            .get("exports")
            .unwrap_or_else(|| panic!("{relative} must declare exports"));

        for (subpath, target) in exports_targets(exports) {
            let target = target
                .strip_prefix("./")
                .unwrap_or_else(|| panic!("{relative} {subpath} must use a relative target"));
            let resolved = lib_root().join(&package_dir).join(target);
            assert!(
                resolved.exists(),
                "{relative} maps {subpath} to {target}, which does not exist"
            );
        }
    }
}

#[test]
fn every_shipped_module_is_reachable_through_exports() {
    for relative in shipped_manifests() {
        let package_dir = relative.parent().unwrap_or(Utf8Path::new("")).to_path_buf();
        let manifest = manifest(&relative);
        let exports = manifest
            .get("exports")
            .unwrap_or_else(|| panic!("{relative} must declare exports"));
        let targets = exports_targets(exports)
            .into_values()
            .map(|target| target.trim_start_matches("./").to_string())
            .collect::<BTreeSet<_>>();

        for module in shipped_modules() {
            let Ok(inside) = module.strip_prefix(&package_dir) else {
                continue;
            };
            if inside.iter().next() == Some(INTERNAL_DIR) {
                // Surface packages are separate npm packages and cannot reach a
                // sibling's internals through a relative path, so the shared
                // native-runtime bridge is exported — under `./native`, which
                // names it as the internal it is. Nothing else may be.
                let exported = EXPORTED_INTERNALS.contains(&module.as_str());
                assert!(
                    exported || !targets.contains(inside.as_str()),
                    "{relative} exports {inside}, which is an internal module"
                );
                continue;
            }
            assert!(
                targets.contains(inside.as_str()),
                "{relative} ships {inside} without an exports subpath, so it is \
                 unreachable from outside the package"
            );
        }
    }
}

/// Every relative import in a shipped module must resolve to a shipped file.
///
/// `exports` subpaths are checked by the test above; this is the other half of
/// the same guarantee. A module that re-exports from a sibling nobody wrote is
/// not a type error a consumer ever sees — Flow does not run over an installed
/// `node_modules` — it is an `ERR_MODULE_NOT_FOUND` thrown on the first import.
/// `@uniflowed/core`'s root entry point shipped in exactly that state: it
/// re-exported `./testing.js` and `./config.js`, neither of which existed, so
/// `import { describe } from "@uniflowed/core"` failed to resolve at all.
#[test]
fn every_relative_import_resolves_to_a_shipped_file() {
    let shipped = shipped_files().into_iter().collect::<BTreeSet<_>>();
    let mut dangling = BTreeSet::new();

    for module in shipped_modules() {
        for specifier in module_specifiers(&read(&module)) {
            if !specifier.starts_with('.') {
                continue;
            }
            match resolve_relative(&module, &specifier) {
                Some(target) if shipped.contains(&target) => {}
                Some(target) => {
                    dangling.insert(format!(
                        "{module} imports {specifier} ({target} does not exist)"
                    ));
                }
                None => {
                    dangling.insert(format!(
                        "{module} imports {specifier}, which climbs out of packages/"
                    ));
                }
            }
        }
    }

    assert!(
        dangling.is_empty(),
        "{} relative imports do not resolve:\n{}",
        dangling.len(),
        dangling.into_iter().collect::<Vec<_>>().join("\n")
    );
}

/// A relative import may not leave the package that contains it.
///
/// Each directory under `packages/` is published as its own npm package, so a
/// relative path that walks into a sibling names a file that exists in this
/// repository and nowhere in an installed tree. Siblings are reached by
/// specifier — that is what the specifier is for — and the workspace symlinks
/// in `node_modules` would otherwise hide the breakage until publish.
#[test]
fn relative_imports_stay_inside_their_package() {
    let mut escaping = BTreeSet::new();

    for module in shipped_modules() {
        let package = module
            .iter()
            .next()
            .expect("a shipped module lives inside a package");
        for specifier in module_specifiers(&read(&module)) {
            if !specifier.starts_with('.') {
                continue;
            }
            let Some(target) = resolve_relative(&module, &specifier) else {
                escaping.insert(format!(
                    "{module} imports {specifier}, which climbs out of packages/"
                ));
                continue;
            };
            if target.iter().next() != Some(package) {
                escaping.insert(format!(
                    "{module} imports {specifier}, which resolves to {target} in another package"
                ));
            }
        }
    }

    assert!(
        escaping.is_empty(),
        "{} relative imports leave their package:\n{}",
        escaping.len(),
        escaping.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn module_specifiers_reads_every_import_form() {
    let source = r#"// @flow
// from "./comment-not-an-import.js"
import "./side-effect.js";
import def from "./default.js";
import type { T } from "./type-only.js";
export { name } from "./re-export.js";
export type { U } from "./type-re-export.js";
const lazy = import("./dynamic.js");
const text = "./not-an-import.js";
"#;

    assert_eq!(
        module_specifiers(source),
        vec![
            "./side-effect.js",
            "./default.js",
            "./type-only.js",
            "./re-export.js",
            "./type-re-export.js",
            "./dynamic.js",
        ]
    );
}

#[test]
fn resolve_relative_collapses_dot_segments_and_refuses_to_escape() {
    let module = Utf8Path::new("core/internal/native-runtime.js");

    assert_eq!(
        resolve_relative(module, "./sibling.js").as_deref(),
        Some(Utf8Path::new("core/internal/sibling.js"))
    );
    assert_eq!(
        resolve_relative(module, "../index.js").as_deref(),
        Some(Utf8Path::new("core/index.js"))
    );
    assert_eq!(
        resolve_relative(module, "../../react/index.js").as_deref(),
        Some(Utf8Path::new("react/index.js"))
    );
    assert_eq!(resolve_relative(module, "../../../escaped.js"), None);
}

/// A module a browser can reach must not import a Node builtin.
///
/// `@uniflowed/server` is server-only and imports `node:async_hooks` to keep
/// one request's context apart from another's. `@uniflowed/router`'s client
/// entry took two string constants from its server entry, which re-exported the
/// request dispatcher, which imports that package — so a browser bundle ended
/// up importing `node:async_hooks`. Nothing called it, so a bundler dropped the
/// code, but the import survived and Vite warned on every build.
///
/// Checked one hop at a time rather than transitively: every module is checked,
/// so a chain is caught at whichever link first crosses the line, and that is
/// the link worth naming.
#[test]
fn a_client_entry_never_imports_a_node_builtin() {
    /// Packages that only ever run on a server, and may.
    const SERVER_ONLY: &[&str] = &["server", "host", "vite", "test", "pm", "rm", "prepare"];
    /// Modules that are a server entry inside a package that is not.
    ///
    /// `react-testing/internal/render.js` is the odd one: it installs a DOM on
    /// a host that has none, which is a thing only a test runner does and never
    /// a browser, where the DOM is already there. `story/collect.js` walks a
    /// project directory to find story files, which a browser has no way to do
    /// and no reason to want — the rendering half of that package imports
    /// nothing from here.
    const SERVER_MODULES: &[&str] = &[
        "router/server.js",
        "router/handler.js",
        "react-testing/internal/render.js",
        "story/collect.js",
    ];

    let mut leaks = Vec::new();
    for module in shipped_modules() {
        let package = module.iter().next().unwrap_or_default();
        if SERVER_ONLY.contains(&package) || SERVER_MODULES.contains(&module.as_str()) {
            continue;
        }
        for specifier in module_specifiers(&read(&module)) {
            if specifier.starts_with("node:") {
                leaks.push(format!("{module} imports {specifier}"));
            }
        }
    }

    assert!(
        leaks.is_empty(),
        "{} browser-reachable modules import a Node builtin:\n{}",
        leaks.len(),
        leaks.join("\n")
    );
}

/// The client half of the router must not import its server half.
///
/// The two constants they share live in `internal/document.js` precisely so
/// this import does not have to exist.
#[test]
fn the_routers_client_entry_does_not_import_its_server_entry() {
    let client = read(Utf8Path::new("router/client.js"));

    for specifier in module_specifiers(&client) {
        assert!(
            !specifier.contains("server") && !specifier.contains("handler"),
            "router/client.js imports {specifier}, which drags the request \
             dispatcher into a browser bundle"
        );
    }
}

#[test]
fn covariant_opaque_types_are_defined_with_a_covariant_carrier() {
    let mut covariant = Vec::new();

    for module in shipped_modules() {
        let code = code_only(&read(&module));
        for statement in code.split(';') {
            let Some(offset) = statement.find("opaque type ") else {
                continue;
            };
            let declaration = &statement[offset..];
            let Some(open) = declaration.find('<') else {
                continue;
            };
            if !declaration[open..].starts_with("<out ") {
                continue;
            }
            // The carrier the opaque type is *defined* as, checked in the
            // module that declares it rather than against a list of names. A
            // list grows every time a package earns a carrier of its own, and
            // a name on it says nothing about whether the type behind it is
            // covariant. The definition rather than the bound, because a bound
            // is often a builtin — `Effect` is bounded by `$Iterable` and
            // defined as `EffectCarrier`.
            let carrier = carrier_of(declaration).unwrap_or_else(|| {
                panic!(
                    "{module} declares a covariant opaque type with no carrier, so `out` \
                     promises more than the definition delivers: {}",
                    declaration.trim()
                )
            });
            assert!(
                code.contains(&format!("type {carrier}<out ")),
                "{module} defines a covariant opaque type as {carrier}, which is not declared \
                 covariant in the same module: {}",
                declaration.trim()
            );
            covariant.push(module.clone());
        }
    }

    assert!(
        !covariant.is_empty(),
        "expected at least one covariant opaque type in the shipped surface"
    );
}

/// The name of the type an `opaque type X<…> = Carrier<…>` is defined as.
///
/// The definition follows the `=` that comes after the type parameters and the
/// bound, so the angle brackets have to be counted rather than searched for:
/// `<out A, out E = empty>` holds an `=` that is not the one, and a bound may
/// hold more.
fn carrier_of(declaration: &str) -> Option<String> {
    let bytes = declaration.as_bytes();
    let mut at = declaration.find('<')?;
    let mut depth = 0usize;
    let mut definition = None;
    while at < bytes.len() {
        match bytes[at] {
            b'<' => depth += 1,
            b'>' => depth = depth.saturating_sub(1),
            // `=>` inside a function type in the bound is not the definition.
            b'=' if depth == 0 && bytes.get(at + 1) != Some(&b'>') => {
                definition = Some(at + 1);
                break;
            }
            _ => {}
        }
        at += 1;
    }
    let name: String = declaration[definition?..]
        .trim_start()
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// The shipped surface is written in the Flow of today, not the Flow of 2019.
///
/// Three spellings the checker itself now reports as deprecated: `+prop` for a
/// read-only property, `<+T>` for a covariant type parameter, and `<T: Bound>`
/// for a bound. Modern Flow spells them `readonly prop`, `<out T>` and
/// `<T extends Bound>`, and `uf check` reported 1015 errors against this
/// repository's own packages for using the old ones.
///
/// A test rather than a one-time cleanup, because the old spellings still parse
/// and a contributor who learned Flow five years ago will reach for them.
#[test]
fn the_shipped_surface_uses_modern_flow_spellings() {
    let mut legacy = BTreeSet::new();

    for module in shipped_modules() {
        let code = code_only(&read(&module));
        for (number, line) in code.lines().enumerate() {
            let number = number + 1;
            let trimmed = line.trim_start();
            if trimmed.starts_with('+') && trimmed.contains(':') {
                legacy.insert(format!("{module}:{number}: `+prop` is now `readonly prop`"));
            }
            if line.contains("<+") {
                legacy.insert(format!("{module}:{number}: `<+T>` is now `<out T>`"));
            }
            if line.contains("<-") {
                legacy.insert(format!("{module}:{number}: `<-T>` is now `<in T>`"));
            }
        }
    }

    assert!(
        legacy.is_empty(),
        "{} deprecated Flow spellings in the shipped surface:\n{}",
        legacy.len(),
        legacy.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn the_variance_fixture_is_a_flow_module_outside_the_shipped_package() {
    let fixture = crate_root().join("tests/flow/effect-variance.js");
    let source = fs::read_to_string(&fixture).unwrap_or_else(|error| panic!("{fixture}: {error}"));

    assert!(source.starts_with("// @flow\n"));
    assert!(
        source.contains("const widenedEffect: Effect<Animal> = dogEffect;"),
        "the fixture must record that Effect<+T> widens"
    );
    assert!(
        source.contains("const widenedCell: Cell<Animal> = dogCell;"),
        "the fixture must record that Cell<T> does not widen"
    );
    assert!(
        !shipped_files()
            .iter()
            .any(|path| path.file_name() == Some("effect-variance.js")),
        "the fixture must stay out of the shipped package"
    );
}

#[test]
fn code_only_blanks_comments_strings_and_templates() {
    let source = "// @flow\nconst a = \"export * from 'x'\";\n/* export * */\nconst b = `${a}}`;\n";
    let code = code_only(source);

    assert!(!code.contains("export *"));
    assert!(code.contains("const a = \"\""));
    assert!(code.contains("const b = ``"));
}

#[test]
fn top_level_statement_tokens_find_import_time_side_effects() {
    let code = code_only(
        "// @flow\nimport { a } from './a.js';\nthrow new Error('boom');\nexport const b = 1;\n",
    );
    let tokens = top_level_statement_tokens(&code);

    assert_eq!(tokens, vec!["import", "throw", "export"]);
}

#[test]
fn top_level_statement_tokens_ignore_nested_and_continued_statements() {
    let code = code_only(
        "// @flow\nexport type Fn = component<T: {...}>(\n  value: T,\n) renders mixed;\n\
         function raise(): empty {\n  throw new Error('x');\n}\nexport const value: number = 1;\n",
    );
    let tokens = top_level_statement_tokens(&code);

    assert_eq!(tokens, vec!["export", "function", "export"]);
}

#[test]
fn top_level_statement_tokens_catch_a_second_statement_on_one_line() {
    let code = code_only("// @flow\nconst a = 1; sideEffect();\n");
    let tokens = top_level_statement_tokens(&code);

    assert_eq!(tokens, vec!["const", "sideEffect"]);
}

/// Every module the registry advertises must resolve to a package on disk.
///
/// `uf inspect` lists these, the scaffold imports them, and `docs` documents
/// them. A specifier with nothing behind it is not a missing nicety: a project
/// `uf create` generates imports `@uniflowed/react`, and if no package declares
/// that name the import resolves to nothing — which is why `uf check` reported
/// `Cannot use Node as a type because it is an any-typed value` on the layout
/// the scaffold itself wrote.
///
/// Resolution here is Node's: a bare `@scope/name` needs a manifest declaring
/// that name, and `@scope/name/sub` needs `sub` in that manifest's `exports`.
///
/// # Why the std table is walked here too
///
/// Because for a long time it was not, and that was the hole. This test read
/// `builtin_modules()` only, which names `@uniflowed/std` and no subpath of it,
/// so `uf_std::std_modules()` could advertise `@uniflowed/std/vfs`,
/// `@uniflowed/std/http` and forty-two more with nothing whatever behind them
/// and every check in the repository stayed green — see ubugeeei-prod/uf#710.
/// Only the entries that claim a file are walked: a [`uf_std::StdStatus`] of
/// `Planned` or `Declined` is a name uf does *not* advertise as importable, and
/// requiring it to resolve would be requiring the roadmap to be written.
#[test]
fn every_advertised_module_resolves_to_a_package() {
    let manifests: BTreeMap<String, Value> = shipped_manifests()
        .iter()
        .map(|relative| {
            let parsed = manifest(relative);
            let name = parsed["name"]
                .as_str()
                .unwrap_or_else(|| panic!("{relative} has no name"))
                .to_owned();
            (name, parsed)
        })
        .collect();

    let advertised = builtin_modules()
        .into_iter()
        .map(|module| module.specifier.to_string())
        .chain(
            std_module_descriptors()
                .into_iter()
                .filter(|module| matches!(module.status, StdStatus::Ships | StdStatus::Declared))
                .map(|module| module.specifier.to_string()),
        )
        .collect::<BTreeSet<_>>();

    let mut unresolvable = Vec::new();
    for specifier in &advertised {
        let specifier = specifier.as_str();
        let (package, subpath) = match specifier.strip_prefix('@').and_then(|rest| {
            let (scope, rest) = rest.split_once('/')?;
            Some(match rest.split_once('/') {
                Some((name, sub)) => (format!("@{scope}/{name}"), Some(sub.to_owned())),
                None => (format!("@{scope}/{rest}"), None),
            })
        }) {
            Some(split) => split,
            None => (specifier.to_owned(), None),
        };

        let Some(found) = manifests.get(&package) else {
            unresolvable.push(format!("{specifier}: no package named {package}"));
            continue;
        };
        if let Some(subpath) = subpath {
            let key = format!("./{subpath}");
            if !exports_targets(&found["exports"]).contains_key(&key) {
                unresolvable.push(format!("{specifier}: {package} does not export {key}"));
            }
        }
    }

    // The std subpaths are six of these, and naming the number is how a
    // regression that quietly stops walking them shows up as a failure rather
    // than as a shorter green run.
    assert!(
        advertised.len() > 40,
        "the walk found almost nothing, so it is not checking anything: {}",
        advertised.len()
    );
    assert!(
        advertised.contains("@uniflowed/std/hex"),
        "the std table is not being walked"
    );
    assert!(
        unresolvable.is_empty(),
        "{} advertised modules do not resolve:\n{}",
        unresolvable.len(),
        unresolvable.join("\n")
    );
}

/// A `@uniflowed/std` module that ships is one that runs, and the root is not.
///
/// `tools/ci/publishable.sh` draws the line for a whole package: a directory
/// whose modules call `nativeRuntimeRequired` is a declaration, and publishing
/// one squats a name that cannot run. `@uniflowed/std` is the package that
/// broke the rule by being both — `index.js` is seventy-odd functions that
/// throw, and six subpaths beside it are code — so the line has to be drawn per
/// subpath, and this is where.
///
/// Both directions. A [`StdStatus::Ships`] module that calls the helper is a
/// module the table says runs and does not; a [`StdStatus::Declared`] one that
/// stops calling it has become an implementation nobody re-labelled, and it
/// would go on being reported as a declaration by `uf inspect` and skipped by
/// `publishable.sh`.
#[test]
fn a_shipping_std_module_is_the_one_that_runs() {
    for module in std_module_descriptors() {
        let subpath = module
            .specifier
            .strip_prefix("@uniflowed/std/")
            .map_or_else(|| "index".to_owned(), str::to_owned);
        let file = Utf8PathBuf::from(format!("std/{subpath}.js"));
        let raises = match module.status {
            StdStatus::Ships | StdStatus::Declared => {
                code_only(&read(&file)).contains("nativeRuntimeRequired(")
            }
            // Nothing to read: the file does not exist, which is what the
            // status says.
            StdStatus::Planned | StdStatus::Declined => continue,
        };

        match module.status {
            StdStatus::Ships => assert!(
                !raises,
                "{} is in the table as shipping and {file} raises nativeRuntimeRequired",
                module.specifier
            ),
            StdStatus::Declared => assert!(
                raises,
                "{} is in the table as a declaration surface and {file} no longer \
                 raises nativeRuntimeRequired — it is an implementation now, and \
                 the registry, tools/ci/publishable.sh and the release manifests \
                 all read that flag",
                module.specifier
            ),
            StdStatus::Planned | StdStatus::Declined => unreachable!(),
        }
    }
}

/// Nothing that claims WinterTC alignment reaches for a host.
///
/// The flag used to be set by the constructor for all forty-five entries, which
/// made it a claim about `@uniflowed/std/net` — `TcpListener`, `UdpSocket` — as
/// loudly as about the six modules somebody wrote. It now means "this file was
/// read and it imports no host", and this is the reading:
/// `docs/app/reference/std` says of the six that "nothing here imports `node:`
/// anything, touches `Buffer`, or reads `process`", and that claim is the
/// difference between a module that runs on Deno and the edge and one that has
/// only ever been run on Node.
///
/// `code_only` first, so that `bytes.js`'s note about Go's `bytes.Buffer` and
/// `hex.js`'s benchmark against Node's native `Buffer` stay what they are:
/// prose about the decision, which is exactly what should be written down.
#[test]
fn a_std_module_that_claims_wintertc_alignment_names_no_host() {
    /// Globals only a host has. `Buffer` and `process` are Node's; the rest of
    /// what these modules use — `Uint8Array`, `TextEncoder`, `AbortController`,
    /// `Promise`, `setTimeout` — is on all four runtimes.
    const HOST_GLOBALS: &[&str] = &["Buffer", "process", "require", "__dirname", "__filename"];

    let mut checked = 0usize;
    let mut leaks = Vec::new();
    for module in std_module_descriptors() {
        if !module.wintertc_aligned {
            continue;
        }
        let subpath = module
            .specifier
            .strip_prefix("@uniflowed/std/")
            .expect("only a subpath can claim to have been read");
        let file = Utf8PathBuf::from(format!("std/{subpath}.js"));
        let source = read(&file);
        checked += 1;

        for specifier in module_specifiers(&source) {
            if specifier.starts_with("node:") {
                leaks.push(format!("{} imports {specifier}", module.specifier));
            }
        }
        let code = code_only(&source);
        for global in HOST_GLOBALS {
            for (index, _) in code.match_indices(global) {
                let before = code[..index].chars().next_back();
                let after = code[index + global.len()..].chars().next();
                let bounded = !before.is_some_and(|c| is_identifier_part(c) || c == '.')
                    && !after.is_some_and(is_identifier_part);
                if bounded {
                    leaks.push(format!("{} reads {global}", module.specifier));
                }
            }
        }
    }

    // Not a number: which modules these are is pinned by
    // `the_std_registry_names_exactly_what_the_std_package_exports` against
    // `packages/std/package.json`, and writing the count down here as well is
    // the second place with the same fact in it that #710 is about.
    assert!(
        checked > 0,
        "no std module claims to have been read, so this is not checking anything"
    );
    assert!(
        leaks.is_empty(),
        "{} module(s) claim WinterTC alignment and name a host:\n{}",
        leaks.len(),
        leaks.join("\n")
    );
}

/// Every `@uniflowed/*` a package imports is declared in its manifest.
///
/// An undeclared dependency resolves inside this workspace, where every
/// sibling is a directory away, and not for anyone who installs the package
/// from npm. `@uniflowed/ui` imported `@uniflowed/validator` for a type and
/// declared nothing, so a consumer running `uf check` could not resolve
/// `Schema`. See ubugeeei-prod/uf#146.
///
/// Type-only imports count: a published package whose types do not resolve
/// is a package whose types do not resolve.
///
/// {@link module_specifiers} is what makes this checkable rather than a
/// search of the text, and the difference matters here. `packages/vite`
/// builds application code in a template literal:
///
/// ```text
/// return `import { hydrate } from "@uniflowed/router/client";
/// ```
///
/// `@uniflowed/vite` does not import `@uniflowed/router` — the app it
/// generates does, and that app declares it. `packages/react` names
/// `@uniflowed/react` in a comment. A search reports both and is wrong about
/// both.
#[test]
fn every_uniflowed_import_is_declared() {
    let mut undeclared: Vec<String> = Vec::new();

    for module in shipped_modules() {
        let Some(package) = module.iter().next() else {
            continue;
        };
        let manifest = manifest(&Utf8PathBuf::from(package).join("package.json"));
        let name = manifest["name"].as_str().unwrap_or_default();
        let declared = declared_uniflowed_dependencies(&manifest);

        for specifier in module_specifiers(&read(&module)) {
            if !specifier.starts_with("@uniflowed/") {
                continue;
            }
            // A package may name itself: `exports` makes that resolve, and
            // it is how one of its own subpaths is spelled from inside.
            let target = specifier.split('/').take(2).collect::<Vec<_>>().join("/");
            if target == name || declared.contains(&target) {
                continue;
            }
            undeclared.push(format!("{name} imports {specifier} in {module}"));
        }
    }

    undeclared.sort();
    undeclared.dedup();
    assert!(
        undeclared.is_empty(),
        "packages import @uniflowed/* they do not declare:\n  {}",
        undeclared.join("\n  ")
    );
}

/// The `@uniflowed/*` names a manifest declares, of any kind.
fn declared_uniflowed_dependencies(manifest: &Value) -> BTreeSet<String> {
    let mut declared = BTreeSet::new();
    for key in ["dependencies", "peerDependencies", "optionalDependencies"] {
        let Some(Value::Object(map)) = manifest.get(key) else {
            continue;
        };
        declared.extend(
            map.keys()
                .filter(|name| name.starts_with("@uniflowed/"))
                .cloned(),
        );
    }
    declared
}

/// A package uf loads into the host on another's behalf is a dependency of
/// the package that needs it.
///
/// [`every_uniflowed_import_is_declared`] reads `import` statements, and this
/// dependency is not written as one. `uf test` starts the Capability JS Host
/// with `--import @uniflowed/host/register` (`--preload` on Bun): the loader
/// reaches the process as a command-line argument that
/// `crates/uf_cli/src/commands/test.rs` builds, resolved out of the project's
/// own `node_modules`, and no module under `packages/test` mentions it.
///
/// So nothing tied the runner to the loader, and a project that installed
/// `@uniflowed/test` got a runner that could not run:
///
/// ```text
/// $ uf create app react demo && cd demo && uf install
/// $ uf test
/// error: `@uniflowed/host` is not installed for …/demo; add it to the
/// project's dependencies and run the package manager (`uf install`)
/// ```
///
/// The template is not the place to fix that. A project depends on the test
/// runner; which loader that runner needs is the runner's business, and
/// every project that ever declares `@uniflowed/test` would otherwise have to
/// know to name it too.
#[test]
fn a_loader_uf_injects_is_declared_by_the_package_that_needs_it() {
    // Package -> what `uf` loads into the host for it. One entry today, from
    // the single `uniflowed_package` call in `commands/test.rs` that is not
    // the package the user asked for. Add a line when a command grows another.
    const INJECTED: &[(&str, &str)] = &[("test", "@uniflowed/host")];

    for (package, loader) in INJECTED {
        let relative = Utf8PathBuf::from(package).join("package.json");
        let declared = declared_uniflowed_dependencies(&manifest(&relative));
        assert!(
            declared.contains(*loader),
            "uf loads {loader} into the host for @uniflowed/{package}, which does not \
             declare it — a project that installs the package does not get the loader"
        );
    }
}
