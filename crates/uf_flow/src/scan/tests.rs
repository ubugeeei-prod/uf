//! The scanner's two modes, and the boundary between them.

use super::jsx::MAX_JSX_DEPTH;
use super::*;

/// The kinds a source lexes to, as a compact string for readable assertions.
fn kinds(tokens: &[Token], source: &str) -> Vec<String> {
    tokens
        .iter()
        .map(|token| match token.kind {
            TokenKind::Ident => format!("id:{}", token.text(source)),
            TokenKind::String => format!("str:{}", token.text(source)),
            TokenKind::Template => String::from("tpl"),
            TokenKind::Number => format!("num:{}", token.text(source)),
            TokenKind::Regex => String::from("re"),
            TokenKind::Arrow => String::from("=>"),
            TokenKind::Punct(byte) => format!("{}", byte as char),
            TokenKind::JsxText => format!("text:{:?}", token.text(source)),
            TokenKind::JsxTagOpen => String::from("jsx<"),
            TokenKind::JsxTagClose => String::from("jsx>"),
            TokenKind::Invalid => String::from("invalid"),
        })
        .collect()
}

fn jsx_texts(source: &str) -> Vec<String> {
    tokenize_jsx(source)
        .into_iter()
        .filter(|token| token.kind == TokenKind::JsxText)
        .map(|token| token.text(source).to_string())
        .collect()
}

#[test]
fn plain_javascript_lexes_the_same_in_both_modes() {
    let source = "const a = 1 < 2;\nconst b = f(x) < y;\n";

    assert_eq!(
        kinds(&tokenize(source), source),
        kinds(&tokenize_jsx(source), source)
    );
}

#[test]
fn an_element_start_is_recognized_after_an_equals_sign() {
    let source = "const a = <div>hi</div>;\n";

    assert_eq!(jsx_texts(source), vec!["hi"]);
}

#[test]
fn a_comparison_is_not_an_element_start() {
    let source = "const ok = count < limit;\n";

    assert!(jsx_texts(source).is_empty());
}

#[test]
fn a_comparison_after_a_call_is_not_an_element_start() {
    let source = "const ok = size(a) < limit;\n";

    assert!(jsx_texts(source).is_empty());
}

#[test]
fn a_comparison_after_an_index_is_not_an_element_start() {
    let source = "const ok = list[0] < limit;\n";

    assert!(jsx_texts(source).is_empty());
}

#[test]
fn an_element_is_recognized_after_return() {
    let source = "function f() {\n  return <p>text</p>;\n}\n";

    assert_eq!(jsx_texts(source), vec!["text"]);
}

#[test]
fn an_apostrophe_in_text_does_not_start_a_string() {
    let source = "const a = <p>it's fine</p>;\nconst b = 1;\n";

    assert_eq!(jsx_texts(source), vec!["it's fine"]);
    let tokens = tokenize_jsx(source);
    assert!(tokens.iter().all(|token| token.kind != TokenKind::Invalid));
    assert!(kinds(&tokens, source).contains(&String::from("id:b")));
}

#[test]
fn a_quote_in_text_does_not_start_a_string() {
    let source = "const a = <p>say \"hi\" now</p>;\nconst b = 2;\n";

    assert_eq!(jsx_texts(source), vec!["say \"hi\" now"]);
    assert!(kinds(&tokenize_jsx(source), source).contains(&String::from("num:2")));
}

#[test]
fn a_slash_in_text_is_not_a_regular_expression() {
    let source = "const a = <p>a / b / c</p>;\nconst b = 3;\n";

    assert_eq!(jsx_texts(source), vec!["a / b / c"]);
    assert!(kinds(&tokenize_jsx(source), source).contains(&String::from("num:3")));
}

#[test]
fn a_closing_tag_ends_the_children_mode() {
    let source = "const a = <p>x</p> + 1;\n";

    let kinds = kinds(&tokenize_jsx(source), source);
    assert!(kinds.contains(&String::from("+")), "{kinds:?}");
    assert!(kinds.contains(&String::from("num:1")), "{kinds:?}");
}

#[test]
fn a_self_closing_child_does_not_end_its_parents_children() {
    let source = "const a = <div>one<br />two</div>;\n";

    assert_eq!(jsx_texts(source), vec!["one", "two"]);
}

