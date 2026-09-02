//! Exactness of the invalidation walk.
//!
//! Each test names one rule from the module documentation. The two that matter
//! most for the product claim are `an_unrelated_edit_invalidates_nothing` and
//! `a_shared_dependency_edit_invalidates_both_dependents`: the first is what
//! keeps a hot update small, the second is what keeps it correct.

use super::*;
use crate::hmr::graph::DevGraph;

mod cases;
mod refresh;
mod verdict;

fn graph_with(sources: &[(&str, &str)]) -> DevGraph {
    let mut graph = DevGraph::new();
    for (path, source) in sources {
        graph.insert(path, source).expect("fixture module inserts");
    }
    graph
}

fn changed(graph: &DevGraph, path: &str) -> Invalidation {
    let id = graph.find(path).expect("module is in the graph");
    invalidate(graph, id, ChangeKind::Modified)
}

fn paths(graph: &DevGraph, ids: &[DevModuleId]) -> Vec<String> {
    ids.iter()
        .map(|id| graph.module(*id).expect("id resolves").path().to_string())
        .collect()
}

/// A route: a server module with no importers.
const ROUTE: &str = "// @flow\nimport Counter from \"./Counter.js\";\n\
                     import { helper } from \"./util.js\";\n\
                     export default function () { return null; }\n";

/// A `"use client"` component: a Fast Refresh boundary.
const COUNTER: &str = "\"use client\";\n// @flow\nimport { helper } from \"./util.js\";\n\
                       export function Counter() { return null; }\n";

/// A shared helper with no directive.
const UTIL: &str = "// @flow\nexport function helper() { return 1; }\n";

/// A module whose whole surface is erased before the browser sees it.
const TYPES: &str = "// @flow\nexport type User = { id: string };\n";
