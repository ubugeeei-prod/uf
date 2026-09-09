use super::*;

fn kinds(source: &str) -> Vec<TokenKind> {
    tokenize(source)
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

#[test]
fn tokenizes_identifiers_and_punctuation() {
    assert_eq!(
        kinds("const a = 1;"),
        vec![
            TokenKind::Ident,
            TokenKind::Ident,
            TokenKind::Punct(b'='),
            TokenKind::Number,
            TokenKind::Punct(b';'),
        ]
    );
}

#[test]
fn skips_a_byte_order_mark_before_the_first_token() {
    let tokens = tokenize("\u{feff}\"use client\";");
    assert_eq!(tokens[0].kind, TokenKind::String);
    assert_eq!(
        tokens[0].quoted_content("\u{feff}\"use client\";"),
        "use client"
    );
}

#[test]
fn skips_a_shebang_line() {
    let source = "#!/usr/bin/env uf\n\"use client\";";
    let tokens = tokenize(source);
    assert_eq!(tokens[0].kind, TokenKind::String);
    assert!(tokens[0].newline_before);
}

#[test]
fn lexes_arrow_as_one_token() {
    assert_eq!(
        kinds("() => 1"),
        vec![
            TokenKind::Punct(b'('),
            TokenKind::Punct(b')'),
            TokenKind::Arrow,
            TokenKind::Number,
        ]
    );
}

#[test]
fn does_not_see_directives_inside_comments() {
    let source = "/* \"use client\"; */ const a = 1;";
    assert!(!kinds(source).contains(&TokenKind::String));
}

#[test]
fn treats_unterminated_strings_as_invalid() {
    assert_eq!(kinds("\"use client\n"), vec![TokenKind::Invalid]);
}

#[test]
fn lexes_regex_literals_without_swallowing_quotes() {
    let source = "const re = /[\"']/; const a = 1;";
    let tokens = tokenize(source);
    assert!(tokens.iter().any(|token| token.kind == TokenKind::Regex));
    assert!(!tokens.iter().any(|token| token.kind == TokenKind::String));
}

#[test]
fn handles_crlf_line_endings() {
    let source = "\"use client\";\r\nconst a = 1;\r\n";
    let tokens = tokenize(source);
    assert_eq!(tokens[0].quoted_content(source), "use client");
    assert!(tokens[2].newline_before);
}

#[test]
fn handles_template_literals_with_substitutions() {
    let source = "const a = `x${ y }z`;";
    assert!(kinds(source).contains(&TokenKind::Template));
}

#[test]
fn returns_no_tokens_for_oversized_sources() {
    let source = "a".repeat(MAX_SOURCE_BYTES + 1);
    assert!(tokenize(&source).is_empty());
}

#[test]
fn scans_static_dynamic_and_require_imports() {
    let source = r#"
import Counter from "./client/Counter.js";
import "./side-effect.js";
export { a } from "./re-export.js";
const lazy = import("./lazy.js");
const legacy = require("./legacy.js");
"#;
    let imports = scan_imports(source);
    let specifiers: Vec<_> = imports
        .iter()
        .map(|import| (import.specifier.as_str(), import.kind))
        .collect();
    assert_eq!(
        specifiers,
        vec![
            ("./client/Counter.js", ImportKind::Static),
            ("./side-effect.js", ImportKind::Static),
            ("./re-export.js", ImportKind::ReExport),
            ("./lazy.js", ImportKind::Dynamic),
            ("./legacy.js", ImportKind::Require),
        ]
    );
}

/// A type-only import is erased before anything runs, so it is not an edge.
///
/// The exception is the one that only looks type-only: `import type from` is
/// a default import into a binding named `type`, and dropping it would lose a
/// real dependency.
#[test]
fn type_only_imports_are_not_module_edges() {
    let source = r#"
import type { Account } from "./types.js";
import typeof * as Actions from "./actions.js";
import typeof Save from "./save.js";
import type Config, { Extra } from "./config.js";
export type { Account } from "./types.js";
import type from "./default-named-type.js";
import { real } from "./real.js";
"#;
    let imports = scan_imports(source);
    let specifiers: Vec<_> = imports
        .iter()
        .map(|import| (import.specifier.as_str(), import.kind))
        .collect();
    assert_eq!(
        specifiers,
        vec![
            ("./default-named-type.js", ImportKind::Static),
            ("./real.js", ImportKind::Static),
        ]
    );
}

#[test]
fn ignores_import_like_text_inside_strings() {
    let imports = scan_imports("const doc = \"import x from './a.js'\";");
    assert!(imports.is_empty());
}

#[test]
fn scans_export_shapes() {
    let source = r#"
export async function refresh() {}
export function render() {}
export const value = 1;
export const handler = async () => {};
export const wrapped = serverAction(async () => {});
export class Widget {}
export default async function () {}
"#;
    let exports = scan_exports(source);
    let shapes: Vec<_> = exports
        .iter()
        .map(|export| (export.name.as_str(), export.kind))
        .collect();
    assert_eq!(
        shapes,
        vec![
            ("refresh", ExportKind::AsyncFunction),
            ("render", ExportKind::SyncFunction),
            ("value", ExportKind::Value),
            ("handler", ExportKind::AsyncFunction),
            ("wrapped", ExportKind::AsyncFunction),
            ("Widget", ExportKind::Class),
            ("default", ExportKind::AsyncFunction),
        ]
    );
}

#[test]
fn resolves_named_exports_through_local_declarations() {
    let source = "async function refresh() {}\nfunction render() {}\nexport { refresh, render };";
    let exports = scan_exports(source);
    assert_eq!(
        exports
            .iter()
            .map(|export| (export.name.as_str(), export.kind))
            .collect::<Vec<_>>(),
        vec![
            ("refresh", ExportKind::AsyncFunction),
            ("render", ExportKind::SyncFunction),
        ]
    );
}

#[test]
fn named_re_exports_are_marked_as_re_exports() {
    let exports = scan_exports("export { refresh } from \"./actions.js\";");
    assert_eq!(exports[0].kind, ExportKind::ReExport);
}

#[test]
fn export_star_declares_no_named_binding() {
    assert!(scan_exports("export * from \"./actions.js\";").is_empty());
}

#[test]
fn flow_type_exports_are_not_runtime_exports() {
    assert!(scan_exports("export type Props = { a: number };").is_empty());
}

#[test]
fn flow_annotated_const_exports_keep_their_shape() {
    let exports = scan_exports("export const run: () => Promise<void> = async () => {};");
    assert_eq!(exports[0].kind, ExportKind::AsyncFunction);
}

#[test]
fn finds_client_only_hooks_and_globals() {
    let source = "function Page() { const [a] = useState(1); window.scrollTo(0); }";
    let uses = scan_client_api_uses(source);
    let names: Vec<_> = uses.iter().map(|use_site| use_site.api).collect();
    assert_eq!(names, vec!["useState", "window"]);
}

#[test]
fn does_not_flag_client_apis_in_comments_or_strings() {
    let source = "// useState(1)\nconst doc = \"useState(1)\";";
    assert!(scan_client_api_uses(source).is_empty());
}

#[test]
fn does_not_flag_a_local_declaration_named_like_a_hook() {
    assert!(scan_client_api_uses("function useState() {}").is_empty());
}

#[test]
fn client_only_lists_are_sorted_for_binary_search() {
    let mut sorted = CLIENT_ONLY_APIS.to_vec();
    sorted.sort_unstable();
    assert_eq!(sorted, CLIENT_ONLY_APIS.to_vec());

    let mut globals = CLIENT_ONLY_GLOBALS.to_vec();
    globals.sort_unstable();
    assert_eq!(globals, CLIENT_ONLY_GLOBALS.to_vec());
}

#[test]
fn empty_source_scans_cleanly() {
    assert!(tokenize("").is_empty());
    assert!(scan_imports("").is_empty());
    assert!(scan_exports("").is_empty());
    assert!(scan_client_api_uses("").is_empty());
}

/// The worked example from ubugeeei-prod/uf#348: `useRoute` is built on
/// `useContext`, and the name lists have never heard of it.
#[test]
fn finds_a_call_to_a_hook_the_name_lists_do_not_know() {
    let source = "function Masthead() { const { pathname } = useRoute(); }";
    assert!(
        scan_client_api_uses(source).is_empty(),
        "the name match has no opinion about `useRoute`, which is the problem"
    );
    let calls = scan_hook_calls(source);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "useRoute");
    assert_eq!(calls[0].line, 1);
}

