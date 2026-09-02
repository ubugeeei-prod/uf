//! Behaviour of the Flow eraser, one construct at a time.

mod annotations;
mod components;
mod declarations;
mod modules;
mod robustness;

use super::*;

/// Strip `source`, asserting the invariants that hold for every input.
fn strip(source: &str) -> String {
    let stripped = strip_types(source).expect("source is small enough to strip");
    assert_eq!(
        stripped.code.lines().count(),
        source.lines().count(),
        "erasure changed the line count\n--- in\n{source}\n--- out\n{}",
        stripped.code
    );
    let outcome = crate::validate_source(&stripped.code).expect("parser ran");
    assert!(
        outcome.is_ok(),
        "stripped output does not parse: {:?}\n{}",
        outcome.diagnostics,
        stripped.code
    );
    stripped.code
}

/// Collapse the whitespace erasure leaves behind, so a test can talk about
/// what survived rather than where the spaces landed.
fn squeezed(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut space = false;
    for character in source.chars() {
        if character == ' ' {
            space = true;
            continue;
        }
        let closing = matches!(character, ')' | ']' | '}' | ',' | ';');
        if space && !closing && !out.is_empty() && !out.ends_with('\n') {
            out.push(' ');
        }
        space = false;
        out.push(character);
    }
    out
}

fn stripped_text(source: &str) -> String {
    squeezed(&strip(source))
}
