//! What a full check-cache hit is allowed to cost.
//!
//! A warm run has to describe the batch, read the records and replay their
//! diagnostics. It must not rebuild the per-call builtin environment that
//! inference needs, because a run answered entirely from disk never reaches
//! inference. ubugeeei-prod/uf#678 measured that environment at about 136,000
//! allocations; this test sits far below that and far above the current warm
//! cache cost, so it catches the fixed cost coming back without pinning every
//! JSON or path allocation in the cache reader.

#![cfg(feature = "upstream-typecheck")]

use tempfile::TempDir;
use uf_check::{CheckCache, CheckLimits, Source, check_sources_cached};
use uf_profiler::{AllocSnapshot, CountingAllocator, Window};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

/// Above the current cost by an order of magnitude, below rebuilding the
/// environment by one.
const CEILING: u64 = 20_000;

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

    let _window = Window::open();
    CountingAllocator::enable();
    let before = AllocSnapshot::capture();
    let warm = check_sources_cached(&sources, &[], &limits, Some(&cache)).expect("checks");
    let after = AllocSnapshot::capture();
    CountingAllocator::disable();

    assert_eq!(warm.files_from_cache, 1, "the cache should answer the file");
    assert!(warm.diagnostics.is_empty(), "{:#?}", warm.diagnostics);

    let delta = after.delta_from(&before);
    assert!(
        delta.allocations <= CEILING,
        "a full cache hit took {} allocations, over the {CEILING} ceiling. \
         That usually means check::environment was rebuilt even though every \
         file was answered from the cache.",
        delta.allocations,
    );
}
