use super::*;
use crate::directive::{FunctionOwner, ModuleEnvironment};
use crate::graph::{EntryKind, RscGraph, RscGraphBuilder, RscModuleInput};
use crate::scan::ExportKind;

mod action_id;
mod build_id;
mod crypto;
mod registry;

fn build_id() -> BuildId {
    BuildId::new("build-id-for-tests").expect("valid build id")
}

fn graph_with_reachable_action() -> RscGraph {
    let mut builder = RscGraphBuilder::new();
    builder.add_source(
        "app/page.js",
        "import Counter from \"./Counter.js\";\nimport { refresh } from \"../server/actions.js\";\n",
    );
    builder.add_source("app/Counter.js", "\"use client\";\n");
    builder.add_source(
        "server/actions.js",
        "\"use server\";\nexport async function refresh() {}\n",
    );
    builder.add_entry("app/page.js", EntryKind::Server);
    builder.build()
}