/// A hook that *is* on the list is answered rather than asked about; reporting
/// it twice would turn one violation into a violation and a shrug.
#[test]
fn a_known_client_hook_is_not_also_an_unanswered_question() {
    let calls = scan_hook_calls("function Page() { const [a] = useState(1); }");
    assert!(calls.is_empty(), "{calls:?}");
}

/// The three exclusions the client-only match already makes, which are the
/// three that keep this from being noise.
#[test]
fn a_hook_call_is_a_call_to_a_hook_by_name() {
    // A declaration is not a use.
    assert!(scan_hook_calls("function useRoute() {}").is_empty());
    assert!(scan_hook_calls("const useRoute = () => {};").is_empty());
    // A property access belongs to whatever owns it.
    assert!(scan_hook_calls("router.useRoute();").is_empty());
    // A reference that is not called is not a hook call: passing `useRoute`
    // somewhere is not running it here.
    assert!(scan_hook_calls("register(useRoute);").is_empty());
    // And React's naming rule is the whole rule: `used` is not `useD`.
    assert!(scan_hook_calls("used(1);").is_empty());
    assert!(scan_hook_calls("use(1);").is_empty());
    assert!(!scan_hook_calls("useRoute();").is_empty());
}

/// A comment or a string is not code, here as everywhere else in the scanner.
#[test]
fn does_not_flag_hook_calls_in_comments_or_strings() {
    assert!(scan_hook_calls("// useRoute()\nconst s = \"useRoute()\";").is_empty());
}

