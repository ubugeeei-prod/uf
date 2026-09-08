//! The catalogue: what has a fix, in which tier, and how a batch of them is
//! planned and applied.

use uf_lint::Severity;

use super::*;

fn diagnostic(rule: &'static str, line: usize, column: usize) -> Diagnostic {
    Diagnostic {
        rule,
        severity: Severity::Error,
        path: Some("app/index.js".to_owned()),
        line,
        column,
        message: String::new(),
    }
}

#[test]
fn a_deprecated_bool_is_replaced_where_the_rule_pointed() {
    let fix = fix_for(&diagnostic("flow/deprecated-type", 2, 10), "type B = bool;").expect("a fix");

    assert_eq!(fix.line, 1);
    assert_eq!((fix.start, fix.end), (9, 13));
    assert_eq!(fix.replacement, "boolean");
    assert_eq!(fix.safety, Safety::Safe);
}

/// A stale range is the normal case, not the exotic one: an editor asks for
/// actions over the range it had before the keystroke that changed the line.
#[test]
fn a_line_that_no_longer_says_bool_gets_no_fix() {
    assert!(
        fix_for(
            &diagnostic("flow/deprecated-type", 2, 10),
            "type B = boolean;"
        )
        .is_none()
    );
    assert!(fix_for(&diagnostic("flow/deprecated-type", 2, 10), "type B =").is_none());
    assert!(fix_for(&diagnostic("flow/deprecated-type", 2, 99), "type B = bool;").is_none());
    assert!(fix_for(&diagnostic("flow/deprecated-type", 2, 0), "type B = bool;").is_none());
}

/// A column landing inside a multi-byte character must not panic.
#[test]
fn a_column_inside_a_character_gets_no_fix() {
    assert!(fix_for(&diagnostic("flow/deprecated-type", 1, 2), "π = bool;").is_none());
}

/// `boolish` starts with `bool` and is not the deprecated alias.
#[test]
fn a_longer_identifier_beginning_with_bool_is_not_the_alias() {
    assert!(
        fix_for(
            &diagnostic("flow/deprecated-type", 1, 10),
            "type B = boolish;"
        )
        .is_none()
    );
}

#[test]
fn a_rule_without_a_mechanical_answer_offers_nothing() {
    assert!(fix_for(&diagnostic("flow/unclear-type", 1, 10), "type A = any;").is_none());
    assert!(
        fix_for(
            &diagnostic("flow/unnecessary-optional-chain", 1, 1),
            "this?.x;"
        )
        .is_none()
    );
}

#[test]
fn a_mutable_export_is_fixed_but_only_when_asked_for() {
    let fix = fix_for(
        &diagnostic("flow/non-const-var-export", 1, 8),
        "export let count = 0;",
    )
    .expect("a fix");

    assert_eq!((fix.start, fix.end), (7, 10));
    assert_eq!(fix.replacement, "const");
    assert_eq!(
        fix.safety,
        Safety::Unsafe,
        "`const` throws where `let` worked, which is a fact this line does not carry"
    );
}

#[test]
fn a_mutable_export_declared_with_var_is_the_same_fix() {
    let fix = fix_for(
        &diagnostic("flow/non-const-var-export", 1, 8),
        "export var count = 0;",
    )
    .expect("a fix");

    assert_eq!((fix.start, fix.end), (7, 10));
    assert_eq!(fix.replacement, "const");
    assert_eq!(fix.title, "Replace `var` with `const`");
}

/// The shapes where `const` would not parse. Each one is a file the fixer
/// would have broken, so each one is checked rather than assumed.
#[test]
fn a_mutable_export_const_could_not_replace_gets_no_fix() {
    let cases = [
        // No initialiser: `export const x;` is a syntax error.
        "export let count;",
        // A declaration list: the second binding has no initialiser.
        "export let a = 1, b;",
        // A declaration list where the *first* binding has none either.
        "export let a, b = 1;",
        // An annotated binding with no initialiser.
        "export let count: number;",
        // An initialiser that runs on to the next line, whose rest this line
        // does not show.
        "export let config = {",
        // Not the keyword at all.
        "export letters = 1;",
        // A name that is not a name.
        "export let 9 = 1;",
        // A comparison is not an initialiser.
        "export let a == 1;",
    ];
    for case in cases {
        assert!(
            fix_for(&diagnostic("flow/non-const-var-export", 1, 8), case).is_none(),
            "expected no fix for {case:?}"
        );
    }
}

