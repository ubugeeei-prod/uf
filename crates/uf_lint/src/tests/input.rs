//! Bytes rather than rules: empty files, CRLF, a byte order mark, non-ASCII text,
//! a missing trailing newline, and input large enough to matter.

use super::*;

#[test]
fn empty_input_produces_no_diagnostics() {
    let report = lint_source(&at("app/index.js", ""), &UniflowedConfig::default()).expect("lint");

    assert!(report.diagnostics.is_empty());
    assert_eq!(report.files_checked, 1);
}

#[test]
fn crlf_line_endings_do_not_shift_positions() {
    let diagnostics = lint_js("flow/unclear-type", "// @flow\r\ntype A = any;\r\n");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 10));
}

#[test]
fn crlf_line_endings_are_not_trailing_whitespace() {
    let diagnostics = lint_js("uniflowed/no-trailing-whitespace", "let a = 1;\r\n");

    assert!(diagnostics.is_empty());
}

#[test]
fn a_byte_order_mark_does_not_shift_later_lines() {
    let diagnostics = lint_js("flow/unclear-type", "\u{feff}// @flow\ntype A = any;\n");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
}

#[test]
fn non_ascii_content_keeps_line_numbers_correct() {
    let diagnostics = lint_js(
        "flow/unclear-type",
        "// @flow\nconst s = 'ようこそ — добро пожаловать';\ntype A = any;\n",
    );

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn a_file_without_a_trailing_newline_is_linted() {
    let diagnostics = lint_js("flow/unclear-type", "// @flow\ntype A = any;");

    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, 2);
}

#[test]
fn very_large_input_is_linted_without_blowing_up() {
    let mut text = String::from("// @flow\n");
    for index in 0..5_000 {
        text.push_str("type A");
        text.push_str(&index.to_string());
        text.push_str(" = any;\n");
    }
    let diagnostics = lint_js("flow/unclear-type", &text);

    assert_eq!(diagnostics.len(), 5_000);
    assert_eq!(diagnostics[4_999].line, 5_001);
}
