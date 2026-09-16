//! `react/no-redundant-memo`: the memoization the React Compiler already did.
//!
//! Every case here is tested from both sides. A linter is only as useful as the
//! reports a reader can act on, so a fixture that must fire is worth exactly as
//! much as one that must not.

use super::*;

const REDUNDANT_MEMO: &str = "react/no-redundant-memo";

/// A module that imports the hooks it uses, so each fixture is a real module.
fn module(body: &str) -> String {
    format!(
        "// @flow\nimport {{useCallback, useEffect, useLayoutEffect, useMemo, useState}} from \"react\";\n\n{body}"
    )
}

fn memo(body: &str) -> Vec<Diagnostic> {
    lint_js(REDUNDANT_MEMO, &module(body))
}

// --- react/no-redundant-memo ------------------------------------------------

#[test]
fn a_memo_the_compiler_removed_is_reported() {
    let diagnostics = memo(
        "component List(items: Array<string>) {\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (5, 18));
    assert!(
        diagnostics[0].message.contains("useMemo"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn a_callback_the_compiler_removed_is_reported() {
    let diagnostics = memo(
        "hook useHandler(id: string): () => void {\n  return useCallback(() => send(id), [id]);\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("useCallback"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn the_namespaced_spelling_is_reported_at_the_hook_and_not_at_react() {
    // `React.useMemo(…)` reported at its callee underlines `React`, which says
    // nothing about which call is meant on a line that holds two of them.
    let diagnostics = lint_js(
        REDUNDANT_MEMO,
        "// @flow\nimport * as React from \"react\";\n\ncomponent List(items: Array<string>) {\n  const sorted = React.useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (5, 24));
}

#[test]
fn a_memo_in_a_function_the_compiler_never_touches_is_left_alone() {
    // `syntax` mode compiles `component` and `hook` declarations. A plain
    // function is an uncompiled boundary, and the memoization in it is the
    // only memoization there is. The compiler's result says so; nothing in
    // uf reads the text to guess it.
    let diagnostics = memo(
        "export function List(props: {items: Array<string>}) {\n  const sorted = useMemo(() => props.items.slice(), [props.items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_memo_the_compiler_could_not_preserve_is_left_alone() {
    // A missing dependency makes the compiler refuse the whole function rather
    // than drop a memo whose guarantees it cannot reproduce — so the report
    // this rule would have made is exactly the report that would be wrong.
    let diagnostics = memo(
        "component Sum(a: number, b: number) {\n  const total = useMemo(() => a + b, [a]);\n  return <p>{total}</p>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_function_that_opted_out_of_the_compiler_keeps_its_memoization() {
    let diagnostics = memo(
        "component List(items: Array<string>) {\n  \"use no memo\";\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_project_with_the_compiler_switched_off_keeps_its_memoization() {
    // Nothing removes it, so it is not redundant — and a rule that said
    // otherwise would be telling a reader to delete the only memoization the
    // build produces.
    let mut config = only(REDUNDANT_MEMO);
    config.app.builtins.react_compiler.enabled = false;
    let file = at(
        "app/index.js",
        &module(
            "component List(items: Array<string>) {\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
        ),
    );

    let diagnostics = lint_source(&file, &config).expect("lint").diagnostics;

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

// --- What is never parsed ---------------------------------------------------

#[test]
fn a_module_that_names_neither_hook_is_not_parsed_at_all() {
    // The gate that keeps the parse off the common path. It is behaviour
    // rather than an optimisation detail: a module with no `useMemo` or
    // `useCallback` in it gives the rule nothing to report, and one that fails
    // to parse must be reported by `flow/syntax` and nothing else.
    let broken = "// @flow\ncomponent Page( {\n";

    assert!(lint_js(REDUNDANT_MEMO, broken).is_empty());
}

#[test]
fn a_module_that_does_not_parse_reports_nothing_here() {
    let broken = "// @flow\nimport {useEffect, useMemo, useState} from \"react\";\ncomponent Page( {\n  const a = useMemo(() => 1, []);\n";

    assert!(lint_js(REDUNDANT_MEMO, broken).is_empty());
}

#[test]
fn a_non_flow_file_is_not_parsed() {
    assert!(
        lint_one(
            REDUNDANT_MEMO,
            "app/page.md",
            "const a = useMemo(() => 1, []);\n"
        )
        .is_empty()
    );
}

/// The caret lands on the hook even when the line holds an astral character.
///
/// The finding comes from the Babel tree, whose `loc` columns `babel::finalize`
/// recomputes as UTF-16 code units, and a diagnostic carries a byte column.
/// Converting one as if it were the other moves the caret on any line with an
/// emoji in it, which is invisible until somebody writes one.
///
/// The comment before the hook is deliberate: `mask_inline_comments` replaces a
/// comment byte for byte, so the masked line is the same length and the
/// conversion has to read the *unmasked* source to see the character at all.
#[test]
fn an_astral_character_before_the_hook_does_not_move_the_caret() {
    let diagnostics = memo(
        "component List(items: Array<string>) {\n  const sorted = /* \u{1f600} */ useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    // `  const sorted = ` is seventeen bytes, `/* ` three, the emoji four and
    // ` */ ` four: `useMemo` starts at byte twenty-nine, one-based. Counted as
    // UTF-16 units the emoji is two, and read as bytes the answer would come
    // out twenty-seven.
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (5, 29));
}