/// The annotation between the name and the `=` is the common case in Flow,
/// not the exotic one, and each of these has a bracket or an `=` in it that a
/// naive search would stop at.
#[test]
fn an_annotated_binding_still_gets_the_fix() {
    let cases = [
        "export let count: number = 0;",
        "export let items: Array<number> = [];",
        "export let table: {| a: number |} = seed;",
        "export let render: (x: number) => string = show;",
        "export let pair: Map<string, number> = new Map();",
    ];
    for case in cases {
        assert!(
            fix_for(&diagnostic("flow/non-const-var-export", 1, 8), case).is_some(),
            "expected a fix for {case:?}"
        );
    }
}

/// A comma inside the initialiser is not a second declarator.
#[test]
fn a_comma_inside_the_initialiser_still_gets_the_fix() {
    assert!(
        fix_for(
            &diagnostic("flow/non-const-var-export", 1, 8),
            "export let total = add(1, 2);",
        )
        .is_some()
    );
    assert!(
        fix_for(
            &diagnostic("flow/non-const-var-export", 1, 8),
            "export let name = \"a, b\";",
        )
        .is_some()
    );
}

#[test]
fn planning_leaves_out_unsafe_fixes_unless_they_were_asked_for() {
    let source = "// @flow\ntype B = bool;\nexport let count: B = true;\n";
    let diagnostics = vec![
        diagnostic("flow/deprecated-type", 2, 10),
        diagnostic("flow/non-const-var-export", 3, 8),
    ];

    let safe = plan(source, &diagnostics, false);
    assert_eq!(safe.len(), 1);
    assert_eq!(safe[0].replacement, "boolean");

    let both = plan(source, &diagnostics, true);
    assert_eq!(both.len(), 2);
    assert_eq!(both[1].replacement, "const");
}

#[test]
fn planning_is_in_document_order_whatever_order_the_rules_ran_in() {
    let source = "type A = bool;\ntype B = bool;\n";
    let diagnostics = vec![
        diagnostic("flow/deprecated-type", 2, 10),
        diagnostic("flow/deprecated-type", 1, 10),
    ];

    let fixes = plan(source, &diagnostics, true);
    assert_eq!(fixes.len(), 2);
    assert_eq!((fixes[0].line, fixes[1].line), (0, 1));
}

/// Two fixes over the same bytes: one is applied and the other is dropped, so
/// the text that comes out is text one rule asked for rather than a splice of
/// two. The caller lints the result and plans again.
#[test]
fn two_fixes_over_the_same_bytes_do_not_both_apply() {
    let source = "type B = bool;\n";
    // The same diagnostic reported twice is the smallest overlapping pair
    // there is, and it is a real one: two rules that agree about a span both
    // report it.
    let diagnostics = vec![
        diagnostic("flow/deprecated-type", 1, 10),
        diagnostic("flow/deprecated-type", 1, 10),
    ];

    let fixes = plan(source, &diagnostics, true);
    assert_eq!(fixes.len(), 1);
    assert_eq!(apply(source, &fixes), "type B = boolean;\n");
}

fn span(title: &'static str, start: usize, end: usize, replacement: &'static str) -> Fix {
    Fix {
        title,
        safety: Safety::Safe,
        line: 0,
        start,
        end,
        replacement,
    }
}

/// The decision the module documentation writes down: apply one, drop the
/// rest, and let the caller re-lint. Spelled out over [`apply`] so the text
/// that comes out is visible, because "corrupt" is what the alternative looks
/// like rather than "wrong".
#[test]
fn overlapping_fixes_keep_the_earlier_one_and_leave_the_text_a_rule_asked_for() {
    let source = "let value = 1;\n";
    let planned = resolve_overlaps(vec![
        span("later", 6, 9, "BBB"),
        span("earlier", 4, 8, "AAA"),
    ]);

    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].title, "earlier");
    assert_eq!(apply(source, &planned), "let AAAe = 1;\n");
}

/// Fixes that merely touch — one ends where the next begins — are not
/// overlapping, and dropping one of those would lose a fix for nothing.
#[test]
fn adjacent_fixes_both_apply() {
    let source = "let value = 1;\n";
    let planned = resolve_overlaps(vec![span("a", 4, 7, "AAA"), span("b", 7, 9, "BB")]);

    assert_eq!(planned.len(), 2);
    assert_eq!(apply(source, &planned), "let AAABB = 1;\n");
}

/// Two fixes on different lines cannot overlap however their columns compare.
#[test]
fn fixes_on_different_lines_never_overlap() {
    let mut second = span("b", 0, 9, "SECOND");
    second.line = 1;
    let planned = resolve_overlaps(vec![span("a", 4, 9, "FIRST"), second]);

    assert_eq!(planned.len(), 2);
}

#[test]
fn applying_no_fixes_returns_the_source_unchanged() {
    let source = "type B = bool;\n";
    assert_eq!(apply(source, &[]), source);
}

