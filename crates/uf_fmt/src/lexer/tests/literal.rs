//! Strings, templates, numbers and identifiers, including the forms that run off
//! the end of the input.

use super::{kinds, significant};
use crate::lexer::{Punctuator, TokenKind, Unterminated};

#[test]
fn strings_handle_escapes_and_line_continuations() {
    assert_eq!(significant("'it\\'s'"), vec![TokenKind::String]);
    assert_eq!(significant("\"a\\\"b\""), vec![TokenKind::String]);
    assert_eq!(significant("'a\\\nb'"), vec![TokenKind::String]);
}

#[test]
fn raw_newline_terminates_a_string_literal() {
    assert_eq!(
        kinds("'oops\nx"),
        vec![
            TokenKind::Unterminated(Unterminated::String),
            TokenKind::Newline,
            TokenKind::Identifier
        ]
    );
}

#[test]
fn templates_track_nested_interpolations() {
    assert_eq!(
        significant("`a${`b${c}d`}e`"),
        vec![
            TokenKind::TemplateHead,
            TokenKind::TemplateHead,
            TokenKind::Identifier,
            TokenKind::TemplateTail,
            TokenKind::TemplateTail,
        ]
    );
}

#[test]
fn template_middle_is_emitted_between_interpolations() {
    assert_eq!(
        significant("`${a}-${b}`"),
        vec![
            TokenKind::TemplateHead,
            TokenKind::Identifier,
            TokenKind::TemplateMiddle,
            TokenKind::Identifier,
            TokenKind::TemplateTail,
        ]
    );
}

#[test]
fn object_literals_inside_interpolations_do_not_end_the_template() {
    assert_eq!(
        significant("`${ {a: 1} }`"),
        vec![
            TokenKind::TemplateHead,
            TokenKind::Punctuator(Punctuator::OpenBrace),
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Colon),
            TokenKind::Number,
            TokenKind::Punctuator(Punctuator::CloseBrace),
            TokenKind::TemplateTail,
        ]
    );
}

#[test]
fn unterminated_template_is_reported() {
    assert_eq!(
        kinds("`abc"),
        vec![TokenKind::Unterminated(Unterminated::Template)]
    );
}
#[test]
fn numbers_cover_every_literal_form() {
    for source in [
        "0x1F",
        "0XFF",
        "0b1010",
        "0o777",
        "1_000_000",
        "1e10",
        "1E-10",
        "1.5",
        ".5",
        "1n",
        "0.5e+3",
    ] {
        assert_eq!(significant(source), vec![TokenKind::Number], "{source}");
    }
}

#[test]
fn a_number_does_not_swallow_a_following_member_access() {
    assert_eq!(
        significant("1..toString()"),
        vec![
            TokenKind::Number,
            TokenKind::Punctuator(Punctuator::Dot),
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::OpenParen),
            TokenKind::Punctuator(Punctuator::CloseParen),
        ]
    );
}

#[test]
fn unicode_identifiers_are_single_tokens() {
    assert_eq!(significant("const café = 1;")[1], TokenKind::Identifier);
    assert_eq!(significant("日本語")[0], TokenKind::Identifier);
}

#[test]
fn private_names_are_lexed_as_one_token() {
    assert_eq!(significant("this.#count")[2], TokenKind::PrivateName);
}
