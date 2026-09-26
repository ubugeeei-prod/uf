#![allow(clippy::disallowed_macros)]

//! Allocation guards for the batch dependency graph.
//!
//! A fully warm check still has to rebuild the graph that says which cached
//! answer belongs to this batch. The graph must scale with the edges it keeps,
//! not with one heap allocation per module in the batch.

#![cfg(feature = "upstream-typecheck")]

use tempfile::TempDir;
use uf_check::{CheckCache, CheckLimits, Source, check_sources_cached};
use uf_profiler::{CountingAllocator, ThreadWindow};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

const MODULES: usize = 512;

/// Above the flattened graph's current warm-hit cost with room for JSON,
/// profile, and allocator drift. The exact "one buffer, not one buffer per
/// module" invariant is covered by `upstream::graph`'s unit test; this keeps a
/// full warm check from quietly drifting back into graph-heavy allocation.
const IMPORT_CHAIN_CEILING: u64 = 10_400;

#[test]
fn a_warm_import_chain_does_not_allocate_one_resolution_list_per_module() {
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).expect("this process can name its own binary");
    let limits = CheckLimits::default().without_timeout();
    let paths: Vec<String> = (0..MODULES)
        .map(|index| format!("module{index}.js"))
        .collect();
    let texts: Vec<String> = (0..MODULES).map(module_source).collect();
    let sources: Vec<Source<'_>> = paths
        .iter()
        .zip(&texts)
        .map(|(path, source)| Source::new(path, source))
        .collect();

    let cold = check_sources_cached(&sources, &[], &limits, Some(&cache)).expect("checks");
    assert_eq!(cold.files_from_cache, 0);

    // This thread, and the check thread `uf_check` runs the batch on, which
    // hands its allocations back — not any other thread in the binary. See
    // `uf_profiler::Handover`.
    let window = ThreadWindow::open();
    let warm = check_sources_cached(&sources, &[], &limits, Some(&cache)).expect("checks");
    let delta = window.close();

    assert_eq!(
        warm.files_from_cache, MODULES,
        "the cache should answer the whole import chain"
    );
    assert!(warm.diagnostics.is_empty(), "{:#?}", warm.diagnostics);

    assert!(
        delta.allocations <= IMPORT_CHAIN_CEILING,
        "a warm {MODULES}-module import chain took {} allocations, over the \
         {IMPORT_CHAIN_CEILING} ceiling. That usually means graph resolution \
         storage is allocating per module again.",
        delta.allocations,
    );
}

fn module_source(index: usize) -> String {
    if index + 1 == MODULES {
        return "// @flow\nexport type Value = number;\nexport const value: Value = 1;\n"
            .to_owned();
    }
    format!(
        "// @flow\nimport type {{ Value as Next }} from \"./module{}.js\";\n\
         export type Value = Next;\nexport const value: Value = 1;\n",
        index + 1
    )
}
