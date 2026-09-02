//! Edge cases: encodings, ceilings, ambiguity, and things that are not JSX.

use super::{call, lower};
use crate::{JsxError, JsxOptions, MAX_SOURCE_BYTES, transform};

fn unchanged(source: &str) -> bool {
    transform(source, &JsxOptions::default())
        .expect("lowers")
        .is_unchanged()
}

#[test]
fn plain_javascript_is_returned_untouched() {
    let source = "export const a = 1;\nexport function f() {\n  return a;\n}\n";

    let transformed = transform(source, &JsxOptions::default()).expect("lowers");

    assert!(transformed.is_unchanged());
    assert_eq!(transformed.code, source);
}

#[test]
fn empty_input_is_untouched() {
    assert!(unchanged(""));
}

#[test]
fn a_comparison_is_not_lowered() {
    assert!(unchanged(
        "const ok = count < limit;\nconst also = a > b;\n"
    ));
}

#[test]
fn a_chain_of_comparisons_is_not_lowered() {
    assert!(unchanged("const ok = a < b && c > d;\n"));
}

#[test]
fn a_generic_call_is_not_lowered() {
    assert!(unchanged("const q = make(a, b);\nconst r = list[0] < 3;\n"));
}

#[test]
fn an_element_inside_a_string_is_not_lowered() {
    assert!(unchanged("const html = \"<div>text</div>\";\n"));
}

#[test]
fn an_element_inside_a_template_is_not_lowered() {
    assert!(unchanged("const html = `<div>${value}</div>`;\n"));
}

#[test]
fn an_element_inside_a_comment_is_not_lowered() {
    assert!(unchanged("// <div>text</div>\nconst a = 1;\n"));
}

#[test]
fn an_element_inside_a_block_comment_is_not_lowered() {
    assert!(unchanged("/* <div>text</div> */\nconst a = 1;\n"));
}

#[test]
fn a_regular_expression_holding_angle_brackets_is_not_lowered() {
    assert!(unchanged("const re = /<div>/g;\nconst a = 1;\n"));
}

#[test]
fn an_unterminated_element_is_left_exactly_as_written() {
    let source = "const a = <div>never closed;\n";

    let transformed = transform(source, &JsxOptions::default()).expect("lowers");

    assert_eq!(transformed.code, source);
}

#[test]
fn a_mismatched_closing_tag_still_lowers_the_shape_it_read() {
    // A lexer cannot check that `</span>` matches `<div>`; that is the type
    // checker's job. What matters here is that the output is still JavaScript.
    let out = call("const a = <div>x</span>;\n");

    assert!(out.contains("_jsx(\"div\", {children: \"x\"})"), "{out}");
}

#[test]
fn a_source_over_the_ceiling_is_refused() {
    let source = format!("const a = <div />;{}", " ".repeat(MAX_SOURCE_BYTES));

    let error = transform(&source, &JsxOptions::default()).expect_err("refused");

    assert!(matches!(error, JsxError::SourceTooLarge { .. }));
}

#[test]
fn too_many_elements_is_refused() {
    let options = JsxOptions {
        max_elements: 4,
        ..JsxOptions::default()
    };
    let source = format!("const a = [{}];\n", "<p />, ".repeat(8));

    let error = transform(&source, &options).expect_err("refused");

    assert!(matches!(error, JsxError::TooManyElements { limit: 4 }));
}

#[test]
fn exactly_the_element_ceiling_is_accepted() {
    let options = JsxOptions {
        max_elements: 3,
        ..JsxOptions::default()
    };
    let source = "const a = [<p />, <p />, <p />];\n";

    assert!(transform(source, &options).is_ok());
}

#[test]
fn crlf_sources_keep_their_line_endings() {
    let source = "// @flow\r\nconst a = (\r\n  <p>\r\n    hi\r\n  </p>\r\n);\r\n";

    let out = lower(source);

    assert_eq!(out.matches("\r\n").count(), source.matches("\r\n").count());
    assert!(out.contains("\"hi\""), "{out}");
}

#[test]
fn non_ascii_text_survives() {
    let out = call("const a = <p>caffè ☕</p>;\n");

    assert!(out.contains("\"caffè ☕\""), "{out}");
}

#[test]
fn a_non_ascii_attribute_value_survives() {
    let out = call("const a = <p title=\"caffè\" />;\n");

    assert!(out.contains("title: \"caffè\""), "{out}");
}

#[test]
fn lowering_is_idempotent() {
    let source = "const a = <div className=\"x\">{y}</div>;\n";

    let once = transform(source, &JsxOptions::default())
        .expect("lowers")
        .code;
    let twice = transform(&once, &JsxOptions::default())
        .expect("lowers")
        .code;

    assert_eq!(once, twice);
}

#[test]
fn a_large_module_lowers_without_changing_its_shape() {
    let unit = "export const a = <p className=\"row\">{value}</p>;\n";
    let source = unit.repeat(2_000);

    let transformed = transform(&source, &JsxOptions::default()).expect("lowers");

    assert_eq!(transformed.code.lines().count(), source.lines().count());
    assert_eq!(transformed.elements, 2_000);
    assert!(!transformed.code.contains("<p"), "output still holds JSX");
}

#[test]
fn deeply_nested_jsx_stays_bounded() {
    let source = format!("const a = {}x{};\n", "<b>".repeat(400), "</b>".repeat(400));

    // Past the parser's depth ceiling the element is left alone rather than
    // half-lowered, so this must terminate and must not panic.
    let transformed = transform(&source, &JsxOptions::default()).expect("lowers");

    assert_eq!(transformed.code.lines().count(), source.lines().count());
}

#[test]
fn a_module_of_only_whitespace_is_untouched() {
    assert!(unchanged("\n\n  \n"));
}

#[test]
fn an_angle_bracket_with_no_element_is_untouched() {
    assert!(unchanged("const a = 1 < 2 > 0;\n"));
}

#[test]
fn text_holding_a_brace_is_split_at_the_container() {
    let out = call("const a = <p>a{b}c</p>;\n");

    assert!(out.contains("[\"a\", b, \"c\",]"), "{out}");
}

#[test]
fn an_element_after_a_lowered_one_on_the_same_line_is_lowered_too() {
    let out = call("const a = [<p>1</p>, <p>2</p>, <p>3</p>];\n");

    assert_eq!(out.matches("_jsx(\"p\"").count(), 3, "{out}");
}

#[test]
fn lowering_never_leaves_a_jsx_token_behind() {
    let sources = [
        "const a = <div />;\n",
        "const a = <>{x}</>;\n",
        "const a = <ul>{items.map((i) => <li key={i}>{i}</li>)}</ul>;\n",
        "const a = <A.B c={<D />} {...rest}>text</A.B>;\n",
    ];

    for source in sources {
        let code = transform(source, &JsxOptions::default())
            .expect("lowers")
            .code;
        assert!(
            !uf_flow::scan::tokenize_jsx(&code)
                .iter()
                .any(|token| token.kind.is_jsx()),
            "JSX survived: {code}"
        );
    }
}
