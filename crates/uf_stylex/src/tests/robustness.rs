//! What a hostile or broken `.stylex.js` gets.
//!
//! Everything here is reachable from `node_modules`: a dependency ships a
//! `.stylex.js` and `uf build` reads it. So each case is a refusal with a typed
//! reason, never a panic, a hang, or a rule that quietly reaches the sheet.

use super::module;
use crate::error::{MAX_DECLARATIONS, MAX_OBJECT_DEPTH, MAX_SOURCE_BYTES, MAX_VALUE_BYTES};
use crate::parse::parse_module;
use crate::{StyleXError, compile_module};

#[test]
fn a_module_over_the_size_ceiling_is_refused_before_it_is_tokenized() {
    let source = "x".repeat(MAX_SOURCE_BYTES + 1);
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::SourceTooLarge { .. })
    ));
}

#[test]
fn a_module_at_the_size_ceiling_is_accepted() {
    let mut source = module("const s = stylex.create({ a: { color: \"red\" } });\n");
    source.push_str(&"\n".repeat(MAX_SOURCE_BYTES - source.len()));
    assert_eq!(source.len(), MAX_SOURCE_BYTES);
    assert!(parse_module(&source).is_ok());
}

#[test]
fn nesting_past_the_depth_cap_is_refused() {
    let source = module(
        "const s = stylex.create({ a: { color: { \":hover\": { \":focus\": \"red\" } } } });\n",
    );
    match parse_module(&source) {
        Err(StyleXError::NestingTooDeep { limit, .. }) => assert_eq!(limit, MAX_OBJECT_DEPTH),
        other => panic!("expected a depth refusal, got {other:?}"),
    }
}

#[test]
fn deeply_nested_braces_do_not_exhaust_the_stack() {
    // The extractor walks with a queue rather than by recursing, so this is a
    // refusal at a fixed cost instead of a stack overflow.
    let depth = 50_000;
    let mut body = String::from("const s = stylex.create({ a: ");
    body.push_str(&"{ k: ".repeat(depth));
    body.push_str("\"red\"");
    body.push_str(&" }".repeat(depth));
    body.push_str(" });\n");
    assert!(parse_module(&module(&body)).is_err());
}

#[test]
fn a_proto_namespace_is_refused() {
    let source = module("const s = stylex.create({ __proto__: { color: \"red\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ForbiddenKey { .. })
    ));
}

#[test]
fn a_proto_property_is_refused() {
    let source = module("const s = stylex.create({ a: { __proto__: \"red\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ForbiddenKey { .. })
    ));
}

#[test]
fn a_constructor_key_is_refused() {
    let source = module("const s = stylex.create({ a: { constructor: \"red\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ForbiddenKey { .. })
    ));
}

#[test]
fn a_prototype_key_is_refused() {
    let source = module("const s = stylex.create({ prototype: { color: \"red\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::ForbiddenKey { .. })
    ));
}

#[test]
fn a_value_that_closes_its_own_rule_is_refused() {
    let source =
        module("const s = stylex.create({ a: { color: \"red} .victim { display: none\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::UnsafeValue { .. })
    ));
}

#[test]
fn a_value_that_closes_a_style_element_is_refused() {
    let source = module("const s = stylex.create({ a: { content: \"</style><script>x()\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::UnsafeValue { .. })
    ));
}

#[test]
fn a_selector_that_escapes_its_rule_is_refused() {
    let source =
        module("const s = stylex.create({ a: { \":hover, .victim\": { display: \"none\" } } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::InvalidKey { .. })
    ));
}

#[test]
fn an_at_rule_that_escapes_its_block_is_refused() {
    let source = module(
        "const s = stylex.create({ a: { \"@media screen{}.victim\": { color: \"red\" } } });\n",
    );
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::InvalidKey { .. })
    ));
}

#[test]
fn a_property_name_with_a_colon_is_refused() {
    let source = module("const s = stylex.create({ a: { \"color:red;x\": \"1\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::InvalidKey { .. })
    ));
}

