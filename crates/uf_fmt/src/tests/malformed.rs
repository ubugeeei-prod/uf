//! Input the formatter has to survive rather than reject: truncated literals,
//! unbalanced delimiters, a one megabyte line and nesting deep enough to exhaust
//! a recursive implementation.

use super::*;

#[test]
fn malformed_input_never_panics_and_stays_token_preserving() {
    let long_line = format!("const x = [{}];\n", "1, ".repeat(60_000));
    let deep = {
        let depth = 10_000;
        let mut source = String::with_capacity(depth * 2 + 1);
        for _ in 0..depth {
            source.push('{');
        }
        for _ in 0..depth {
            source.push('}');
        }
        source.push('\n');
        source
    };
    let cases: Vec<String> = vec![
        "'unterminated".to_string(),
        "\"unterminated".to_string(),
        "`unterminated ${".to_string(),
        "`${`${`${".to_string(),
        "/* unterminated".to_string(),
        "/** unterminated".to_string(),
        "\\".to_string(),
        "\\\\\\".to_string(),
        "}}}}".to_string(),
        "((((".to_string(),
        "[[[[".to_string(),
        "<div>".to_string(),
        "</div>".to_string(),
        "<div><span></div>".to_string(),
        "const x = /unterminated".to_string(),
        "\0\u{1}\u{2}".to_string(),
        "#!".to_string(),
        "?:".to_string(),
        "a\u{2028}b".to_string(),
        long_line,
        deep,
    ];

    for source in &cases {
        let first = format(source);
        // Idempotence alone is too weak here: a formatter can make a stable
        // token-changing rewrite and pass it. Malformed input is exactly
        // where a recovery path might silently drop or invent a token, so
        // the stream has to be checked too.
        assert_token_kinds_preserved(source, &first);
        let second = format(&first);
        similar_asserts::assert_eq!(first, second, "not idempotent for {source:?}");
    }
}

#[test]
fn a_one_megabyte_single_line_is_formatted() {
    let source = format!("const x = \"{}\";\n", "a".repeat(1_000_000));
    let formatted = format(&source);
    assert_eq!(formatted, source);
}

#[test]
fn deeply_nested_indentation_is_capped() {
    let depth = 2_000;
    let mut source = String::new();
    for _ in 0..depth {
        source.push_str("{\n");
    }
    for _ in 0..depth {
        source.push_str("}\n");
    }
    let formatted = format(&source);
    let widest = formatted
        .lines()
        .map(|line| line.len() - line.trim_start().len())
        .max()
        .unwrap_or(0);
    assert!(widest <= 256 * 2, "indent grew to {widest} columns");
}
