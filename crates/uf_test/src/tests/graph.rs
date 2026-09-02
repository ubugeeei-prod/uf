//! What one edit invalidates.

use crate::ImportGraph;

/// A three-module project: two tests, one shared helper, one unrelated helper.
fn project() -> Vec<(&'static str, &'static str)> {
    vec![
        ("src/shared.js", "export const shared = 1;\n"),
        ("src/lonely.js", "export const lonely = 1;\n"),
        (
            "src/a.test.js",
            "import { shared } from './shared.js';\nit('a', () => {});\n",
        ),
        (
            "src/b.test.js",
            "import { shared } from './shared.js';\nit('b', () => {});\n",
        ),
        (
            "src/c.test.js",
            "import { thing } from './unrelated.js';\nit('c', () => {});\n",
        ),
        ("src/unrelated.js", "export const thing = 1;\n"),
    ]
}

fn is_test(path: &str) -> bool {
    path.ends_with(".test.js")
}

#[test]
fn an_unrelated_edit_reruns_nothing() {
    let graph = ImportGraph::build(project());
    let affected = graph.affected_tests(["src/lonely.js"], is_test);

    assert!(
        affected.is_empty(),
        "editing a module nothing imports must rerun no tests, got {affected:?}"
    );
}

#[test]
fn a_shared_dependency_edit_reruns_both_dependents() {
    let graph = ImportGraph::build(project());
    let affected = graph.affected_tests(["src/shared.js"], is_test);

    similar_asserts::assert_eq!(
        affected.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        vec!["src/a.test.js", "src/b.test.js"]
    );
}

#[test]
fn editing_a_test_file_reruns_only_that_test() {
    let graph = ImportGraph::build(project());
    let affected = graph.affected_tests(["src/a.test.js"], is_test);

    similar_asserts::assert_eq!(
        affected.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        vec!["src/a.test.js"]
    );
}

#[test]
fn invalidation_is_transitive_through_a_chain() {
    let graph = ImportGraph::build([
        ("src/deep.js", "export const deep = 1;\n"),
        ("src/mid.js", "import { deep } from './deep.js';\n"),
        ("src/top.js", "import { mid } from './mid.js';\n"),
        ("src/a.test.js", "import { top } from './top.js';\n"),
    ]);

    let affected = graph.affected_tests(["src/deep.js"], is_test);
    similar_asserts::assert_eq!(
        affected.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        vec!["src/a.test.js"]
    );
}

#[test]
fn a_cycle_does_not_loop_forever() {
    let graph = ImportGraph::build([
        ("src/a.js", "import { b } from './b.js';\n"),
        ("src/b.js", "import { a } from './a.js';\n"),
        ("src/a.test.js", "import { a } from './a.js';\n"),
    ]);

    let affected = graph.affected(["src/a.js"]);
    assert_eq!(affected.len(), 3);
}

#[test]
fn a_changed_file_is_always_in_its_own_affected_set() {
    let graph = ImportGraph::build(project());
    assert!(
        graph
            .affected(["src/shared.js"])
            .iter()
            .any(|path| path == "src/shared.js")
    );
}

#[test]
fn an_extensionless_specifier_resolves() {
    let graph = ImportGraph::build([
        ("src/util.js", "export const u = 1;\n"),
        ("src/a.test.js", "import { u } from './util';\n"),
    ]);

    similar_asserts::assert_eq!(
        graph
            .affected_tests(["src/util.js"], is_test)
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["src/a.test.js"]
    );
}

#[test]
fn a_directory_index_specifier_resolves() {
    let graph = ImportGraph::build([
        ("src/util/index.js", "export const u = 1;\n"),
        ("src/a.test.js", "import { u } from './util';\n"),
    ]);

    assert_eq!(
        graph.affected_tests(["src/util/index.js"], is_test).len(),
        1
    );
}

#[test]
fn a_parent_directory_specifier_resolves() {
    let graph = ImportGraph::build([
        ("src/shared.js", "export const s = 1;\n"),
        ("src/ui/a.test.js", "import { s } from '../shared.js';\n"),
    ]);

    similar_asserts::assert_eq!(
        graph
            .affected_tests(["src/shared.js"], is_test)
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["src/ui/a.test.js"]
    );
}

#[test]
fn a_jsx_specifier_resolves() {
    let graph = ImportGraph::build([
        ("src/Button.jsx", "export const Button = 1;\n"),
        ("src/a.test.js", "import { Button } from './Button';\n"),
    ]);

    assert_eq!(graph.affected_tests(["src/Button.jsx"], is_test).len(), 1);
}

