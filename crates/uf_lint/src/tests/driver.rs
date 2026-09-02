//! The runner itself: diagnostics come back sorted, linting the same file twice
//! gives the same answer, and the parallel path agrees with the sequential one.

use super::*;

#[test]
fn diagnostics_are_sorted_by_position() {
    let report = lint_source(
        &at("app/index.js", "// @flow\ntype A = any;\ntype B = bool;\n"),
        &UniflowedConfig::default(),
    )
    .expect("lint");

    let positions = report
        .diagnostics
        .iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column))
        .collect::<Vec<_>>();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted);
}

#[test]
fn linting_is_idempotent() {
    let file = at("app/index.js", "// @flow\ntype A = { v: any };\n");
    let config = UniflowedConfig::default();

    let first = lint_source(&file, &config).expect("lint");
    let second = lint_source(&file, &config).expect("lint");

    assert_eq!(first, second);
}

#[test]
fn parallel_and_sequential_linting_agree() {
    let files = vec![
        at("app/a.js", "// @flow\ntype A = any;\n"),
        at("app/b.js", "// @flow\ntype B = bool;\n"),
    ];
    let config = UniflowedConfig::default();

    let parallel = lint_sources(&files, &config).expect("lint");
    let mut sequential = files
        .iter()
        .flat_map(|file| lint_source(file, &config).expect("lint").diagnostics)
        .collect::<Vec<_>>();
    sort_diagnostics(&mut sequential);

    assert_eq!(parallel.diagnostics, sequential);
    assert_eq!(parallel.files_checked, 2);
}

/// Regression test for the QuickJS stack-budget trap.
///
/// Before `uf_flow::prepare_thread` was broadcast to the pool, linting a few
/// hundred perfectly valid files in parallel failed with
/// `Flow parser runtime error: SyntaxError: stack overflow`, because rayon ran
/// later jobs several frames deeper than the one that created each worker's
/// parser. The failure scaled with parallelism rather than with input size, so
/// a small project never saw it and a real one always did.
#[test]
fn lints_hundreds_of_files_in_parallel_without_exhausting_the_parser_stack() {
    let config = UniflowedConfig::default();
    let files = (0..800)
        .map(|index| SourceFile {
            path: format!("app/route{index}/_uf.page.js"),
            source: format!(
                "// @flow\ncomponent Page{index}() renders React.Node {{\n  return <main>hello</main>;\n}}\n"
            ),
        })
        .collect::<Vec<_>>();

    let report = lint_sources(&files, &config).expect("lint must not fail on valid sources");

    assert_eq!(report.files_checked, 800);
    let syntax = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule == "flow/syntax")
        .collect::<Vec<_>>();
    assert!(
        syntax.is_empty(),
        "unexpected syntax diagnostics: {syntax:?}"
    );
}
