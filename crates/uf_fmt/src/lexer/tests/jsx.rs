//! JSX tags, fragments, names and expression containers, and the two places a
//! `<` looks like JSX but is not.

use super::significant;
use crate::lexer::{Punctuator, TokenKind, tokenize};

#[test]
fn jsx_elements_produce_jsx_tokens() {
    assert_eq!(
        significant("<div a=\"1\">hi</div>"),
        vec![
            TokenKind::JsxOpenStart,
            TokenKind::JsxName,
            TokenKind::JsxName,
            TokenKind::Punctuator(Punctuator::Equal),
            TokenKind::JsxString,
            TokenKind::JsxTagEnd,
            TokenKind::JsxText,
            TokenKind::JsxCloseStart,
            TokenKind::JsxName,
            TokenKind::JsxTagEnd,
        ]
    );
}

#[test]
fn jsx_text_keeps_quotes_and_apostrophes_as_text() {
    let tokens = significant("<p>it's \"fine\"</p>");
    assert!(!tokens.contains(&TokenKind::String));
}

#[test]
fn jsx_expression_containers_return_to_javascript() {
    assert_eq!(
        significant("<p>{a / b}</p>"),
        vec![
            TokenKind::JsxOpenStart,
            TokenKind::JsxName,
            TokenKind::JsxTagEnd,
            TokenKind::Punctuator(Punctuator::OpenBrace),
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Slash),
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::CloseBrace),
            TokenKind::JsxCloseStart,
            TokenKind::JsxName,
            TokenKind::JsxTagEnd,
        ]
    );
}

#[test]
fn self_closing_jsx_tags_are_a_single_token() {
    assert_eq!(significant("<br />")[2], TokenKind::JsxSelfClose);
}

#[test]
fn jsx_fragments_open_and_close() {
    assert_eq!(
        significant("<><b>x</b></>"),
        vec![
            TokenKind::JsxOpenStart,
            TokenKind::JsxTagEnd,
            TokenKind::JsxOpenStart,
            TokenKind::JsxName,
            TokenKind::JsxTagEnd,
            TokenKind::JsxText,
            TokenKind::JsxCloseStart,
            TokenKind::JsxName,
            TokenKind::JsxTagEnd,
            TokenKind::JsxCloseStart,
            TokenKind::JsxTagEnd,
        ]
    );
}

#[test]
fn dashed_and_namespaced_jsx_names_stay_together() {
    let tokens = tokenize("<a data-x=\"1\" svg:y=\"2\" Foo.Bar />");
    let names: Vec<&str> = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::JsxName)
        .map(|token| token.text("<a data-x=\"1\" svg:y=\"2\" Foo.Bar />"))
        .collect();
    assert_eq!(names, vec!["a", "data-x", "svg:y", "Foo.Bar"]);
}

#[test]
fn less_than_after_an_identifier_is_not_jsx() {
    assert_eq!(
        significant("a < b"),
        vec![
            TokenKind::Identifier,
            TokenKind::Punctuator(Punctuator::Less),
            TokenKind::Identifier,
        ]
    );
}

#[test]
fn generic_function_types_in_type_aliases_are_not_jsx() {
    let tokens = significant("type F = <T>(value: T) => T;");
    assert!(!tokens.iter().any(|kind| kind.is_jsx()));
}

#[test]
fn jsx_after_an_arrow_is_recognized() {
    let tokens = significant("const A = () => <div />;");
    assert!(tokens.contains(&TokenKind::JsxOpenStart));
}
