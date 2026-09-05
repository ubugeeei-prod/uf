//! Minimal reproductions of formatter bugs that are filed and not yet fixed.
//!
//! Every one of these asserts the *correct* behaviour, so every one of them
//! fails today. They are `#[ignore]`d, which keeps CI green while keeping
//! the reproduction where the fix will be written rather than in an issue
//! body nobody greps.
//!
//! ```sh
//! cargo test -p uf_fmt --test known_bugs -- --ignored
//! ```
//!
//! When one is fixed, its reproduction moves to the test file that owns
//! the guarantee it broke and the entry here goes. When the last one does,
//! this file goes with it — its job is to be empty.
//!
//! Each was found by `upstream_corpus.rs`, running the formatter over the
//! third-party Flow in `tests/fixtures/git`.
//!
//! Gone from here: #128, unterminated JSX at end of input, now
//! `jsx_truncated_at_end_of_input_is_refused` in `guarantees.rs`.

use std::time::{Duration, Instant};

use uf_config::FmtConfig;
use uf_fmt::format_source;

/// ubugeeei-prod/uf#125 — deeply nested call arguments are exponential.
///
/// `print_arguments` prints its interesting child a second time to decide
/// whether to hug it, so the document is 2^depth. Measured at about 4x per
/// level and independent of the line width, which is what says it is the
/// document rather than the measuring:
///
/// | depth | time |
/// |---|---|
/// | 8 | 0.09s |
/// | 10 | 1.0s |
/// | 12 | 16s |
/// | 14 | killed at 30s |
///
/// React Native writes `expect.objectContaining` nineteen deep in
/// `react-native-compatibility-check`, and `uf fmt` does not finish on it.
#[test]
#[ignore = "ubugeeei-prod/uf#125"]
fn deeply_nested_call_arguments_finish_in_reasonable_time() {
    // Twelve rather than fourteen: fourteen does not finish, and a test
    // that hangs is a test people stop running. Twelve takes about sixteen
    // seconds, which is decisive enough against a two-second bound.
    let mut source = "x".to_owned();
    for _ in 0..12 {
        source = format!("expect.objectContaining({{ fault: {source} }})");
    }
    let source = format!("// @flow\nconst result = {source};\n");

    let started = Instant::now();
    format_source(&source, &FmtConfig::default()).expect("formats");
    let took = started.elapsed();

    assert!(
        took < Duration::from_secs(2),
        "twelve levels took {took:?}; depth 6 takes 0.03s"
    );
}

/// ubugeeei-prod/uf#126 — comment types are rewritten into real syntax.
///
/// Flow's comment types exist so that a file can carry annotations *and*
/// run without a build step. React Native's `scripts/spm` has such a file
/// and `node` runs it directly; after `uf fmt` it needs a compiler.
///
/// The corpus caught it as non-idempotence — the signature measures
/// differently once the annotations stop being comments — which is a
/// symptom. Fixing the layout alone would leave a formatter that quietly
/// requires a toolchain.
#[test]
#[ignore = "ubugeeei-prod/uf#126"]
fn comment_types_stay_comments() {
    let source = concat!(
        "// @flow\n",
        "\n",
        "function greet(name /*: string */) /*: string */ {\n",
        "  return 'hi ' + name;\n",
        "}\n",
    );

    let output = format_source(source, &FmtConfig::default())
        .expect("formats")
        .output;

    assert!(
        output.contains("/*: string */"),
        "the annotations stopped being comments:\n{output}"
    );
}
