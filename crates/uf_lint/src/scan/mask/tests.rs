//! What gets blanked, what does not, and the invariant everything else rests on.

use super::*;

/// The one property every offset in the crate depends on.
fn assert_same_shape(source: &str, masked: &str) {
    assert_eq!(source.len(), masked.len(), "the length moved");
    assert_eq!(
        source.lines().count(),
        masked.lines().count(),
        "the line count moved"
    );
    for (before, after) in source.char_indices().zip(masked.char_indices()) {
        assert_eq!(before.0, after.0, "a byte offset moved");
    }
}

fn mask(source: &str) -> String {
    let masked = mask_inline_comments(source).into_owned();
    assert_same_shape(source, &masked);
    masked
}

/// ubugeeei-prod/uf#476: the `}` after the comment used to be dropped, and the
/// brace stack never came back down.
#[test]
fn a_jsx_comment_leaves_the_braces_around_it() {
    assert_eq!(
        mask("      {/* a comment */}\n"),
        "      {               }\n"
    );
}

#[test]
fn code_on_both_sides_of_an_inline_comment_survives() {
    assert_eq!(
        mask("const a = /* why */ any;\n"),
        "const a =           any;\n"
    );
}

#[test]
fn several_on_one_line_are_all_blanked() {
    assert_eq!(mask("a(/*x*/, /*y*/);\n"), "a(     ,      );\n");
}

/// The last one on the line has nothing after it but `);`, which is code, so it
/// is blanked too — the rule is "something follows", not "a statement follows".
#[test]
fn the_something_that_follows_can_be_punctuation() {
    assert_eq!(mask("a(/*x*/);\n"), "a(     );\n");
}

/// `scan_line` already carries a multi-line comment from line to line, and
/// blanking one would be a slower way to the same answer.
/// A `/** … */` above a declaration has no code after it to rescue, and
/// blanking it would leave a line of spaces that
/// `uniflowed/no-trailing-whitespace` reports — 1,556 of them in this
/// repository.
#[test]
fn a_comment_with_nothing_after_it_is_left_alone() {
    for source in [
        "/** A page in the manual. */\nexport const a = 1;\n",
        "  /* indented, and alone on its line */\n",
        "const a = 1; /* a note after the statement */\n",
    ] {
        assert_eq!(mask(source), source, "{source}");
    }
}

#[test]
fn a_comment_that_spans_lines_is_left_to_the_line_scanner() {
    let source = "/**\n * a header\n */\nexport const a = 1;\n";
    assert_eq!(mask(source), source);
}

/// The direction that matters: never blank something that is not a comment.
#[test]
fn a_block_opener_inside_a_string_is_not_a_comment() {
    for source in [
        "const s = \"a /* b */ c\";\n",
        "const s = 'a /* b */ c';\n",
        "const s = `a /* b */ c`;\n",
        "const s = \"a /* b\";\nconst t = \"c */ d\";\n",
    ] {
        assert_eq!(mask(source), source, "{source}");
    }
}

#[test]
fn an_escaped_quote_does_not_end_the_string() {
    let source = "const s = \"a \\\" /* b */ c\";\n";
    assert_eq!(mask(source), source);
}

/// A template literal is the one string that crosses lines, so a `/*` on the
/// line after it opened is still inside it.
#[test]
fn a_template_literal_keeps_its_comment_looking_text_across_lines() {
    let source = "const s = `\n  /* not a comment */\n`;\n";
    assert_eq!(mask(source), source);
}

/// An unterminated `'` or `"` is a syntax error, and reading the rest of the
/// file as a string would blank nothing after it at all.
#[test]
fn an_unterminated_quote_ends_at_the_newline() {
    assert_eq!(
        mask("const s = \"oops;\nconst a = /* x */ 1;\n"),
        "const s = \"oops;\nconst a =         1;\n"
    );
}

#[test]
fn a_line_comment_hides_a_block_opener_after_it() {
    let source = "const a = 1; // /* not opened\nconst b = 2;\n";
    assert_eq!(mask(source), source);
}

/// The common case allocates nothing.
#[test]
fn a_file_with_no_block_comment_is_borrowed() {
    assert!(matches!(
        mask_inline_comments("export const a = 1;\n"),
        Cow::Borrowed(_)
    ));
    assert!(matches!(
        mask_inline_comments("// a line comment\nexport const a = 1;\n"),
        Cow::Borrowed(_)
    ));
}

/// Multi-byte text keeps its offsets, which is what every diagnostic span is
/// measured in.
#[test]
fn a_non_ascii_line_keeps_every_offset() {
    let source = "const s = \"日本語\"; /* c */ const t = 1;\n";
    let masked = mask(source);
    assert!(masked.contains("日本語"));
    assert!(!masked.contains("/* c */"));
}
