//! Flow's type syntax: type arguments, nullable and optional markers, exact
//! object types, unions, variance and opaque types.

use super::*;

#[test]
fn type_arguments_are_printed_without_spaces() {
    similar_asserts::assert_eq!(
        format("const m: Map < string , number > = new Map();\n"),
        "const m: Map<string, number> = new Map();\n"
    );
    similar_asserts::assert_eq!(
        format("type Nested = Array<Map<string, Array<number>>>;\n"),
        "type Nested = Array<Map<string, Array<number>>>;\n"
    );
}

#[test]
fn comparisons_are_not_mistaken_for_type_arguments() {
    similar_asserts::assert_eq!(
        format("const ok=a<b&&c>d;\n"),
        "const ok = a < b && c > d;\n"
    );
    similar_asserts::assert_eq!(format("if (a<b) f();\n"), "if (a < b) f();\n");
}

#[test]
fn nullable_and_optional_flow_markers_hug_their_type() {
    similar_asserts::assert_eq!(format("type A = ? string;\n"), "type A = ?string;\n");
    similar_asserts::assert_eq!(
        format("function f(x ? : ?number) {}\n"),
        "function f(x?: ?number) {}\n"
    );
}

#[test]
fn exact_object_types_keep_their_pipes_attached() {
    similar_asserts::assert_eq!(
        format("type E = {| a: number |};\n"),
        "type E = {| a: number |};\n"
    );
}

#[test]
fn unions_and_variance_are_spaced_like_operators() {
    similar_asserts::assert_eq!(format("type U = | A|B & C;\n"), "type U = | A | B & C;\n");
    similar_asserts::assert_eq!(
        format("type V = { +read: string, -write: number };\n"),
        "type V = { +read: string, -write: number };\n"
    );
}

#[test]
fn opaque_types_and_generics_survive() {
    similar_asserts::assert_eq!(
        format("opaque type   Id<T> =  Wrapped<T>;\n"),
        "opaque type Id<T> = Wrapped<T>;\n"
    );
}
