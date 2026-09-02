//! Edge cases: encodings, size ceilings, unusual input, idempotency.

use super::super::*;
use super::{squeezed, stripped_text};

#[test]
fn plain_javascript_is_returned_unchanged() {
    let source = "const a = 1;\nexport function f() {\n  return a;\n}\n";

    let stripped = strip_types(source).expect("strips");

    assert!(stripped.is_unchanged());
    assert_eq!(stripped.code, source);
}

#[test]
fn empty_input_is_unchanged() {
    let stripped = strip_types("").expect("strips");

    assert!(stripped.is_unchanged());
    assert_eq!(stripped.code, "");
}

#[test]
fn a_source_of_only_whitespace_is_unchanged() {
    let stripped = strip_types("\n\n  \n").expect("strips");

    assert_eq!(stripped.code, "\n\n  \n");
}

#[test]
fn erasure_preserves_byte_length_for_pure_removals() {
    let source = "// @flow\nconst a: number = 1;\n";

    let stripped = strip_types(source).expect("strips");

    assert_eq!(stripped.code.len(), source.len());
}

#[test]
fn a_source_over_the_ceiling_is_refused() {
    let source = "a".repeat(MAX_STRIP_BYTES + 1);

    let error = strip_types(&source).expect_err("refused");

    assert_eq!(
        error,
        StripError::SourceTooLarge {
            bytes: MAX_STRIP_BYTES + 1,
            limit: MAX_STRIP_BYTES,
        }
    );
}

#[test]
fn a_source_exactly_at_the_ceiling_is_accepted() {
    let source = "a".repeat(MAX_STRIP_BYTES);

    assert!(strip_types(&source).is_ok());
}

#[test]
fn a_leading_pipe_union_alias_is_erased_whole() {
    let source = "// @flow\ntype Tone =\n  | \"calm\"\n  | \"sharp\";\nconst a = 1;\n";

    let out = stripped_text(source);

    assert!(!out.contains("calm"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn a_colon_inside_a_string_is_never_erased() {
    let out = stripped_text("// @flow\nconst a: string = \"key: value\";\n");

    assert!(out.contains("\"key: value\""), "{out}");
}

#[test]
fn a_type_inside_a_comment_is_never_erased() {
    let source = "// @flow\n// type Id = string;\nconst a = 1;\n";

    let stripped = strip_types(source).expect("strips");

    assert_eq!(stripped.code, source);
}

#[test]
fn jsx_survives_erasure() {
    let source =
        "// @flow\ncomponent Page(name: string) {\n  return <main><p>tone: {name}</p></main>;\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("<p>tone: {name}</p>"), "{out}");
}

#[test]
fn a_jsx_closing_tag_does_not_swallow_the_rest_of_the_line() {
    let source = "// @flow\nconst node = <div>a</div>;\nconst a: number = 1;\n";

    let out = stripped_text(source);

    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn crlf_sources_keep_their_line_endings() {
    let source = "// @flow\r\ntype Id = string;\r\nconst a: number = 1;\r\n";

    let stripped = strip_types(source).expect("strips");

    assert_eq!(stripped.code.matches("\r\n").count(), 3);
    assert!(
        squeezed(&stripped.code).contains("const a = 1;"),
        "{}",
        stripped.code
    );
}

#[test]
fn a_byte_order_mark_is_preserved() {
    let source = "\u{feff}// @flow\nconst a: number = 1;\n";

    let stripped = strip_types(source).expect("strips");

    assert!(stripped.code.starts_with('\u{feff}'));
    assert!(
        squeezed(&stripped.code).contains("const a = 1;"),
        "{}",
        stripped.code
    );
}

#[test]
fn a_shebang_line_is_preserved() {
    let source = "#!/usr/bin/env uf\n// @flow\nconst a: number = 1;\n";

    let stripped = strip_types(source).expect("strips");

    assert!(stripped.code.starts_with("#!/usr/bin/env uf\n"));
}

#[test]
fn non_ascii_identifiers_and_strings_survive() {
    let source = "// @flow\nconst café: string = \"caffè ☕\";\n";

    let stripped = strip_types(source).expect("strips");

    assert!(stripped.code.contains("caffè ☕"), "{}", stripped.code);
    assert!(
        squeezed(&stripped.code).contains("const café = \"caffè ☕\";"),
        "{}",
        stripped.code
    );
}

#[test]
fn erasure_is_idempotent() {
    let source = "// @flow\nexport type Id = string;\ncomponent Page(a: number) renders Node {\n  return a;\n}\n";

    let once = strip_types(source).expect("strips").code;
    let twice = strip_types(&once).expect("strips").code;

    assert_eq!(once, twice);
}

#[test]
fn an_unterminated_string_leaves_the_source_alone() {
    let source = "// @flow\nconst a = \"unterminated;\n";

    let stripped = strip_types(source).expect("strips");

    assert_eq!(stripped.code, source);
}

#[test]
fn an_unbalanced_angle_bracket_does_not_erase_the_rest_of_the_file() {
    let source = "// @flow\nconst a = x < y;\nconst b = 2;\n";

    let out = stripped_text(source);

    assert!(out.contains("const b = 2;"), "{out}");
}

#[test]
fn deeply_nested_types_are_erased_whole() {
    let annotation = "Array<".repeat(64) + "number" + &">".repeat(64);
    let source = format!("// @flow\nconst a: {annotation} = [];\n");

    let out = stripped_text(&source);

    assert!(out.contains("const a = [];"), "{out}");
}

#[test]
fn a_large_source_is_stripped_without_changing_its_shape() {
    let unit =
        "// @flow\ntype Id = string;\nexport function f(a: number): number {\n  return a;\n}\n";
    let source = unit.repeat(2_000);

    let stripped = strip_types(&source).expect("strips");

    assert_eq!(stripped.code.lines().count(), source.lines().count());
    assert!(!stripped.code.contains("type Id"));
    assert_eq!(
        squeezed(&stripped.code)
            .matches("export function f(a) {")
            .count(),
        2_000
    );
}

#[test]
fn a_directive_prologue_survives_erasure() {
    let source = "\"use client\";\n// @flow\ntype Id = string;\nexport const a: Id = \"a\";\n";

    let out = stripped_text(source);

    assert!(out.starts_with("\"use client\";"), "{out}");
    assert!(out.contains("export const a = \"a\";"), "{out}");
}
