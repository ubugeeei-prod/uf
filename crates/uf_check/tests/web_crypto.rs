//! Web Crypto, as Flow sees it.
//!
//! Flow's vendored `bom.js` types `crypto.subtle` with one method, `digest`.
//! `packages/server/internal/oauth.js` only hashes, so it checked clean;
//! `packages/server/internal/draft.js` signs the draft-mode cookie with
//! HMAC-SHA256 and reported four errors from `uf check` for calls that are
//! correct and that every runtime uf targets implements. See
//! ubugeeei-prod/uf#619.
//!
//! There are two fixtures and they are equally load-bearing. The first says
//! the API is usable; the second says it is *typed*, because a libdef that
//! declared `subtle` as `any` would pass the first one, close the issue, and
//! be worse than the gap — every call would check, including the wrong ones.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_source};

const WEB_CRYPTO: &str = include_str!("fixtures/web_crypto.js");
const WEB_CRYPTO_MISUSE: &str = include_str!("fixtures/web_crypto_misuse.js");

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    // No project library definitions: the point of these fixtures is what uf
    // ships, and a `[libs]` entry here would be checking the project's.
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
fn the_web_crypto_api_checks_clean() {
    let diagnostics = check("web_crypto.js", WEB_CRYPTO);

    assert!(
        diagnostics.is_empty(),
        "expected Web Crypto to check clean, got {:?}",
        lines_and_codes(&diagnostics)
    );
}

/// The lines of `fixtures/web_crypto_misuse.js` that must be reported, read
/// out of the fixture itself.
///
/// A `// misuse: <why>` comment says the line after it must be an error. Kept
/// beside the code rather than as a table of line numbers here, because a
/// table of line numbers is wrong the first time somebody inserts a line and
/// nobody notices until it is wrong in the reader's favour.
fn misuses() -> Vec<(u32, String)> {
    let marked: Vec<(u32, String)> = WEB_CRYPTO_MISUSE
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let why = line.trim().strip_prefix("// misuse: ")?;
            // Lines are one-based, and the line that must fail is the next one.
            Some((index as u32 + 2, why.to_owned()))
        })
        .collect();

    // Without this the test would pass on a fixture somebody had emptied.
    assert!(
        marked.len() >= 12,
        "the misuse fixture has {} marked lines; it is supposed to cover the surface",
        marked.len()
    );
    marked
}

#[test]
fn every_misuse_of_web_crypto_is_an_error() {
    let diagnostics = check("web_crypto_misuse.js", WEB_CRYPTO_MISUSE);
    let found = lines_and_codes(&diagnostics);

    // A line missing from `found` means that part of the declaration went
    // slack — which is what `subtle: any` would look like from here.
    //
    // Only the line is asserted, not the code: Flow answers a failed match
    // against a disjoint union with whichever incompatibility it reached
    // first, and which member of the union it names is not a contract. That
    // the call is refused is.
    for (line, why) in misuses() {
        assert!(
            found.iter().any(|(at, _)| *at == line),
            "line {line} must be an error — {why}; got {found:?}"
        );
    }
}

#[test]
fn nothing_else_in_the_misuse_fixture_is_an_error() {
    let diagnostics = check("web_crypto_misuse.js", WEB_CRYPTO_MISUSE);
    let found = lines_and_codes(&diagnostics);
    let marked = misuses();

    // The fixture is written so that only the marked lines are wrong, and it
    // ends with the same calls written correctly. An error anywhere else is
    // this libdef breaking something that was fine, which is the failure mode
    // a positive-only test cannot see.
    for (line, code) in &found {
        assert!(
            marked.iter().any(|(at, _)| at == line),
            "unexpected `{code}` on line {line}; the whole set is {found:?}"
        );
    }
}