/// The file's line endings are not the fixer's business. Rebuilding the text
/// from `str::lines` would rewrite every one of them.
#[test]
fn applying_a_fix_keeps_crlf_terminators() {
    let source = "// @flow\r\ntype B = bool;\r\n";
    let fixes = plan(source, &[diagnostic("flow/deprecated-type", 2, 10)], false);

    assert_eq!(apply(source, &fixes), "// @flow\r\ntype B = boolean;\r\n");
}

#[test]
fn applying_a_fix_to_a_line_with_no_terminator_works() {
    let source = "type B = bool;";
    let fixes = plan(source, &[diagnostic("flow/deprecated-type", 1, 10)], false);

    assert_eq!(apply(source, &fixes), "type B = boolean;");
}

/// A multi-byte character before the fix shifts every byte offset after it.
#[test]
fn a_fix_after_a_multibyte_character_lands_on_the_right_bytes() {
    let source = "const π = 1;\ntype B = bool;\n";
    let fixes = plan(source, &[diagnostic("flow/deprecated-type", 2, 10)], false);

    assert_eq!(apply(source, &fixes), "const π = 1;\ntype B = boolean;\n");
}

#[test]
fn a_diagnostic_pointing_past_the_end_of_the_file_plans_nothing() {
    let source = "type B = bool;\n";
    assert!(plan(source, &[diagnostic("flow/deprecated-type", 9, 10)], true).is_empty());
    assert!(plan(source, &[diagnostic("flow/deprecated-type", 0, 10)], true).is_empty());
}

#[test]
fn line_spans_agree_with_the_lines_a_diagnostic_counts() {
    for source in ["", "a", "a\n", "a\nb", "a\n\nb\n", "a\r\nb\r\n"] {
        let spans = line_spans(source);
        let lines: Vec<&str> = source.lines().collect();
        assert_eq!(spans.len(), lines.len(), "{source:?}");
        for (index, (offset, text)) in spans.iter().enumerate() {
            assert_eq!(*text, lines[index], "{source:?}");
            assert_eq!(&source[*offset..*offset + text.len()], *text, "{source:?}");
        }
    }
}

#[test]
fn a_hot_read_gains_the_optional_chain_but_only_when_asked_for() {
    let line = "import.meta.hot.accept((module) => module);";
    let fix = fix_for(&diagnostic("vite/hot-needs-optional-chaining", 2, 1), line).expect("a fix");

    assert_eq!(fix.safety, Safety::Unsafe);
    assert_eq!(fix.replacement, "?.");
    assert_eq!(&line[fix.start..fix.end], ".");
    assert_eq!(
        apply(&format!("// @flow\n{line}\n"), &[fix]),
        "// @flow\nimport.meta.hot?.accept((module) => module);\n"
    );
    assert!(plan(&format!("// @flow\n{line}\n"), &[], false).is_empty());
}

#[test]
fn a_hot_read_inside_a_guard_is_fixed_where_the_rule_pointed() {
    // Two leading spaces: the rule reports the member expression, not the line.
    let line = "  import.meta.hot.invalidate(message);";
    let fix = fix_for(&diagnostic("vite/hot-needs-optional-chaining", 3, 3), line).expect("a fix");

    assert_eq!(
        apply(
            &format!("// @flow\nif (import.meta.hot) {{\n{line}\n}}\n"),
            &[fix]
        ),
        "// @flow\nif (import.meta.hot) {\n  import.meta.hot?.invalidate(message);\n}\n"
    );
}

#[test]
fn a_hot_read_that_already_chains_gets_no_fix() {
    // Not a line this rule reports, and the guard is what keeps a stale range
    // from splicing a second `?` into a chain that has one.
    assert!(
        fix_for(
            &diagnostic("vite/hot-needs-optional-chaining", 2, 1),
            "import.meta.hot?.accept();"
        )
        .is_none()
    );
    assert!(
        fix_for(
            &diagnostic("vite/hot-needs-optional-chaining", 2, 1),
            "const url = import.meta.url;"
        )
        .is_none()
    );
}

#[test]
fn the_accessibility_rules_have_no_mechanical_answer() {
    // Each of these asks for markup to be rearranged or for words only a
    // person has; see the module documentation for the argument per rule.
    for rule in [
        "a11y/alt-text",
        "a11y/aria-props",
        "a11y/heading-order",
        "a11y/label-has-associated-control",
        "a11y/no-static-element-interactions",
        "markup/no-invalid-nesting",
    ] {
        assert!(
            fix_for(&diagnostic(rule, 2, 1), "<img src=\"/cat.png\" />").is_none(),
            "{rule} offered a fix"
        );
    }
}