/// A method *definition* is not a call, and this is the one the rule was
/// getting wrong: `useRoute() {}` in an object literal or a class body is a
/// hook being written, not one being run, and a module that only defines it
/// was emitting `rsc/unclassified-hook-in-server` against a line that does
/// nothing. A warning that fires on a definition teaches people to ignore the
/// rule, which costs more than the rule is worth.
#[test]
fn a_method_definition_is_not_a_hook_call() {
    for source in [
        "export const routes = { useRoute() { return null; } };",
        "class Router { useRoute() { return null; } }",
        "class Router { async useRoute(): Promise<void> {} }",
        "class Router { static useRoute() {} }",
        "const o = { get useRoute() { return 1; } };",
        "class Router { *useRoute() {} }",
        "class Router { other() {} useRoute() {} }",
        "type R = {| useRoute(): void |};",
        "interface R { useRoute(): void }",
    ] {
        assert!(
            scan_hook_calls(source).is_empty(),
            "definition reported as a call: {source}"
        );
    }
}

/// And every shape of an actual call still is one. The definition test above
/// is only safe if this one holds: a check that silences the rule in a place
/// it should speak is the more expensive mistake of the two.
#[test]
fn a_call_next_to_a_brace_is_still_a_hook_call() {
    for source in [
        "useRoute();",
        "if (useRoute()) { go(); }",
        "f(a, useRoute());",
        "{ useRoute(); }",
        "const x = cond ? useRoute() : null;",
        "for (const p of useRoute()) { go(); }",
        "const v = useRoute().pathname;",
        "async function f() { await useRoute(); }",
        "function f() { return useRoute(); }",
        "switch (useRoute()) { default: break; }",
        "const flags = a | useRoute();",
    ] {
        assert_eq!(
            scan_hook_calls(source).len(),
            1,
            "call not reported: {source}"
        );
    }
}

/// The same mistake the client-only name match could make, since a method
/// definition is a declaration site there too.
#[test]
fn a_method_definition_is_not_a_client_api_use() {
    assert!(scan_client_api_uses("class Store { useState() {} }").is_empty());
    assert!(scan_client_api_uses("const o = { useEffect() {} };").is_empty());
    assert!(!scan_client_api_uses("function Page() { useState(1); }").is_empty());
}

// ---------------------------------------------------------------------------
// Import bindings: which local name an import binds, and to which export.
// ---------------------------------------------------------------------------

fn bindings(source: &str) -> Vec<(ImportedName, String)> {
    scan_imports(source)
        .iter()
        .flat_map(|import| import.bindings.clone())
        .map(|binding| (binding.imported, binding.local.to_string()))
        .collect()
}

fn named(imported: &str, local: &str) -> (ImportedName, String) {
    (
        ImportedName::Named(CompactString::from(imported)),
        local.to_string(),
    )
}

/// The case the pair exists for. Collapsing `{ imported, local }` into either
/// half loses an aliased import: the local name `useS` is what the call site
/// two lines down says, and `useState` is what a package's table is keyed on.
#[test]
fn an_aliased_import_keeps_both_names() {
    let source = "import { useState as useS } from \"react\";\n";
    assert_eq!(bindings(source), vec![named("useState", "useS")]);

    let import = &scan_imports(source)[0];
    assert_eq!(import.specifier, "react");
    assert_eq!(
        import.bindings[0].imported.as_export_name(),
        Some("useState"),
        "the aliased name must still resolve to the export it renames"
    );
}

/// An unaliased import binds the same name twice, which is what makes a lookup
/// by either half agree.
#[test]
fn an_unaliased_import_binds_the_name_it_names() {
    assert_eq!(
        bindings("import { useRoute } from \"@uniflowed/router\";\n"),
        vec![named("useRoute", "useRoute")]
    );
}

