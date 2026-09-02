//! Bytes rather than syntax: the byte order mark, CRLF and lone CR line endings,
//! and multi-byte characters, which the printer measures in characters but must
//! never split.

use super::*;

#[test]
fn a_leading_byte_order_mark_is_preserved() {
    let formatted = format("\u{feff}const x = 1;\n");
    assert!(formatted.starts_with('\u{feff}'));
    similar_asserts::assert_eq!(formatted, "\u{feff}const x = 1;\n");
}

#[test]
fn crlf_is_normalized_to_lf() {
    similar_asserts::assert_eq!(
        format("const x = 1;\r\nconst y = 2;\r\n"),
        "const x = 1;\nconst y = 2;\n"
    );
    similar_asserts::assert_eq!(format("a;\rb;\r"), "a;\nb;\n");
}

#[test]
fn multi_byte_characters_are_never_split() {
    let source = "const s = \"日本語のテキスト 🎉🎈\";\nconst ünïcödé = 1;\n";
    let formatted = format(source);
    assert!(formatted.contains("日本語のテキスト 🎉🎈"));
    assert!(formatted.is_char_boundary(formatted.len()));
    assert_token_preserving(source, &formatted);
}

#[test]
fn non_ascii_widths_are_measured_in_characters() {
    let config = config_with(|config| {
        config.line_width = 20;
    });
    let formatted = format_with("f(\"日本語日本語日本語\", \"日本語日本語\");\n", &config);
    assert!(formatted.contains('\n'));
    assert_token_preserving("f(\"日本語日本語日本語\", \"日本語日本語\");\n", &formatted);
}

#[test]
fn line_endings_are_normalized_before_lexing() {
    assert_eq!(normalize_line_endings("a\nb"), "a\nb");
    assert_eq!(normalize_line_endings("a\r\nb"), "a\nb");
    assert_eq!(normalize_line_endings("a\rb"), "a\nb");
    assert_eq!(normalize_line_endings("a\r\n\r\nb"), "a\n\nb");
}

#[test]
fn the_byte_order_mark_is_split_off_only_at_the_start() {
    assert_eq!(split_bom("\u{feff}x"), ("\u{feff}", "x"));
    assert_eq!(split_bom("x\u{feff}"), ("", "x\u{feff}"));
}
