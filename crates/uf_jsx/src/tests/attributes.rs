//! Attributes: values, spreads, quoting, entities, and `key` extraction.

use super::{call, expression, lower};

#[test]
fn an_expression_attribute_becomes_a_property() {
    assert_eq!(
        expression("<div onClick={f} />"),
        "_jsx(\"div\", {onClick: f,})"
    );
}

#[test]
fn a_string_attribute_becomes_a_string_property() {
    assert_eq!(
        expression("<div className=\"card\" />"),
        "_jsx(\"div\", {className: \"card\",})"
    );
}

#[test]
fn a_single_quoted_attribute_becomes_a_double_quoted_property() {
    assert_eq!(
        expression("<div className='card' />"),
        "_jsx(\"div\", {className: \"card\",})"
    );
}

#[test]
fn an_attribute_with_no_value_is_true() {
    assert_eq!(
        expression("<input disabled />"),
        "_jsx(\"input\", {disabled: true,})"
    );
}

#[test]
fn several_attributes_are_separated_by_commas() {
    assert_eq!(
        expression("<div a={1} b=\"two\" c />"),
        "_jsx(\"div\", {a: 1, b: \"two\", c: true,})"
    );
}

#[test]
fn a_dashed_attribute_name_is_quoted() {
    assert_eq!(
        expression("<div data-id=\"7\" />"),
        "_jsx(\"div\", {\"data-id\": \"7\",})"
    );
}

#[test]
fn a_namespaced_attribute_name_is_quoted() {
    assert_eq!(
        expression("<use xlink:href=\"#a\" />"),
        "_jsx(\"use\", {\"xlink:href\": \"#a\",})"
    );
}

#[test]
fn a_spread_attribute_becomes_an_object_spread() {
    assert_eq!(expression("<div {...rest} />"), "_jsx(\"div\", {...rest,})");
}

#[test]
fn a_spread_attribute_mixes_with_named_ones() {
    assert_eq!(
        expression("<div {...rest} id=\"a\" />"),
        "_jsx(\"div\", {...rest, id: \"a\",})"
    );
}

#[test]
fn a_later_named_attribute_still_wins_over_a_spread() {
    let out = expression("<div id=\"a\" {...rest} />");

    assert_eq!(out, "_jsx(\"div\", {id: \"a\", ...rest,})");
}

#[test]
fn a_spread_of_a_call_keeps_the_call() {
    assert_eq!(
        expression("<main {...stylex.props(styles.shell)} />"),
        "_jsx(\"main\", {...stylex.props(styles.shell),})"
    );
}

#[test]
fn an_object_literal_attribute_value_survives() {
    assert_eq!(
        expression("<div style={{ color: \"red\" }} />"),
        "_jsx(\"div\", {style: {color: \"red\"},})"
    );
}

#[test]
fn a_key_moves_out_of_the_props_and_behind_them() {
    assert_eq!(
        expression("<li key={item.id}>x</li>"),
        "_jsx(\"li\", {children: \"x\"}, item.id)"
    );
}

#[test]
fn a_key_on_a_self_closing_element_moves_too() {
    assert_eq!(expression("<Row key={id} />"), "_jsx(Row, {}, id)");
}

#[test]
fn a_key_keeps_the_other_attributes_in_the_props() {
    assert_eq!(
        expression("<li key={id} className=\"row\">x</li>"),
        "_jsx(\"li\", {className: \"row\", children: \"x\"}, id)"
    );
}

#[test]
fn a_string_key_is_still_extracted() {
    assert_eq!(expression("<li key=\"a\" />"), "_jsx(\"li\", {}, \"a\")");
}

#[test]
fn a_key_on_a_list_element_moves_after_the_children() {
    assert_eq!(
        expression("<li key={id}>{a}{b}</li>"),
        "_jsxs(\"li\", {children: [a, b,]}, id)"
    );
}

#[test]
fn a_multi_line_key_expression_keeps_the_line_count() {
    let source = "const a = (\n  <li\n    key={\n      item.id\n    }\n  >\n    x\n  </li>\n);\n";

    let out = lower(source);

    assert_eq!(out.lines().count(), source.lines().count());
    assert!(out.contains("item.id"), "{out}");
}

#[test]
fn a_key_named_property_on_a_fragment_is_left_alone() {
    // Fragments take no props at all, so there is nothing to extract from.
    let out = expression("<>{a}</>");

    assert_eq!(out, "_jsx(_Fragment, {children: a})");
}

#[test]
fn entities_in_a_string_attribute_are_decoded() {
    assert_eq!(
        expression("<a title=\"a &amp; b\" />"),
        "_jsx(\"a\", {title: \"a & b\",})"
    );
}

#[test]
fn a_numeric_entity_in_an_attribute_is_decoded() {
    assert_eq!(
        expression("<a title=\"&#65;&#x42;\" />"),
        "_jsx(\"a\", {title: \"AB\",})"
    );
}

#[test]
fn an_unknown_entity_in_an_attribute_is_left_alone() {
    assert_eq!(
        expression("<a title=\"&hellip;\" />"),
        "_jsx(\"a\", {title: \"&hellip;\",})"
    );
}

#[test]
fn a_quote_inside_a_single_quoted_attribute_is_escaped() {
    let out = expression("<a title='say \"hi\"' />");

    assert_eq!(out, "_jsx(\"a\", {title: \"say \\\"hi\\\"\",})");
}

#[test]
fn an_attribute_holding_an_arrow_function_survives() {
    assert_eq!(
        expression("<button onClick={() => run(1)} />"),
        "_jsx(\"button\", {onClick: () => run(1),})"
    );
}

#[test]
fn attributes_spread_over_lines_keep_their_lines() {
    let source = "const a = (\n  <div\n    className=\"a\"\n    onClick={f}\n  />\n);\n";

    let out = lower(source);

    assert_eq!(out.lines().count(), source.lines().count());
    assert!(out.contains("className: \"a\""), "{out}");
    assert!(out.contains("onClick: f"), "{out}");
}

#[test]
fn a_ref_attribute_stays_an_ordinary_prop() {
    // React 19 passes `ref` through props; only `key` is special.
    assert_eq!(
        expression("<input ref={r} />"),
        "_jsx(\"input\", {ref: r,})"
    );
}

#[test]
fn an_attribute_named_children_is_overridden_by_real_children() {
    // Last property wins in an object literal, and `children:` is emitted last.
    let out = expression("<div children={a}>b</div>");

    assert!(out.ends_with("children: \"b\"})"), "{out}");
}

#[test]
fn a_component_attribute_holding_jsx_is_lowered() {
    let out = call("const a = <Layout header={<h1>t</h1>} />;\n");

    assert!(
        out.contains("header: _jsx(\"h1\", {children: \"t\"})"),
        "{out}"
    );
}
