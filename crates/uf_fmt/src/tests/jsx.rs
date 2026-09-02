//! JSX, where whitespace between children is program text rather than trivia and
//! must survive formatting untouched.

use super::*;

#[test]
fn jsx_attributes_are_normalized_without_spaces_around_equals() {
    similar_asserts::assert_eq!(
        format("const a = <div className = \"x\" id={ y } />;\n"),
        "const a = <div className=\"x\" id={y} />;\n"
    );
}

#[test]
fn jsx_children_keep_their_significant_whitespace() {
    similar_asserts::assert_eq!(
        format("const a = <p>hello   world {name} !</p>;\n"),
        "const a = <p>hello   world {name} !</p>;\n"
    );
}

#[test]
fn jsx_children_are_indented_by_nesting_depth() {
    similar_asserts::assert_eq!(
        format("const a = (\n<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n);\n"),
        "const a = (\n  <ul>\n    <li>one</li>\n    <li>two</li>\n  </ul>\n);\n"
    );
}

#[test]
fn nested_jsx_expression_containers_are_formatted_as_javascript() {
    similar_asserts::assert_eq!(
        format("const a = <ul>{items.map((i)=>(<li key={i}>{i}</li>))}</ul>;\n"),
        "const a = <ul>{items.map((i) => (<li key={i}>{i}</li>))}</ul>;\n"
    );
}

#[test]
fn jsx_text_is_never_reflowed() {
    let source = "const a = (\n  <p>\n    a long line of prose that would otherwise be wrapped by a formatter\n  </p>\n);\n";
    similar_asserts::assert_eq!(format(source), source);
}
