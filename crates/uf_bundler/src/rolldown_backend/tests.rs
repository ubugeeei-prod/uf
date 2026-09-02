//! Bundling a real Flow project through Rolldown.
//!
//! These assert on the bytes Rolldown emitted, not on a shape uf computed:
//! whether the Flow types are gone, whether the JSX is lowered, whether the
//! output is JavaScript a parser accepts, and whether an export nothing imports
//! survived. A test that only counted chunks would pass on output no engine
//! could run.

use camino::Utf8PathBuf;
use uf_config::UniflowedConfig;

use super::{RolldownBundle, bundle};
use crate::BundleOptions;

/// Write `files` into a temporary project and bundle `entry` through Rolldown.
fn bundled(files: &[(&str, &str)], entry: &str) -> (tempfile::TempDir, RolldownBundle) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

    for (name, source) in files {
        let path = root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
    }

    let mut options = BundleOptions::new(&root, root.join("dist"));
    options.entries = vec![Utf8PathBuf::from(entry)];

    let output = bundle(&options, &UniflowedConfig::default(), &[]).expect("bundle");
    (dir, output)
}

/// The single chunk of a one-entry build.
fn only_chunk(output: &RolldownBundle) -> &str {
    assert_eq!(output.chunks.len(), 1, "{:?}", output.chunks);
    &output.chunks[0].code
}

#[test]
fn flow_types_are_erased_and_the_output_parses() {
    let (_dir, output) = bundled(
        &[
            (
                "main.js",
                "// @flow\nimport { greet } from \"./greet.js\";\nconst name: string = \"world\";\nconsole.log(greet(name));\n",
            ),
            (
                "greet.js",
                "// @flow\nexport type Greeting = string;\nexport function greet(who: string): Greeting {\n  return `hello ${who}`;\n}\n",
            ),
        ],
        "main.js",
    );

    let code = only_chunk(&output);
    assert!(
        !code.contains(": string"),
        "type annotation survived:\n{code}"
    );
    assert!(
        !code.contains("export type"),
        "type alias survived:\n{code}"
    );
    assert!(
        code.contains("hello "),
        "the module body is missing:\n{code}"
    );

    // It has to be JavaScript, not merely free of the strings above.
    let parsed = uf_flow::validate_source(code).expect("parser ran");
    assert!(
        parsed.is_ok(),
        "bundle does not parse: {:?}\n{code}",
        parsed.diagnostics
    );
}

#[test]
fn an_export_nothing_imports_is_shaken_out() {
    let (_dir, output) = bundled(
        &[
            (
                "main.js",
                "// @flow\nimport { used } from \"./both.js\";\nconsole.log(used());\n",
            ),
            (
                "both.js",
                "// @flow\nexport function used(): string {\n  return \"kept\";\n}\nexport function unused(): string {\n  return \"dropped\";\n}\n",
            ),
        ],
        "main.js",
    );

    let code = only_chunk(&output);
    assert!(
        code.contains("kept"),
        "the used export was dropped:\n{code}"
    );
    assert!(
        !code.contains("dropped"),
        "the unused export survived:\n{code}"
    );
}

#[test]
fn jsx_is_lowered_to_the_automatic_runtime() {
    let (_dir, output) = bundled(
        &[(
            "main.js",
            "// @flow\nexport function App(): mixed {\n  return <main className=\"a\">hi</main>;\n}\nconsole.log(App());\n",
        )],
        "main.js",
    );

    let code = only_chunk(&output);
    assert!(!code.contains("<main"), "JSX survived lowering:\n{code}");
    let parsed = uf_flow::validate_source(code).expect("parser ran");
    assert!(
        parsed.is_ok(),
        "lowered bundle does not parse: {:?}\n{code}",
        parsed.diagnostics
    );
}

#[test]
fn the_rsc_directive_prologue_is_blanked() {
    let (_dir, output) = bundled(
        &[(
            "main.js",
            "\"use client\";\n// @flow\nexport function widget(): string {\n  return \"w\";\n}\nconsole.log(widget());\n",
        )],
        "main.js",
    );

    let code = only_chunk(&output);
    assert!(
        !code.contains("\"use client\""),
        "the directive reached the bundle:\n{code}"
    );
    let parsed = uf_flow::validate_source(code).expect("parser ran");
    assert!(
        parsed.is_ok(),
        "bundle does not parse: {:?}\n{code}",
        parsed.diagnostics
    );
}

#[test]
fn rolldowns_own_runtime_modules_are_not_handed_to_the_flow_eraser() {
    // A `require` shim Rolldown synthesises is not the user's source. Erasing
    // Flow types from it turned its own helpers into a syntax error, and the
    // failure surfaced as a parse error in a file nobody wrote.
    let (_dir, output) = bundled(
        &[(
            "main.js",
            "// @flow\nconst pick = (a: mixed, b: mixed): mixed => (typeof a !== \"undefined\" ? a : b);\nconsole.log(pick(1, 2));\n",
        )],
        "main.js",
    );

    let code = only_chunk(&output);
    let parsed = uf_flow::validate_source(code).expect("parser ran");
    assert!(
        parsed.is_ok(),
        "bundle does not parse: {:?}\n{code}",
        parsed.diagnostics
    );
    assert!(
        code.contains("typeof a"),
        "the ternary was mangled:\n{code}"
    );
}

#[test]
fn a_missing_entry_is_an_error_rather_than_an_empty_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let mut options = BundleOptions::new(&root, root.join("dist"));
    options.entries = vec![Utf8PathBuf::from("nope.js")];

    let result = bundle(&options, &UniflowedConfig::default(), &[]);

    assert!(result.is_err(), "a missing entry produced a bundle");
}
