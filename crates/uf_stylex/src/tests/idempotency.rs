//! Determinism and idempotency.
//!
//! A build is only reproducible if the same input produces the same bytes, and
//! only re-runnable if compiling the output again changes nothing. Both are
//! properties of the pass rather than of any one input, so they are checked
//! over a fixture that exercises every shape the extractor understands.

use super::{compile, module};
use crate::compile::compile_module;
use crate::sheet::StyleSheet;

/// A module using namespaces, conditions, at-rules, variables and shorthands.
fn fixture() -> String {
    module(
        "export const tokens = stylex.defineVars({ ink: \"#151b1f\", canvas: \"#f7f7f2\" });\n\
         const styles = stylex.create({\n\
         \x20 shell: { margin: 0, marginTop: 8, backgroundColor: tokens.canvas },\n\
         \x20 label: {\n\
         \x20   color: { default: tokens.ink, \":hover\": \"red\", \"@media (min-width: 600px)\": \"blue\" },\n\
         \x20   \"::before\": { content: \"x\" },\n\
         \x20 },\n\
         });\n",
    )
}

#[test]
fn compiling_the_same_module_twice_produces_the_same_bytes() {
    let source = fixture();
    let first = compile(&source);
    let second = compile(&source);
    similar_asserts::assert_eq!(first.code, second.code);
    similar_asserts::assert_eq!(first.sheet.to_css(), second.sheet.to_css());
}

#[test]
fn compiling_the_output_again_changes_nothing() {
    let source = fixture();
    let once = compile(&source);
    let twice = compile(&once.code);
    assert!(!twice.changed, "the rewritten module has no call left");
    similar_asserts::assert_eq!(twice.code, once.code);
    assert!(twice.sheet.is_empty());
}

#[test]
fn a_third_pass_still_changes_nothing() {
    let source = fixture();
    let once = compile(&source);
    let twice = compile(&once.code);
    let thrice = compile(&twice.code);
    similar_asserts::assert_eq!(thrice.code, once.code);
}

#[test]
fn whitespace_between_declarations_does_not_change_the_sheet() {
    let tight = compile(&module(
        "const s = stylex.create({a:{color:\"red\",marginTop:8}});\n",
    ));
    let loose = compile(&module(
        "const s = stylex.create({\n  a: {\n    color: \"red\",\n\n    marginTop: 8,\n  },\n});\n",
    ));
    similar_asserts::assert_eq!(tight.sheet.to_css(), loose.sheet.to_css());
}

#[test]
fn a_trailing_comma_does_not_change_the_sheet() {
    let with = compile(&module(
        "const s = stylex.create({ a: { color: \"red\", } });\n",
    ));
    let without = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    similar_asserts::assert_eq!(with.sheet.to_css(), without.sheet.to_css());
}

#[test]
fn crlf_and_lf_produce_the_same_sheet() {
    let lf = compile(&fixture());
    let crlf = compile(&fixture().replace('\n', "\r\n"));
    similar_asserts::assert_eq!(lf.sheet.to_css(), crlf.sheet.to_css());
}

#[test]
fn a_comment_between_declarations_does_not_change_the_sheet() {
    let plain = compile(&module(
        "const s = stylex.create({ a: { color: \"red\" } });\n",
    ));
    let commented = compile(&module(
        "const s = stylex.create({ /* tone */ a: { color: \"red\" } });\n",
    ));
    similar_asserts::assert_eq!(plain.sheet.to_css(), commented.sheet.to_css());
}

#[test]
fn declaration_order_inside_a_namespace_does_not_change_the_sheet() {
    let forwards = compile(&module(
        "const s = stylex.create({ a: { color: \"red\", opacity: 1 } });\n",
    ));
    let backwards = compile(&module(
        "const s = stylex.create({ a: { opacity: 1, color: \"red\" } });\n",
    ));
    similar_asserts::assert_eq!(forwards.sheet.to_css(), backwards.sheet.to_css());
}

#[test]
fn a_module_compiled_alone_and_in_a_sheet_agrees_with_itself() {
    let compiled = compile(&fixture());
    let mut sheet = StyleSheet::new();
    sheet.extend(&compiled.sheet);
    similar_asserts::assert_eq!(sheet.to_css(), compiled.sheet.to_css());
}

#[test]
fn the_class_names_are_pinned() {
    // A golden test on purpose: a change to the hash construction changes every
    // class name in every project, invalidates every CDN cache entry, and must
    // therefore be a deliberate edit to this list rather than a side effect.
    let compiled = compile(&module(
        "const s = stylex.create({ shell: { color: \"red\", marginTop: 8 } });\n",
    ));
    similar_asserts::assert_eq!(
        compiled.sheet.to_css(),
        "\
.x2z9w9t3hlsbot{color:red}
.x35kb6byhc7nw4{margin-top:8px}
"
    );
}

#[test]
fn the_compiled_module_is_pinned() {
    let compiled = compile_module(
        "import { stylex } from \"@uniflowed/stylex\";\nconst s = stylex.create({ shell: { color: \"red\" } });\n",
    )
    .expect("a module that compiles");
    similar_asserts::assert_eq!(
        compiled.code,
        "import { stylex } from \"@uniflowed/stylex\";\nconst s = {\"shell\":{\"$$css\":true,\"color\":\"x2z9w9t3hlsbot\"}};\n"
    );
}
