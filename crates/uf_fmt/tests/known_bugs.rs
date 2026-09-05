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
//! this file goes with it — its job is to be empty. One left.
//!
//! Each was found by `upstream_corpus.rs`, running the formatter over the
//! third-party Flow in `tests/fixtures/git`.
//!
//! Gone from here: #128, unterminated JSX at end of input, and #125,
//! exponential call arguments — both now in `guarantees.rs`, beside the
//! guarantee each broke.

use uf_config::FmtConfig;
use uf_fmt::format_source;

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
