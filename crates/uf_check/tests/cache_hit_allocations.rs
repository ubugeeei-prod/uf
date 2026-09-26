#![allow(clippy::disallowed_macros)]

//! What a full check-cache hit is allowed to cost.
//!
//! A warm run has to describe the batch, read the records and replay their
//! diagnostics. It must not rebuild the per-call builtin environment that
//! inference needs, because a run answered entirely from disk never reaches
//! inference. ubugeeei-prod/uf#678 measured that environment at about 136,000
//! allocations; this test sits far below that and far above the current warm
//! cache cost, so it catches the fixed cost coming back without pinning every
//! JSON or path allocation in the cache reader.
//!
//! # What each figure counts
//!
//! This test's thread, and the check thread `uf_check` runs every call on,
//! which hands its allocations back (`uf_profiler::Handover`). Nothing else.
//! The five tests here run at once, and three of them start with the heaviest
//! work in the binary — a cold check, or preparing the builtins — while the
//! others measure. Read from the allocator's process-wide counters, as these
//! once were, a figure was the measured call plus whatever those three did in
//! the same stretch, and the extensionless closure below read 12,346 on a CI
//! run that changed nothing it measures (ubugeeei-prod/uf#1015).

#![cfg(feature = "upstream-typecheck")]

use tempfile::TempDir;
use uf_check::{
    CheckCache, CheckLimits, Source, check_sources_cached, module_closure, prepare_builtins,
};
use uf_profiler::{CountingAllocator, ThreadWindow};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

/// Above the current cost by an order of magnitude, below rebuilding the
/// environment by one.
const CEILING: u64 = 20_000;

/// Above the current cost with room for JSON and platform allocator drift, and
/// below the old cost that rebuilt cache-key strings per file.
const BATCH_CEILING: u64 = 4_100;

/// Above the parser's own broken-file cost, below that cost plus rebuilding
/// the per-call builtin environment.
const PARSE_ERROR_CEILING: u64 = 500_000;

/// Above a two-file closure walk, below #678's fixed per-call environment.
const RELATIVE_CLOSURE_CEILING: u64 = 100_000;

/// Above the nightly/all-features extensionless-miss closure cost, below the
/// fixed environment cost #678 is about. `resolve::tests` keeps the narrower
/// per-fallback candidate allocation guard.
const EXTENSIONLESS_MISS_CLOSURE_CEILING: u64 = 10_000;

#[test]
fn a_full_cache_hit_does_not_rebuild_the_check_environment() {
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).expect("this process can name its own binary");
    let limits = CheckLimits::default().without_timeout();
    let sources = [Source::new(
        "app.js",
        "// @flow\nexport const answer: number = 42;\n",
    )];

    let cold = check_sources_cached(&sources, &[], &limits, Some(&cache)).expect("checks");
    assert_eq!(cold.files_from_cache, 0);

    let window = ThreadWindow::open();
    let warm = check_sources_cached(&sources, &[], &limits, Some(&cache)).expect("checks");
    let delta = window.close();

    assert_eq!(warm.files_from_cache, 1, "the cache should answer the file");
    assert!(warm.diagnostics.is_empty(), "{:#?}", warm.diagnostics);

    assert!(
        delta.allocations <= CEILING,
        "a full cache hit took {} allocations, over the {CEILING} ceiling. \
         That usually means check::environment was rebuilt even though every \
         file was answered from the cache.",
        delta.allocations,
    );
}