#[test]
fn an_over_long_value_is_refused() {
    let long = "a".repeat(MAX_VALUE_BYTES + 1);
    let source = module(&format!(
        "const s = stylex.create({{ a: {{ color: \"{long}\" }} }});\n"
    ));
    match parse_module(&source) {
        Err(StyleXError::ValueTooLong { limit, .. }) => assert_eq!(limit, MAX_VALUE_BYTES),
        other => panic!("expected a length refusal, got {other:?}"),
    }
}

#[test]
fn more_declarations_than_the_cap_are_refused() {
    let mut body = String::from("const s = stylex.create({ a: {");
    for index in 0..=MAX_DECLARATIONS {
        body.push_str(&format!(" p{index}: 1,"));
    }
    body.push_str("} });\n");
    match parse_module(&module(&body)) {
        Err(StyleXError::TooManyDeclarations { limit }) => assert_eq!(limit, MAX_DECLARATIONS),
        other => panic!("expected a count refusal, got {other:?}"),
    }
}

#[test]
fn an_unterminated_call_is_refused() {
    let source = module("const s = stylex.create({ a: { color: \"red\" }\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::UnterminatedObject { .. })
    ));
}

#[test]
fn an_unterminated_string_does_not_reach_the_sheet() {
    let source = module("const s = stylex.create({ a: { color: \"red } });\n");
    assert!(parse_module(&source).is_err());
}

#[test]
fn an_empty_module_compiles_to_itself() {
    let compiled = compile_module("").expect("an empty module compiles");
    assert_eq!(compiled.code, "");
    assert!(!compiled.changed);
}

#[test]
fn an_empty_create_call_compiles_to_an_empty_object() {
    let compiled =
        compile_module(&module("const s = stylex.create({});\n")).expect("an empty call");
    assert!(compiled.code.contains("const s = {};"));
    assert!(compiled.sheet.is_empty());
}

#[test]
fn an_empty_namespace_compiles_to_a_marker_only_object() {
    let compiled = compile_module(&module("const s = stylex.create({ a: {} });\n"))
        .expect("an empty namespace");
    assert!(compiled.code.contains("{\"a\":{\"$$css\":true}}"));
}

#[test]
fn a_module_that_is_only_a_byte_order_mark_compiles() {
    let compiled = compile_module("\u{feff}").expect("a bare BOM compiles");
    assert!(!compiled.changed);
}

#[test]
fn a_module_that_is_only_a_shebang_compiles() {
    let compiled = compile_module("#!/usr/bin/env node\n").expect("a shebang compiles");
    assert!(!compiled.changed);
}

#[test]
fn a_non_ascii_namespace_is_refused_rather_than_emitted() {
    let source = module("const s = stylex.create({ 見出し: { color: \"red\" } });\n");
    assert!(matches!(
        parse_module(&source),
        Err(StyleXError::InvalidKey { .. })
    ));
}

#[test]
fn a_hostile_module_never_panics() {
    // Every one of these has broken a naive extractor at some point; none of
    // them may do worse here than return an error.
    for source in [
        "import { stylex } from \"@uniflowed/stylex\";stylex.create",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create(",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({a",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({a:",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({a:{",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({a:{b",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({a:{b:",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({a:{b:}",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({}}}}",
        "import { stylex } from \"@uniflowed/stylex\";stylex.create({a:{b:\"c\"}})))",
        "import { stylex } from \"@uniflowed/stylex\";stylex.defineVars",
        "import { stylex } from \"@uniflowed/stylex\";const t = stylex.defineVars({",
        "import { stylex } from \"@uniflowed/stylex\";const t = stylex.defineVars({a:t.a})",
        "import",
        "import {",
        "import { stylex } from",
        "import { stylex } from \"@uniflowed/stylex",
        "stylex",
        ".stylex.js",
    ] {
        let _ = compile_module(source);
    }
}
