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
//! - every shipped `package.json` declares `"sideEffects": false`,
//! - the Rust registry in `uf_lib` and the shipped subpaths agree.
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
use uf_lib::builtin_modules;
use walkdir::WalkDir;

/// Directory holding an implementation detail that is deliberately kept out of
/// `package.json#exports`, so `@uniflowed/core/internal/*` is unresolvable.
const INTERNAL_DIR: &str = "internal";

/// Packages the host runs directly, before any Flow transform exists. See the
/// module docs for why they are plain JavaScript.
const PLAIN_JAVASCRIPT_PACKAGES: &[&str] = &["vite"];

/// Individual modules that are process entry points, and so run when they are
/// loaded because that is what running them means. Everything else in their
/// package is held to the ordinary bar — including the Flow pragma: an entry
/// point is still Flow, it just does something when it loads.
const ENTRY_POINT_MODULES: &[&str] = &["test/worker.js"];

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

fn shipped_modules() -> Vec<Utf8PathBuf> {
    shipped_files()
        .into_iter()
        .filter(|path| path.extension() == Some("js"))
        .collect()
}

fn read(relative: &Utf8Path) -> String {
    let path = lib_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn assert_exports(relative: &str, names: &[&str]) {
    let source = code_only(&read(Utf8Path::new(relative)));
    for name in names {
        assert!(
            source.contains(&format!("export function {name}"))
                || source.contains(&format!("export hook {name}"))
                || source.contains(&format!("export const {name}"))
                || source.contains(&format!("export opaque type {name}"))
                || source.contains(&format!("export type {name}")),
            "{relative} must export {name}"
        );
    }
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
            assert!(
                !next.starts_with('*'),
                "{module} re-exports with `export *`, which pins the whole \
                 module graph and defeats tree-shaking; list the names instead"
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
        "validator/internal/schema.js",
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
                let bridge = package_dir.as_str() == "core"
                    && inside.as_str() == "internal/native-runtime.js";
                assert!(
                    bridge || !targets.contains(inside.as_str()),
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
            if !declaration[open..].starts_with("<+") {
                continue;
            }
            assert!(
                declaration.contains("NativeHandleCovariant")
                    || declaration.contains("EffectCarrier")
                    || declaration.contains("FiberCarrier")
                    || declaration.contains("TagCarrier")
                    || declaration.contains("LayerCarrier"),
                "{module} declares a covariant opaque type without a covariant \
                 carrier, so the `+` sigil promises more than the definition \
                 delivers: {}",
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

    let mut unresolvable = Vec::new();
    for module in builtin_modules() {
        let specifier = module.specifier.as_str();
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

    assert!(
        unresolvable.is_empty(),
        "{} advertised modules do not resolve:\n{}",
        unresolvable.len(),
        unresolvable.join("\n")
    );
}
