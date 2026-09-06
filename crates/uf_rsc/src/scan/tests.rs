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
