//! Element shapes: names, fragments, self-closing tags, nesting.

use super::{call, expression, lower};
use crate::element_type;

#[test]
fn a_host_element_becomes_a_string_type() {
    assert_eq!(expression("<div />"), "_jsx(\"div\", {})");
}

#[test]
fn a_component_element_keeps_its_identifier() {
    assert_eq!(expression("<Counter />"), "_jsx(Counter, {})");
}

#[test]
fn a_member_element_keeps_its_member_expression() {
    assert_eq!(expression("<Form.Root />"), "_jsx(Form.Root, {})");
}

#[test]
fn a_deep_member_element_keeps_every_segment() {
    assert_eq!(expression("<A.B.C />"), "_jsx(A.B.C, {})");
}

#[test]
fn a_custom_element_becomes_a_string_type() {
    assert_eq!(expression("<my-widget />"), "_jsx(\"my-widget\", {})");
}

#[test]
fn a_namespaced_element_becomes_a_string_type() {
    assert_eq!(expression("<svg:circle />"), "_jsx(\"svg:circle\", {})");
}

#[test]
fn an_element_with_no_children_takes_an_empty_props_object() {
    assert_eq!(expression("<div></div>"), "_jsx(\"div\", {})");
}

#[test]
fn an_element_with_one_child_uses_the_single_form() {
    assert_eq!(
        expression("<div>text</div>"),
        "_jsx(\"div\", {children: \"text\"})"
    );
}

#[test]
fn an_element_with_two_children_uses_the_list_form() {
    assert_eq!(
        expression("<div>{a}{b}</div>"),
        "_jsxs(\"div\", {children: [a, b,]})"
    );
}

#[test]
fn a_fragment_lowers_to_the_fragment_helper() {
    assert_eq!(expression("<>{a}</>"), "_jsx(_Fragment, {children: a})");
}

#[test]
fn an_empty_fragment_lowers_to_an_empty_call() {
    assert_eq!(expression("<></>"), "_jsx(_Fragment, {})");
}

#[test]
fn a_fragment_with_two_children_uses_the_list_form() {
    assert_eq!(
        expression("<>{a}{b}</>"),
        "_jsxs(_Fragment, {children: [a, b,]})"
    );
}

#[test]
fn nested_fragments_lower_independently() {
    assert_eq!(
        expression("<><><span /></></>"),
        "_jsx(_Fragment, {children: _jsx(_Fragment, {children: _jsx(\"span\", {})})})"
    );
}

#[test]
fn nested_elements_lower_from_the_inside_out() {
    assert_eq!(
        expression("<div><span>x</span></div>"),
        "_jsx(\"div\", {children: _jsx(\"span\", {children: \"x\"})})"
    );
}

#[test]
fn a_self_closing_child_is_still_a_child() {
    assert_eq!(
        expression("<div><br /></div>"),
        "_jsx(\"div\", {children: _jsx(\"br\", {})})"
    );
}

#[test]
fn sibling_elements_are_separate_children() {
    assert_eq!(
        expression("<ul><li>a</li><li>b</li></ul>"),
        "_jsxs(\"ul\", {children: [_jsx(\"li\", {children: \"a\"}), _jsx(\"li\", {children: \"b\"}),]})"
    );
}

#[test]
fn an_element_inside_a_container_is_lowered_too() {
    let out = call("const a = <ul>{items.map((i) => <li>{i}</li>)}</ul>;\n");

    assert!(
        out.contains("items.map((i) => _jsx(\"li\", {children: i}))"),
        "{out}"
    );
}

#[test]
fn an_element_in_an_attribute_value_is_lowered_too() {
    assert_eq!(
        expression("<Dialog body={<p>hi</p>} />"),
        "_jsx(Dialog, {body: _jsx(\"p\", {children: \"hi\"}),})"
    );
}

#[test]
fn two_top_level_elements_are_both_lowered() {
    let out = call("const a = <p>one</p>;\nconst b = <p>two</p>;\n");

    assert!(out.contains("_jsx(\"p\", {children: \"one\"})"), "{out}");
    assert!(out.contains("_jsx(\"p\", {children: \"two\"})"), "{out}");
}

#[test]
fn a_multi_line_element_keeps_its_lines() {
    let source = "const a = (\n  <main>\n    <h1>title</h1>\n    <p>body</p>\n  </main>\n);\n";

    let out = lower(source);

    assert_eq!(out.lines().count(), source.lines().count());
    assert!(out.contains("_jsxs(\"main\", {"), "{out}");
}

#[test]
fn element_type_classifies_names() {
    assert_eq!(element_type("div"), "\"div\"");
    assert_eq!(element_type("Counter"), "Counter");
    assert_eq!(element_type("Form.Root"), "Form.Root");
    assert_eq!(element_type("my-widget"), "\"my-widget\"");
    assert_eq!(element_type("svg:circle"), "\"svg:circle\"");
    assert_eq!(element_type("_private"), "_private");
}

#[test]
fn an_element_returned_from_a_function_is_lowered() {
    let out = call("function f() {\n  return <p>x</p>;\n}\n");

    assert!(
        out.contains("return _jsx(\"p\", {children: \"x\"});"),
        "{out}"
    );
}

#[test]
fn an_element_in_a_ternary_is_lowered_on_both_arms() {
    let out = expression("<div>{ok ? <a /> : <b />}</div>");

    assert!(
        out.contains("ok ? _jsx(\"a\", {}) : _jsx(\"b\", {})"),
        "{out}"
    );
}

#[test]
fn an_element_in_an_array_literal_is_lowered() {
    let out = call("const a = [<p>one</p>, <p>two</p>];\n");

    assert!(
        out.contains("[_jsx(\"p\", {children: \"one\"}), _jsx(\"p\", {children: \"two\"})]"),
        "{out}"
    );
}
