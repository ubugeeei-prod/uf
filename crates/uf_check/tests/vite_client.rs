//! The Vite client API, as Flow sees it.
//!
//! A TypeScript project gets `import.meta.glob`, `import.meta.hot` and
//! `import.meta.env` from `vite/client`. Flow's own `core.js` types
//! `import.meta` as `{ [key: string]: unknown, url?: string }` and stops, so
//! all three were type errors in a uf project until `libdefs/vite-client.js`
//! shipped. See ubugeeei-prod/uf#264.
//!
//! There are two fixtures and they are equally load-bearing. The first says
//! the API is usable; the second says it is *typed*, because a libdef that
//! declared everything `any` would pass the first one and be worthless.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_source};

const VITE_CLIENT: &str = include_str!("fixtures/vite_client.js");
const VITE_CLIENT_MISUSE: &str = include_str!("fixtures/vite_client_misuse.js");

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    check_source(Source::new(path, source), &limits()).expect("the checker runs")
}

/// The `(line, code)` of every diagnostic, which is what these tests compare.
///
/// Codes rather than message text: the codes are Flow's public contract and
/// the prose is not, so an upstream rewording must not turn this red.
fn lines_and_codes(diagnostics: &[TypeDiagnostic]) -> Vec<(u32, &str)> {
    let mut found: Vec<(u32, &str)> = diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.primary.start.line,
                diagnostic.code.unwrap_or("<none>"),
            )
        })
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

#[test]
fn the_vite_client_api_checks_clean() {
    let diagnostics = check("vite_client.js", VITE_CLIENT);

    assert!(
        diagnostics.is_empty(),
        "expected the Vite client API to check clean, got {:?}",
        lines_and_codes(&diagnostics)
    );
}

#[test]
fn every_misuse_of_the_vite_client_api_is_an_error() {
    let diagnostics = check("vite_client_misuse.js", VITE_CLIENT_MISUSE);
    let found = lines_and_codes(&diagnostics);

    // Each line of the fixture that must not typecheck, and why. A line
    // missing from `found` means the declaration for that API went slack.
    let required: &[(u32, &str, &str)] = &[
        (10, "incompatible-type", "MODE is a string, not a number"),
        (13, "incompatible-type", "PROD is a boolean, not a string"),
        (
            16,
            "incompatible-type",
            "a .env key may be unset, so it is not a bare string",
        ),
        (
            20,
            "incompatible-exact",
            "the lazy form hands back a thunk, not the module",
        ),
        (
            24,
            "incompatible-type",
            "the eager form hands back the module, not a thunk",
        ),
        (27, "incompatible-type", "`eager` is a boolean"),
        (
            34,
            "incompatible-use",
            "`hot` is optional, and `if (import.meta.hot)` does not refine it",
        ),
    ];

    for (line, code, why) in required {
        assert!(
            found.contains(&(*line, *code)),
            "line {line} must be a `{code}` error — {why}; got {found:?}"
        );
    }
}

#[test]
fn nothing_else_in_the_misuse_fixture_is_an_error() {
    let diagnostics = check("vite_client_misuse.js", VITE_CLIENT_MISUSE);
    let found = lines_and_codes(&diagnostics);

    // The fixture is written so that only the lines above are wrong. An error
    // on any other line is this libdef breaking something that was fine, which
    // is the failure mode a positive-only test cannot see.
    let expected_lines = [10, 13, 16, 20, 24, 27, 34];
    for (line, code) in &found {
        assert!(
            expected_lines.contains(line),
            "unexpected `{code}` on line {line}; the whole set is {found:?}"
        );
    }
}
