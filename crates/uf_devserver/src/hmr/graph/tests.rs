//! Behaviour of the incremental dev module graph.
//!
//! The fixtures and helpers live here; the assertions live one file per topic
//! beside them.

use super::*;

use uf_rsc::ModuleEnvironment;

mod bounds;
mod edges;
mod surface;

/// A `"use client"` module whose only export is a component.
const CLIENT_COMPONENT: &str =
    "\"use client\";\n// @flow\nexport function Counter() { return null; }\n";

/// A `"use client"` module exporting something Fast Refresh cannot swap.
const CLIENT_CONSTANT: &str = "\"use client\";\n// @flow\nexport const LIMIT = 10;\n";

/// A module with no directive and one plain export.
const SERVER_HELPER: &str = "// @flow\nexport function helper() { return 1; }\n";

/// A module whose exports are all erased before the browser sees it.
const TYPES_ONLY: &str = "// @flow\nexport type User = { id: string };\n";

fn graph_with(sources: &[(&str, &str)]) -> DevGraph {
    let mut graph = DevGraph::new();
    for (path, source) in sources {
        graph.insert(path, source).expect("fixture module inserts");
    }
    graph
}

fn module(graph: &DevGraph, path: &str) -> DevModule {
    let id = graph.find(path).expect("module is in the graph");
    graph.module(id).expect("id resolves").clone()
}

fn import_paths(graph: &DevGraph, path: &str) -> Vec<String> {
    module(graph, path)
        .imports()
        .iter()
        .map(|id| graph.module(*id).expect("edge target").path().to_string())
        .collect()
}

fn importer_paths(graph: &DevGraph, path: &str) -> Vec<String> {
    module(graph, path)
        .importers()
        .iter()
        .map(|id| graph.module(*id).expect("edge source").path().to_string())
        .collect()
}
