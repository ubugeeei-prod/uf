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

/// A bounded or variance-annotated type parameter list is still a type
/// parameter list.
///
/// The speculative scan accepted only a handful of keywords between the angle
/// brackets, and `extends` was not among them, so it gave up and printed the
/// brackets as comparisons: `route<P extends string>(` came out as
/// `route < P extends string > (`. These are the spellings Flow now requires —
/// `<T: Bound>` and `<+T>` are deprecated — so the formatter mangled exactly
/// the syntax uf tells users to write, and the router codegen had the mangling
/// baked into it to survive `uf fmt --check`.
#[test]
fn bounded_and_variance_annotated_type_parameters_are_not_comparisons() {
    for source in [
        "declare export function route<P extends string>(p: P): string;\n",
        "export function f<P extends string>(p: P): P {\n  return p;\n}\n",
        "type A<P extends string> = P;\n",
        "type B<A, P extends string> = [A, P];\n",
        "type N<P extends Array<string>> = P;\n",
        "class C<P extends string> {}\n",
        // `out` lexes as an identifier, `in` as a keyword; both are the modern
        // replacements for the deprecated `+`/`-` sigils.
        "type Cov<out P> = (P) => void;\n",
        "type Con<in P> = (P) => void;\n",
    ] {
        similar_asserts::assert_eq!(format(source), source);
    }

    // And the spacing is actively removed, not merely preserved.
    similar_asserts::assert_eq!(
        format("type A < P extends string > = P;\n"),
        "type A<P extends string> = P;\n"
    );
}

/// `in` inside angle brackets must not make an ordinary `in` lose its spacing.
#[test]
fn the_in_operator_is_still_an_operator() {
    similar_asserts::assert_eq!(
        format("const has = \"k\" in obj;\n"),
        "const has = \"k\" in obj;\n"
    );
    similar_asserts::assert_eq!(
        format("for (const k in obj) {\n  f(k);\n}\n"),
        "for (const k in obj) {\n  f(k);\n}\n"
    );
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
