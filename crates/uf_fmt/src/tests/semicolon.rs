//! Adding and removing statement terminators, which is the one edit the printer
//! makes that could change what a program means if the rule were wrong.

use super::*;

#[test]
fn missing_statement_semicolons_are_added() {
    similar_asserts::assert_eq!(
        format("const x = 1\nconst y = 2\n"),
        "const x = 1;\nconst y = 2;\n"
    );
    similar_asserts::assert_eq!(
        format("function f() {\n  return 1\n}\n"),
        "function f() {\n  return 1;\n}\n"
    );
}

#[test]
fn semicolons_are_not_added_where_the_next_line_continues_the_expression() {
    // `a\n[0]` is a single index expression: a semicolon would change it.
    similar_asserts::assert_eq!(format("const x = a\n[0].b()\n"), "const x = a\n[0].b();\n");
    similar_asserts::assert_eq!(format("const x = a\n(0)\n"), "const x = a\n(0);\n");
    similar_asserts::assert_eq!(format("const x = a\n+ b\n"), "const x = a\n+ b;\n");
}

#[test]
fn semicolons_are_not_added_inside_object_literals_or_argument_lists() {
    similar_asserts::assert_eq!(
        format("const o = {\n  a: 1,\n  b: 2\n};\n"),
        "const o = {\n  a: 1,\n  b: 2\n};\n"
    );
    similar_asserts::assert_eq!(format("f(\n  a,\n  b\n);\n"), "f(\n  a,\n  b\n);\n");
}

#[test]
fn semicolons_are_not_added_after_a_trailing_comment() {
    similar_asserts::assert_eq!(
        format("const x = 1 // note\nconst y = 2;\n"),
        "const x = 1 // note\nconst y = 2;\n"
    );
}

#[test]
fn semicolons_can_be_removed() {
    let config = config_with(|config| {
        config.semicolons = false;
    });
    similar_asserts::assert_eq!(
        format_with("const x = 1;\nconst y = 2;\n", &config),
        "const x = 1\nconst y = 2\n"
    );
    similar_asserts::assert_eq!(
        format_with("for (let i = 0; i < 3; i++) {}\n", &config),
        "for (let i = 0; i < 3; i++) {}\n"
    );
}

#[test]
fn an_empty_statement_after_a_header_is_never_dropped() {
    let config = config_with(|config| {
        config.semicolons = false;
    });
    similar_asserts::assert_eq!(format_with("while (f());\n", &config), "while (f());\n");
}

/// A body brace after a tuple or array return type is a block, not an object.
///
/// The formatter used to read the `{` in
/// `hook useX(): [string, () => void] {` as an object literal, because the
/// token before it is `]`, and then emitted a `;` after the closing brace —
/// a token the input never had. Two golden fixtures had the extra semicolon
/// baked in as expected output, which is how it survived: a fixture that
/// records a bug turns the bug into the specification.
#[test]
fn a_body_after_a_tuple_return_type_gains_no_semicolon() {
    for source in [
        r#"// @flow
hook useX(): [string, (next: string) => void] {
  return ["", () => {}];
}
"#,
        r#"// @flow
function pair(): [number, number] {
  return [1, 2];
}
"#,
        r#"// @flow
function rows(): Array<[string, number]> {
  return [];
}
"#,
        r#"// @flow
export function tuple(): [A] {
  return [a];
}
"#,
    ] {
        let output = format(source);

        assert_eq!(
            output.matches(';').count(),
            source.matches(';').count(),
            "formatting added or removed a semicolon:\n--- input\n{source}--- output\n{output}"
        );
        assert_token_preserving(source, &output);
    }
}

/// The same shape after an angle-bracket return type, which already worked —
/// kept so the two cases cannot drift apart again.
#[test]
fn a_body_after_a_generic_return_type_gains_no_semicolon() {
    let source = r#"// @flow
function load(): Promise<void> {
  return go();
}
"#;

    let output = format(source);

    assert_eq!(output.matches(';').count(), source.matches(';').count());
    assert_token_preserving(source, &output);
}

/// An object literal directly after `]` is not valid JavaScript, so nothing
/// legitimate regresses from treating that brace as a block. An index
/// expression followed by a block statement still formats.
#[test]
fn an_indexed_access_followed_by_a_block_still_formats() {
    let source = r#"// @flow
const x = items[0];
{
  run();
}
"#;

    let output = format(source);

    assert_token_preserving(source, &output);
}
