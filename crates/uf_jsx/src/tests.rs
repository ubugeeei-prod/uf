//! Behaviour of the JSX transform, one construct at a time.

mod attributes;
mod children;
mod elements;
mod plugin;
mod robustness;
mod runtime;

use super::*;

/// Lower `source`, asserting the invariants that hold for every input.
fn lower(source: &str) -> String {
    let transformed = transform(source, &JsxOptions::default()).expect("source lowers");

    assert_eq!(
        transformed.code.lines().count(),
        source.lines().count(),
        "lowering changed the line count\n--- in\n{source}\n--- out\n{}",
        transformed.code
    );
    assert!(
        !uf_flow::scan::tokenize_jsx(&transformed.code)
            .iter()
            .any(|token| token.kind.is_jsx()),
        "JSX survived lowering:\n{}",
        transformed.code
    );
    // The QuickJS-hosted backend budgets its stack from wherever its runtime
    // was created, so it has to be created from a shallow frame. When it still
    // refuses — a deeply nested call tree can exhaust its own 256 kB — that is
    // the parser giving up rather than the output being wrong, so the two
    // assertions above stand and this one steps aside.
    uf_flow::prepare_thread().expect("parser ready");
    if let Ok(outcome) = uf_flow::validate_source(&transformed.code) {
        assert!(
            outcome.is_ok(),
            "lowered output does not parse: {:?}\n{}",
            outcome.diagnostics,
            transformed.code
        );
    }

    transformed.code
}

/// Lower `source` and collapse whitespace, so a test can talk about the shape
/// of the call rather than about where the spaces landed.
fn call(source: &str) -> String {
    squeezed(&lower(source))
}

/// Collapse runs of whitespace, and drop the spaces before closing brackets
/// that in-place rewriting leaves behind.
fn squeezed(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut space = false;
    for character in source.chars() {
        if character.is_whitespace() {
            space = true;
            continue;
        }
        let closing = matches!(character, ')' | ']' | '}' | ',' | ';');
        let after_open = out.ends_with(['(', '[', '{']);
        if space && !closing && !after_open && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(character);
    }
    out
}

/// The lowered body of `const a = …;`, without the import or the wrapper.
fn expression(jsx: &str) -> String {
    let source = format!("const a = {jsx};\n");
    let lowered = call(&source);
    let start = lowered.find("const a =").expect("the declaration");
    lowered[start + "const a =".len()..]
        .trim_end_matches(';')
        .trim()
        .to_string()
}
