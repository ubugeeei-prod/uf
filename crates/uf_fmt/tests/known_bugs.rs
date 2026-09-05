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
//! guarantee each broke. #126 has shrunk to its declaration half; its
//! annotation half is `comment_types.js` in the fixture directory.

use uf_config::FmtConfig;
use uf_fmt::format_source;

/// ubugeeei-prod/uf#126 — a `/*:: … */` declaration block is rewritten.
///
/// The annotation half of that issue is fixed: `/*: string */` survives, and
/// `comment_types.js` in the fixture directory pins it against Prettier.
/// What is left is the *declaration* form, where a whole statement lives
/// inside the comment:
///
/// ```text
/// /*:: import type { SpmGraph } from './spm-types'; */
/// /*:: type PbxEntry = { uuid: string }; */
/// ```
///
/// The parser hands those back as ordinary statements — their locations sit
/// inside the comment, which is how the annotation form is detected, but a
/// run of them shares one `/*::` and one `*/` and the printer has no notion
/// of that yet.
///
/// React Native's `scripts/spm/generate-spm-xcodeproj.js` is the module: it
/// is idempotent and keeps its tree now, so it is out of `KNOWN_BROKEN`, and
/// `node` still cannot run what comes out.
#[test]
#[ignore = "ubugeeei-prod/uf#126"]
fn comment_type_declarations_stay_comments() {
    let source = concat!(
        "// @flow\n",
        "\n",
        "/*:: type Named = { name: string }; */\n",
        "\n",
        "/*::\n",
        "type Pair = { a: string, b: string };\n",
        "type Triple = { a: string, b: string, c: string };\n",
        "*/\n",
        "\n",
        "const x = 1;\n",
    );

    let output = format_source(source, &FmtConfig::default())
        .expect("formats")
        .output;

    assert!(
        output.contains("/*:: type Named"),
        "the declaration stopped being a comment:\n{output}"
    );
    assert!(
        output.contains("/*::\ntype Pair"),
        "the block stopped being a comment:\n{output}"
    );
}