#[test]
fn a_full_cache_hit_reuses_dependency_walk_storage_across_the_batch() {
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).expect("this process can name its own binary");
    let limits = CheckLimits::default().without_timeout();
    let paths: Vec<String> = (0..64).map(|index| format!("module{index}.js")).collect();
    let sources: Vec<Source<'_>> = paths
        .iter()
        .map(|path| Source::new(path, "// @flow\nexport const answer: number = 42;\n"))
        .collect();

    let cold = check_sources_cached(&sources, &[], &limits, Some(&cache)).expect("checks");
    assert_eq!(cold.files_from_cache, 0);

    let window = ThreadWindow::open();
    let warm = check_sources_cached(&sources, &[], &limits, Some(&cache)).expect("checks");
    let delta = window.close();

    assert_eq!(
        warm.files_from_cache, 64,
        "the cache should answer the whole batch"
    );
    assert!(warm.diagnostics.is_empty(), "{:#?}", warm.diagnostics);

    assert!(
        delta.allocations <= BATCH_CEILING,
        "a 64-file full cache hit took {} allocations, over the {BATCH_CEILING} ceiling. \
         That usually means per-file dependency-digest storage is being allocated again.",
        delta.allocations,
    );
}

#[test]
fn a_parse_error_cache_miss_does_not_build_the_check_environment() {
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).expect("this process can name its own binary");
    let limits = CheckLimits::default().without_timeout();
    let sources = [Source::new("app.js", "// @flow\nfunction (\n")];

    // Keep this test about the per-call environment, not the process-global
    // master context that is shared after the first preparation.
    prepare_builtins(&[]).expect("builtins prepare");

    let window = ThreadWindow::open();
    let report = check_sources_cached(&sources, &[], &limits, Some(&cache)).expect("checks");
    let delta = window.close();

    assert_eq!(
        report.files_from_cache, 0,
        "there is no record for the broken edit"
    );
    assert!(
        !report.diagnostics.is_empty(),
        "the parse error should still be reported"
    );

    assert!(
        delta.allocations <= PARSE_ERROR_CEILING,
        "a parse-error cache miss took {} allocations, over the {PARSE_ERROR_CEILING} ceiling. \
         That usually means check::environment was rebuilt before the parser found there was \
         no file to infer.",
        delta.allocations,
    );
}

#[test]
fn a_relative_only_module_closure_does_not_build_the_check_environment() {
    let limits = CheckLimits::default().without_timeout();
    let sources = [
        Source::new("app.js", "// @flow\nimport { value } from './value.js';\n"),
        Source::new("value.js", "// @flow\nexport const value: number = 42;\n"),
    ];

    let window = ThreadWindow::open();
    let closure = module_closure(&["app.js"], &sources, &[], &limits).expect("closure");
    let delta = window.close();

    assert_eq!(
        closure
            .sources
            .iter()
            .map(|source| source.path)
            .collect::<Vec<_>>(),
        ["app.js", "value.js"]
    );
    assert!(
        closure.builtins.is_none(),
        "relative-only closure should not merge builtins"
    );

    assert!(
        delta.allocations <= RELATIVE_CLOSURE_CEILING,
        "a relative-only closure took {} allocations, over the {RELATIVE_CLOSURE_CEILING} \
         ceiling. That usually means the walk rebuilt check::environment before seeing an \
         import that needed Flow's libdefs.",
        delta.allocations,
    );
}

#[test]
fn an_extensionless_missing_closure_reuses_resolution_candidate_storage() {
    let limits = CheckLimits::default().without_timeout();
    let imports = (0..128)
        .map(|index| format!("import './missing{index}';\n"))
        .collect::<String>();
    let source = format!("// @flow\n{imports}");
    let sources = [Source::new("app.js", &source)];

    let window = ThreadWindow::open();
    let closure = module_closure(&["app.js"], &sources, &[], &limits).expect("closure");
    let delta = window.close();

    assert_eq!(
        closure
            .sources
            .iter()
            .map(|source| source.path)
            .collect::<Vec<_>>(),
        ["app.js"]
    );
    assert_eq!(closure.unresolved.len(), 128);
    assert!(
        closure.builtins.is_none(),
        "extensionless relative misses should not merge builtins"
    );

    assert!(
        delta.allocations <= EXTENSIONLESS_MISS_CLOSURE_CEILING,
        "an extensionless-missing closure took {} allocations, over the \
         {EXTENSIONLESS_MISS_CLOSURE_CEILING} ceiling. That usually means path resolution is \
         allocating a fresh candidate for every extension and index fallback.",
        delta.allocations,
    );
}