#[test]
fn a_bare_package_specifier_is_not_a_project_edge() {
    let graph = ImportGraph::build([
        ("src/a.test.js", "import React from 'react';\n"),
        ("react", "// not a project module\n"),
    ]);

    assert!(graph.affected_tests(["react"], is_test).is_empty());
}

#[test]
fn a_dynamic_import_is_an_edge() {
    let graph = ImportGraph::build([
        ("src/lazy.js", "export const lazy = 1;\n"),
        (
            "src/a.test.js",
            "it('a', async () => { await import('./lazy.js'); });\n",
        ),
    ]);

    assert_eq!(graph.affected_tests(["src/lazy.js"], is_test).len(), 1);
}

#[test]
fn a_require_call_is_an_edge() {
    let graph = ImportGraph::build([
        ("src/legacy.js", "module.exports = 1;\n"),
        ("src/a.test.js", "const legacy = require('./legacy.js');\n"),
    ]);

    assert_eq!(graph.affected_tests(["src/legacy.js"], is_test).len(), 1);
}

#[test]
fn a_re_export_is_an_edge() {
    let graph = ImportGraph::build([
        ("src/inner.js", "export const inner = 1;\n"),
        ("src/a.test.js", "export * from './inner.js';\n"),
    ]);

    assert_eq!(graph.affected_tests(["src/inner.js"], is_test).len(), 1);
}

#[test]
fn a_specifier_pointing_at_nothing_is_dropped() {
    let graph = ImportGraph::build([("src/a.test.js", "import x from './missing.js';\n")]);
    assert_eq!(graph.affected(["src/missing.js"]).len(), 1);
    assert_eq!(graph.affected_tests(["src/missing.js"], is_test).len(), 0);
}

#[test]
fn a_module_added_later_closes_the_edge_that_pointed_at_it() {
    let mut graph = ImportGraph::build([("src/a.test.js", "import x from './late.js';\n")]);
    assert!(graph.affected_tests(["src/late.js"], is_test).is_empty());

    graph.insert("src/late.js", "export default 1;\n");
    assert_eq!(graph.affected_tests(["src/late.js"], is_test).len(), 1);
}

#[test]
fn editing_a_module_replaces_only_its_own_edges() {
    let mut graph = ImportGraph::build(project());
    graph.insert("src/a.test.js", "it('a', () => {});\n");

    let affected = graph.affected_tests(["src/shared.js"], is_test);
    similar_asserts::assert_eq!(
        affected.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        vec!["src/b.test.js"]
    );
}

#[test]
fn removing_a_module_removes_its_edges() {
    let mut graph = ImportGraph::build(project());
    graph.remove("src/a.test.js");

    assert!(!graph.contains("src/a.test.js"));
    similar_asserts::assert_eq!(
        graph
            .affected_tests(["src/shared.js"], is_test)
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["src/b.test.js"]
    );
}

#[test]
fn a_graph_reports_the_dependencies_of_a_module() {
    let graph = ImportGraph::build(project());
    similar_asserts::assert_eq!(
        graph
            .dependencies_of("src/a.test.js")
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["src/shared.js"]
    );
}

#[test]
fn an_empty_graph_invalidates_only_what_changed() {
    let graph = ImportGraph::new();
    assert!(graph.is_empty());
    assert_eq!(graph.len(), 0);
    assert_eq!(graph.affected(["src/a.js"]).len(), 1);
}

#[test]
fn the_affected_set_is_sorted_and_deduplicated() {
    let graph = ImportGraph::build(project());
    let affected = graph.affected(["src/shared.js", "src/shared.js", "src/lonely.js"]);

    let mut sorted = affected.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(affected, sorted);
}

#[test]
fn invalidation_does_not_depend_on_the_order_of_the_changed_list() {
    let graph = ImportGraph::build(project());
    let forwards = graph.affected(["src/shared.js", "src/lonely.js"]);
    let backwards = graph.affected(["src/lonely.js", "src/shared.js"]);
    assert_eq!(forwards, backwards);
}

#[test]
fn a_source_past_the_size_limit_contributes_no_edges() {
    let big = "x".repeat(crate::graph::MAX_SOURCE_BYTES + 1);
    let mut graph = ImportGraph::new();
    graph.insert("src/huge.js", &big);
    graph.insert("src/other.js", "export const o = 1;\n");

    assert_eq!(graph.dependencies_of("src/huge.js").len(), 0);
}

#[test]
fn a_module_with_an_unsafe_path_is_not_recorded() {
    let mut graph = ImportGraph::new();
    graph.insert("../escape.js", "export const e = 1;\n");
    graph.insert("/etc/passwd", "export const e = 1;\n");

    assert!(graph.is_empty());
}
