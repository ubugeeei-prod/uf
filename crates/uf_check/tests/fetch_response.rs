//! Fetch's `Response` and `AbortSignal`, as Flow sees them.
//!
//! The vendored `bom.js` declares `Response.error()` and `Response.redirect()`
//! and no `Response.json()`, so every route handler answering with JSON was
//! "property json is missing in statics of Response": 32 errors in this
//! repository. `libdefs/fetch.js` redeclares the class whole with the
//! standard's `json` added. See ubugeeei-prod/uf#1451.
//!
//! Two fixtures, as for Web Crypto: one says the API is usable, the other that
//! it is typed.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_source};

const RESPONSE: &str = include_str!("fixtures/fetch_response.js");
const RESPONSE_MISUSE: &str = include_str!("fixtures/fetch_response_misuse.js");

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    check_source(Source::new(path, source), &[], &limits()).expect("the checker runs")
}

/// The line of every error, deduplicated. Lines rather than messages: the
/// prose is upstream's and not a contract.
fn error_lines(diagnostics: &[TypeDiagnostic]) -> Vec<u32> {
    let mut lines: Vec<u32> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.primary.start.line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

#[test]
fn response_json_and_the_rest_of_the_class_check_clean() {
    let diagnostics = check("fetch_response.js", RESPONSE);

    assert!(
        diagnostics.is_empty(),
        "expected Response to check clean, got errors on lines {:?}",
        error_lines(&diagnostics)
    );
}

#[test]
fn a_misused_response_json_is_an_error_and_nothing_else_is() {
    let diagnostics = check("fetch_response_misuse.js", RESPONSE_MISUSE);

    // A string status, the response mistaken for the data it carries, and a
    // factory called on an instance.
    assert_eq!(error_lines(&diagnostics), [8, 11, 15], "{diagnostics:#?}");
}
