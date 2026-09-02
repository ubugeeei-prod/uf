//! String delimiters: normalizing them to the configured style, and the literals
//! that keep the quotes the author chose because converting would cost an escape.

use super::*;

#[test]
fn quotes_are_normalized_to_the_configured_style() {
    similar_asserts::assert_eq!(format("const s = 'a';\n"), "const s = \"a\";\n");
    let single = config_with(|config| {
        config.quotes = QuoteStyle::Single;
    });
    similar_asserts::assert_eq!(
        format_with("const s = \"a\";\n", &single),
        "const s = 'a';\n"
    );
}

#[test]
fn quotes_are_left_alone_when_converting_would_add_escapes() {
    similar_asserts::assert_eq!(
        format("const s = 'say \"hi\"';\n"),
        "const s = 'say \"hi\"';\n"
    );
}

#[test]
fn template_literals_are_never_requoted() {
    similar_asserts::assert_eq!(
        format("const t = `a 'b' \"c\"`;\n"),
        "const t = `a 'b' \"c\"`;\n"
    );
}

#[test]
fn directives_keep_their_meaning() {
    similar_asserts::assert_eq!(format("'use client';\nf();\n"), "\"use client\";\nf();\n");
    similar_asserts::assert_eq!(format("\"use server\"\nf()\n"), "\"use server\";\nf();\n");
}