#[test]
fn nested_elements_nest_their_text() {
    let source = "const a = <div>outer<span>inner</span>tail</div>;\n";

    assert_eq!(jsx_texts(source), vec!["outer", "inner", "tail"]);
}

#[test]
fn an_expression_container_returns_to_javascript() {
    let source = "const a = <p>{value < limit}</p>;\n";

    assert!(jsx_texts(source).is_empty());
    let kinds = kinds(&tokenize_jsx(source), source);
    assert!(kinds.contains(&String::from("id:value")), "{kinds:?}");
    assert!(kinds.contains(&String::from("id:limit")), "{kinds:?}");
}

#[test]
fn an_object_literal_inside_a_container_closes_correctly() {
    let source = "const a = <p style={{ color: \"red\" }}>x</p>;\n";

    assert_eq!(jsx_texts(source), vec!["x"]);
}

#[test]
fn an_element_inside_a_container_nests() {
    let source = "const a = <ul>{items.map((i) => <li>{i}</li>)}</ul>;\nconst b = 4;\n";

    assert!(kinds(&tokenize_jsx(source), source).contains(&String::from("num:4")));
}

#[test]
fn an_attribute_string_may_hold_the_other_quote() {
    let source = "const a = <p title='say \"hi\"'>x</p>;\nconst b = 5;\n";

    let tokens = tokenize_jsx(source);
    assert!(tokens.iter().all(|token| token.kind != TokenKind::Invalid));
    assert!(kinds(&tokens, source).contains(&String::from("num:5")));
}

#[test]
fn a_fragment_lexes_as_children() {
    let source = "const a = <>one<b>two</b></>;\n";

    assert_eq!(jsx_texts(source), vec!["one", "two"]);
}

#[test]
fn text_keeps_its_newlines() {
    let source = "const a = (\n  <p>\n    hello\n  </p>\n);\n";

    assert_eq!(jsx_texts(source), vec!["\n    hello\n  "]);
}

#[test]
fn an_unterminated_element_does_not_loop() {
    let source = "const a = <div>never closed\n";

    let tokens = tokenize_jsx(source);

    assert!(!tokens.is_empty());
}

#[test]
fn deeply_nested_jsx_stays_bounded() {
    let source = format!(
        "const a = {}{};\n",
        "<b>".repeat(MAX_JSX_DEPTH * 2),
        "</b>".repeat(MAX_JSX_DEPTH * 2)
    );

    let tokens = tokenize_jsx(&source);

    assert!(!tokens.is_empty());
}

#[test]
fn flow_type_parameters_stay_punctuation_in_the_plain_mode() {
    let source = "type F = <T>(value: T) => T;\n";

    let kinds = kinds(&tokenize(source), source);

    assert!(kinds.contains(&String::from("<")), "{kinds:?}");
    assert!(
        !kinds.iter().any(|kind| kind.starts_with("text:")),
        "{kinds:?}"
    );
}

#[test]
fn a_jsx_source_with_no_elements_produces_no_text_tokens() {
    let source = "export const a = 1;\nexport function f() {\n  return a;\n}\n";

    assert!(jsx_texts(source).is_empty());
    assert!(
        tokenize_jsx(source)
            .iter()
            .all(|token| !token.kind.is_jsx())
    );
}

#[test]
fn matching_close_finds_the_paired_bracket() {
    let source = "f({ a: { b: 1 } })";
    let tokens = tokenize(source);
    let open = tokens
        .iter()
        .position(|token| token.is_punct(b'{'))
        .expect("an open brace");

    let close = matching_close(&tokens, open, b'{', b'}').expect("a close brace");

    assert_eq!(tokens[close].start, source.rfind('}').expect("a brace"));
}

#[test]
fn matching_open_finds_the_paired_bracket() {
    let source = "f({ a: 1 })";
    let tokens = tokenize(source);
    let close = tokens
        .iter()
        .rposition(|token| token.is_punct(b'}'))
        .expect("a close brace");

    let open = matching_open(&tokens, close, b'{', b'}').expect("an open brace");

    assert_eq!(tokens[open].start, source.find('{').expect("a brace"));
}

#[test]
fn a_statement_starts_at_the_top_and_after_a_terminator() {
    let source = "const a = 1;\nb();\n";
    let tokens = tokenize(source);

    assert!(starts_statement(&tokens, 0));
    let b = tokens
        .iter()
        .position(|token| token.is_ident(source, "b"))
        .expect("b");
    assert!(starts_statement(&tokens, b));
}
