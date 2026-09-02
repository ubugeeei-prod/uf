//! Type aliases, interfaces, `declare` statements and type parameters.

use super::super::*;
use super::stripped_text;

#[test]
fn type_aliases_are_erased() {
    let out = stripped_text("// @flow\ntype Id = string;\nconst a = 1;\n");

    assert!(!out.contains("type Id"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn opaque_type_aliases_are_erased() {
    let out = stripped_text("// @flow\nexport opaque type Id = string;\nconst a = 1;\n");

    assert!(!out.contains("opaque"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn a_multi_line_object_type_alias_is_erased_whole() {
    let source = "// @flow\ntype Box = {\n  value: string,\n  next: ?Box,\n};\nconst a = 1;\n";

    let out = stripped_text(source);

    assert!(!out.contains("Box"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn a_type_alias_without_a_semicolon_ends_at_the_line_break() {
    let out = stripped_text("// @flow\ntype Id = string\nconst a = 1\n");

    assert!(!out.contains("Id"), "{out}");
    assert!(out.contains("const a = 1"), "{out}");
}

#[test]
fn generic_type_aliases_are_erased() {
    let out = stripped_text("// @flow\ntype Box<T> = { value: T };\nconst a = 1;\n");

    assert!(!out.contains("Box"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn interfaces_are_erased() {
    let out = stripped_text("// @flow\ninterface Shape {\n  area(): number;\n}\nconst a = 1;\n");

    assert!(!out.contains("Shape"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn declare_statements_are_erased() {
    let out = stripped_text("// @flow\ndeclare var globalThing: string;\nconst a = 1;\n");

    assert!(!out.contains("globalThing"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn declare_blocks_are_erased() {
    let source = "// @flow\ndeclare module foo {\n  declare var x: number;\n}\nconst a = 1;\n";

    let out = stripped_text(source);

    assert!(!out.contains("declare"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn class_type_parameters_and_implements_clauses_are_erased() {
    let source = "// @flow\ninterface Shape {}\nclass Box<T> implements Shape {\n  value: T;\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("class Box {"), "{out}");
    assert!(!out.contains("implements"), "{out}");
}

#[test]
fn function_type_parameters_are_erased() {
    let out = stripped_text("// @flow\nfunction identity<T>(value: T): T {\n  return value;\n}\n");

    assert!(out.contains("identity (value) {"), "{out}");
    assert!(!out.contains('<'), "{out}");
}

#[test]
fn a_type_alias_inside_a_template_literal_is_never_erased() {
    let source = "// @flow\nconst text = `type Id = string;`;\n";

    let stripped = strip_types(source).expect("strips");

    assert_eq!(stripped.code, source);
}
