//! Telling a regular expression from a division, which is decided entirely by
//! what the previous significant token was.

use super::{kinds, significant};
use crate::lexer::{Punctuator, TokenKind, Unterminated};

#[test]
fn regex_is_recognized_at_the_start_of_an_expression() {
    assert_eq!(
        significant("x = /a\\/b/g"),
        vec![
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Equal),
            TokenKind::Regex,
        ]
    );
}

#[test]
fn slash_after_an_identifier_is_division() {
    assert_eq!(
        significant("a / b / c"),
        vec![
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Slash),
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Slash),
            TokenKind::Identifier,
        ]
    );
}

#[test]
fn slash_after_a_call_is_division_but_after_a_statement_header_is_regex() {
    assert!(significant("f() / 2").contains(&TokenKind::Punctuator(Punctuator::Slash)));
    assert!(!significant("f() / 2").contains(&TokenKind::Regex));
    assert!(significant("if (a) /re/.test(b)").contains(&TokenKind::Regex));
    assert!(significant("while (a) /re/.test(b)").contains(&TokenKind::Regex));
}

#[test]
fn reserved_words_after_a_dot_are_property_names() {
    assert_eq!(
        significant("promise.catch(f)"),
        vec![
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Dot),
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::OpenParen),
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::CloseParen),
        ]
    );
    assert_eq!(significant("a?.default")[2], TokenKind::Identifier);
}

#[test]
fn slash_after_an_object_literal_is_division() {
    let tokens = significant("const x = {a: 1} / 2;");
    assert!(tokens.contains(&TokenKind::Punctuator(Punctuator::Slash)));
    assert!(!tokens.contains(&TokenKind::Regex));
}

#[test]
fn a_brace_after_a_return_type_is_a_block() {
    let source = "function f(): Promise<void> {}\n/re/.test(x);";
    // The block classification is what lets the following `/` start a regex.
    assert!(significant(source).contains(&TokenKind::Regex));
}

#[test]
fn slash_after_a_block_is_a_regex() {
    let tokens = significant("function f() {}\n/re/.test(x);");
    assert!(tokens.contains(&TokenKind::Regex));
}

#[test]
fn regex_character_class_may_contain_a_slash() {
    assert_eq!(
        significant("x = /[/]/"),
        vec![
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Equal),
            TokenKind::Regex,
        ]
    );
}

#[test]
fn regex_with_a_newline_before_its_close_is_unterminated() {
    assert_eq!(
        kinds("x = /ab\n")[4],
        TokenKind::Unterminated(Unterminated::Regex)
    );
}

#[test]
fn keywords_after_which_a_regex_may_start() {
    assert!(significant("return /re/;").contains(&TokenKind::Regex));
    assert!(significant("typeof /re/;").contains(&TokenKind::Regex));
    assert!(significant("case /re/:").contains(&TokenKind::Regex));
}

#[test]
fn contextual_keywords_do_not_start_a_regex() {
    // `type` is a plain identifier here, so `/` must stay division.
    let tokens = significant("const type = 4; const half = type / 2;");
    assert!(!tokens.contains(&TokenKind::Regex));
}
