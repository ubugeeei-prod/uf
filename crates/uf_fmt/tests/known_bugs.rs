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
//! When one is fixed, delete the `#[ignore]`. When all three are, delete the
//! file — its job is to be empty.
//!
//! Each was found by `upstream_corpus.rs`, running the formatter over React,
//! Metro, Relay and React Native.

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

/// ubugeeei-prod/uf#128 — unterminated JSX at EOF is formatted, not refused.
///
/// Fifty-eight bytes, no trailing newline. `flow::format` refuses anything
/// the parser reports a diagnostic for and the parser reports none, so the
/// printer runs and drops the `</div>`; the output does not parse. The same
/// source *with* a trailing newline is refused correctly, so the difference
/// is where EOF falls rather than the JSX.
///
/// Breaks two of the guarantees `guarantees.rs` states outright: invalid
/// syntax is refused rather than rewritten, and the output parses.
#[test]
#[ignore = "ubugeeei-prod/uf#128"]
fn unterminated_jsx_at_eof_is_refused() {
    let source = "const el = <div className=\"a\" data-testid='b'>text {value}";

    let Ok(result) = format_source(source, &FmtConfig::default()) else {
        // Refused, which is the correct outcome.
        return;
    };

    // Accepted. Then the least it can do is produce something that parses.
    format_source(&result.output, &FmtConfig::default()).unwrap_or_else(|error| {
        panic!(
            "formatted invalid syntax into output that does not parse: {error}\n\
             --- out\n{}",
            result.output
        )
    });
}