#[test]
fn every_clause_shape_records_what_it_binds() {
    assert_eq!(
        bindings("import Counter from \"./Counter.js\";\n"),
        vec![(ImportedName::Default, "Counter".to_string())]
    );
    assert_eq!(
        bindings("import * as hooks from \"@uniflowed/hooks\";\n"),
        vec![(ImportedName::Namespace, "hooks".to_string())]
    );
    assert_eq!(
        bindings("import React, { useState, useRef as ref } from \"react\";\n"),
        vec![
            (ImportedName::Default, "React".to_string()),
            named("useState", "useState"),
            named("useRef", "ref"),
        ]
    );
    assert_eq!(
        bindings("import Counter, * as all from \"./Counter.js\";\n"),
        vec![
            (ImportedName::Default, "Counter".to_string()),
            (ImportedName::Namespace, "all".to_string()),
        ]
    );
    // `import x` and `import { default as x }` name the same export, so they
    // are the same variant here.
    assert_eq!(
        bindings("import { default as Counter } from \"./Counter.js\";\n"),
        vec![(ImportedName::Default, "Counter".to_string())]
    );
    // ES2022 arbitrary module namespace names are legal export names.
    assert_eq!(
        bindings("import { \"use-route\" as useRoute } from \"./m.js\";\n"),
        vec![named("use-route", "useRoute")]
    );
}

/// A re-export binds no module-scope name; `useCurrentRoute` is what this
/// module exports the other module's `useRoute` as, which answers the same
/// question from the other side.
#[test]
fn a_re_export_records_the_name_it_publishes() {
    let source = "export { useRoute as useCurrentRoute } from \"./router.js\";\n";
    assert_eq!(scan_imports(source)[0].kind, ImportKind::ReExport);
    assert_eq!(bindings(source), vec![named("useRoute", "useCurrentRoute")]);
}

/// The forms that are edges and bind nothing a clause can name.
#[test]
fn a_form_with_no_clause_binds_nothing() {
    for source in [
        "import \"./side-effect.js\";\n",
        "export * from \"./barrel.js\";\n",
        "const lazy = import(\"./lazy.js\");\n",
        "const legacy = require(\"./legacy.js\");\n",
    ] {
        assert!(
            scan_imports(source)[0].bindings.is_empty(),
            "clause-less form bound something: {source}"
        );
    }
}

/// Inline type specifiers are erased with the rest of the types, exactly as the
/// statement-level `import type` is — and by the same test, so `{ type }` and
/// `{ type as t }`, which import a value called `type`, survive.
#[test]
fn inline_type_specifiers_are_not_bindings() {
    assert_eq!(
        bindings("import { type Theme, typeof Store, useTheme } from \"./theme.js\";\n"),
        vec![named("useTheme", "useTheme")]
    );
    assert_eq!(
        bindings("import { type } from \"./m.js\";\n"),
        vec![named("type", "type")]
    );
    assert_eq!(
        bindings("import { type as t } from \"./m.js\";\n"),
        vec![named("type", "t")]
    );
    assert_eq!(
        bindings("import { type Theme as T } from \"./theme.js\";\n"),
        vec![],
        "an aliased type specifier is still a type specifier"
    );
}

// ---------------------------------------------------------------------------
// Owners: which body a use site is in.
// ---------------------------------------------------------------------------

fn use_owners(source: &str) -> Vec<(&'static str, Option<String>)> {
    scan_client_api_uses(source)
        .iter()
        .map(|use_site| {
            (
                use_site.api,
                use_site.owner.as_ref().map(ToString::to_string),
            )
        })
        .collect()
}

/// The nesting case. A `localStorage` inside a callback inside `useTheme`
/// belongs to `useTheme`: the callback has no name any importer can reach, and
/// `useTheme` is the binding an export-graph fixpoint propagates along.
#[test]
fn a_use_inside_a_nested_closure_belongs_to_the_declaration_that_holds_it() {
    let source = r#"
export hook useTheme() {
  const [theme, setTheme] = useState("system");
  useEffect(() => {
    const stored = localStorage.getItem("theme");
    subscribe(() => setTheme(stored));
  }, []);
  return theme;
}
"#;
    assert_eq!(
        use_owners(source),
        vec![
            ("useState", Some("useTheme".to_string())),
            ("useEffect", Some("useTheme".to_string())),
            ("localStorage", Some("useTheme".to_string())),
        ]
    );
}

