//! Node's `url`, `fs` and `async_hooks`, Fetch's `Request`, `Headers` and
//! `URLSearchParams`, and `Intl.RelativeTimeFormat`, as Flow sees them.
//!
//! Each was a gap in the vendored environment library definitions that made
//! correct code an error: `pathToFileURL(path).href` possibly `undefined`, a
//! `Dirent`'s name possibly a `Buffer`, no `async_hooks` module at all, no
//! `getSetCookie`, no `RelativeTimeFormat`, and a `Request` refused as another
//! `Request`'s init. `libdefs/node-*.js`, `fetch.js` and
//! `intl-relative-time.js` declare each as the runtime has it. See
//! ubugeeei-prod/uf#1451.
//!
//! Two fixtures, as for Web Crypto: the first says the surfaces are usable,
//! the second that they are typed narrowly enough to be wrong about.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_source};

const CLEAN: &str = include_str!("fixtures/runtime_libdefs.js");
const MISUSE: &str = include_str!("fixtures/runtime_libdefs_misuse.js");

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

fn check(path: &str, source: &str) -> Vec<TypeDiagnostic> {
    check_source(Source::new(path, source), &[], &limits()).expect("the checker runs")
}

/// The line of every diagnostic, deduplicated. Lines rather than messages:
/// the prose is upstream's and not a contract.
fn lines(diagnostics: &[TypeDiagnostic]) -> Vec<u32> {
    let mut lines: Vec<u32> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.primary.start.line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

#[test]
fn the_node_fetch_and_intl_surfaces_check_clean() {
    let diagnostics = check("runtime_libdefs.js", CLEAN);

    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got {diagnostics:#?}"
    );
}

#[test]
fn every_misuse_is_an_error_and_nothing_else_is() {
    let diagnostics = check("runtime_libdefs_misuse.js", MISUSE);

    // A `URL` taken for a string, buffers taken for strings, a `duplex` that
    // is not "half", a unit that is not one, and a store that may be absent.
    assert_eq!(lines(&diagnostics), MISUSE_LINES, "{diagnostics:#?}");
}

const MISUSE_LINES: [u32; 5] = [0, 0, 0, 0, 0];
