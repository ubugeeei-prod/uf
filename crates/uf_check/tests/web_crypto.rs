//! Web Crypto, as Flow sees it.
//!
//! The vendored `bom.js` types `crypto.subtle` as an object with `digest` in
//! it and nothing else, so a module that only hashes checked clean while
//! `packages/server/internal/draft.js` — which HMACs the draft-mode cookie —
//! reported four errors for calls every runtime uf targets implements, and had
//! no `CryptoKey` to annotate the key with. `libdefs/web-crypto.js` is the
//! answer. See ubugeeei-prod/uf#619.
//!
//! Two fixtures, equally load-bearing. The first says the API is usable; the
//! second says it is *typed*, because a declaration that made the algorithm a
//! `string` and the key an `any` would pass the first and be worse than the
//! gap it closed.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_source};

const WEB_CRYPTO: &str = include_str!("fixtures/web_crypto.js");
const WEB_CRYPTO_MISUSE: &str = include_str!("fixtures/web_crypto_misuse.js");

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    check_source(Source::new(path, source), &[], &limits()).expect("the checker runs")
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
fn the_web_crypto_a_uf_project_calls_checks_clean() {
    let diagnostics = check("web_crypto.js", WEB_CRYPTO);

    assert!(
        diagnostics.is_empty(),
        "expected Web Crypto to check clean, got {:?}",
        lines_and_codes(&diagnostics)
    );
}

#[test]
fn every_misuse_of_web_crypto_is_an_error() {
    let diagnostics = check("web_crypto_misuse.js", WEB_CRYPTO_MISUSE);
    let found = lines_and_codes(&diagnostics);

    // Each line of the fixture that must not typecheck, and why. A line
    // missing from `found` means the declaration for that call went slack.
    let required: &[(u32, &str)] = &[
        (10, "an algorithm name that is not one"),
        (13, "the dictionary form of the same typo"),
        (16, "`ECDSA` without the hash it signs under"),
        (19, "bytes where a `CryptoKey` belongs"),
        (22, "`HMAC` without its hash"),
        (25, "a key usage that is not one"),
        (28, "a key format that is not one"),
        (31, "a digest that does not exist"),
        (35, "`extractable` and `usages` the wrong way round"),
    ];

    let missing: Vec<&str> = required
        .iter()
        .filter(|(line, _)| !found.iter().any(|(reported, _)| reported == line))
        .map(|(_, why)| *why)
        .collect();
    assert!(
        missing.is_empty(),
        "these misuses were accepted: {missing:?}; the checker reported {found:?}"
    );
}