/// And a use no declaration holds gets nothing rather than the nearest name.
/// A wrong owner propagates through a fixpoint; a missing one is the answer
/// this crate already gives for everything it cannot decide.
#[test]
fn a_use_at_module_top_level_has_no_owner() {
    let source = r#"
const theme = localStorage.getItem("theme");
export hook useTheme() {
  return useState(theme);
}
if (window.matchMedia) { track(); }
"#;
    assert_eq!(
        use_owners(source),
        vec![
            ("localStorage", None),
            ("useState", Some("useTheme".to_string())),
            ("window", None),
        ]
    );
}

/// Every declaration shape a hook is written in, including the one the issue
/// was filed about: a Flow return type holding a function type used to make
/// the walk mistake its `=>` for the head of an arrow function, so `useTheme`
/// had no name at all.
#[test]
fn a_declaration_is_named_however_it_is_written() {
    for (source, owner) in [
        ("function Page() { useState(1); }", "Page"),
        ("export function Page() { useState(1); }", "Page"),
        ("export async function load() { useState(1); }", "load"),
        ("hook useTheme() { useState(1); }", "useTheme"),
        ("component Card() { useState(1); }", "Card"),
        ("const useTheme = () => { useState(1); };", "useTheme"),
        (
            "export const useTheme = function () { useState(1); };",
            "useTheme",
        ),
        ("export default function Page() { useState(1); }", "Page"),
        ("export default function () { useState(1); }", "default"),
        ("export default () => { useState(1); };", "default"),
        (
            "export hook useTheme(): Promise<number> { useState(1); }",
            "useTheme",
        ),
        (
            "export hook useTheme(): [string, (next: string) => void] { useState(1); }",
            "useTheme",
        ),
    ] {
        assert_eq!(
            use_owners(source),
            vec![("useState", Some(owner.to_string()))],
            "wrong owner for: {source}"
        );
    }
}

/// Two declarations in one module are two disjoint bodies, and neither borrows
/// the other's uses.
#[test]
fn each_declaration_owns_only_its_own_body() {
    let source = r#"
export hook useA() { useState(1); }
export hook useB() { useEffect(noop); }
"#;
    assert_eq!(
        use_owners(source),
        vec![
            ("useState", Some("useA".to_string())),
            ("useEffect", Some("useB".to_string())),
        ]
    );
}

/// A method body is not a module-level declaration, and naming it would be
/// worse than naming nothing: `read` is not a name an importer can reach, and
/// it can collide with an export that has nothing to do with it.
#[test]
fn a_use_inside_a_method_has_no_module_level_owner() {
    for source in [
        "class Store { read() { return localStorage.getItem(\"k\"); } }",
        "const store = { read() { return localStorage.getItem(\"k\"); } };",
        "register({ onMount() { useEffect(noop); } });",
    ] {
        let owners: Vec<_> = scan_client_api_uses(source)
            .iter()
            .map(|use_site| use_site.owner.clone())
            .collect();
        assert_eq!(owners, vec![None], "method body claimed an owner: {source}");
    }
}

/// The other half of the fixpoint's edge set. A wrapper is client-only when it
/// reaches a client-only API *or* calls a hook that does, and the second clause
/// needs the calling body as much as the first.
#[test]
fn a_hook_call_records_the_body_that_makes_it() {
    let source = r#"
export hook useTheme() {
  const { pathname } = useRoute();
  useMediaQuery(() => track());
}
useRoute();
"#;
    let owners: Vec<_> = scan_hook_calls(source)
        .iter()
        .map(|call| {
            (
                call.name.to_string(),
                call.owner.as_ref().map(ToString::to_string),
            )
        })
        .collect();
    assert_eq!(
        owners,
        vec![
            ("useRoute".to_string(), Some("useTheme".to_string())),
            ("useMediaQuery".to_string(), Some("useTheme".to_string())),
            ("useRoute".to_string(), None),
        ]
    );
}

/// An anonymous body at module level names nobody, so nothing inside it is
/// attributed — the walk declines rather than reaching for the next name up.
#[test]
fn an_anonymous_module_level_body_owns_nothing() {
    let owners: Vec<_> = scan_client_api_uses("(() => { useState(1); })();")
        .iter()
        .map(|use_site| use_site.owner.clone())
        .collect();
    assert_eq!(owners, vec![None]);
}

/// An unbalanced source is a syntax error somebody else reports; the walk still
/// has to finish over it rather than loop or panic.
#[test]
fn an_unbalanced_source_still_scans() {
    for source in [
        "export hook useTheme() { useState(1);",
        "}}} useState(1);",
        "function f( { useState(1); }",
        "import { useState as from \"react\";",
    ] {
        let _ = scan_client_api_uses(source);
        let _ = scan_hook_calls(source);
        let _ = scan_imports(source);
    }
}
