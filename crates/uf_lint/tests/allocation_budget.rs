//! Allocation guard for ubugeeei-prod/uf#668.
//!
//! The issue was filed from `packages/router/internal/runtime.js`, which used
//! to make `uf lint` build enough Babel-shaped tree data to allocate hundreds
//! of thousands of times. The exact number may move as the router grows, but
//! this file must stay far below the old `serde_json::Value` pipeline cost.

use uf_config::UniflowedConfig;
use uf_lint::SourceFile;
use uf_profiler::{AllocSnapshot, CountingAllocator, Window};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

/// Above the current cost on `main` with room for allocator and fixture drift,
/// below the 293,000-allocation post-#708 figure from #668.
const RUNTIME_JS_ALLOCATIONS_PER_KIB_CEILING: u64 = 256;

/// Above the current 4.14 MiB run with room for fixture drift, below the
/// 32 MiB post-#708 figure from #668.
const RUNTIME_JS_BYTES_PER_BYTE_CEILING: u64 = 40;

#[test]
fn runtime_js_lint_stays_below_the_babel_tree_allocation_budget() {
    let path = runtime_fixture();
    let source = std::fs::read_to_string(&path).expect("read router runtime fixture");
    let file = SourceFile {
        path: "packages/router/internal/runtime.js".to_owned(),
        source,
    };
    let config = UniflowedConfig::default();

    // Warm process-global parser/compiler state before measuring the lint path
    // itself. The issue's `alloc_report` example does the same.
    let warm = uf_lint::lint_source(&file, &config).expect("warm lint");
    assert!(
        warm.diagnostics.is_empty(),
        "the fixture under measurement should lint cleanly, got {:#?}",
        warm.diagnostics
    );

    let _window = Window::open();
    CountingAllocator::enable();
    let before = AllocSnapshot::capture();
    let report = uf_lint::lint_source(&file, &config).expect("measured lint");
    let after = AllocSnapshot::capture();
    CountingAllocator::disable();

    assert!(
        report.diagnostics.is_empty(),
        "the fixture under measurement should lint cleanly, got {:#?}",
        report.diagnostics
    );

    let delta = after.delta_from(&before);
    let source_bytes = u64::try_from(file.source.len()).expect("source length fits in u64");
    let source_kib = source_bytes.div_ceil(1024);
    let allocation_ceiling = source_kib * RUNTIME_JS_ALLOCATIONS_PER_KIB_CEILING;
    let byte_ceiling = source_bytes * RUNTIME_JS_BYTES_PER_BYTE_CEILING;
    assert!(
        delta.allocations <= allocation_ceiling,
        "linting {} bytes of router runtime took {} allocations, over the \
         {allocation_ceiling} ceiling ({RUNTIME_JS_ALLOCATIONS_PER_KIB_CEILING} \
         allocations per KiB). This usually means the #668 gate stopped \
         skipping Babel/ESTree work that cannot produce a lint finding. Run \
         `cargo run --release --example alloc_report -p uf_lint -- \
         packages/router/internal/runtime.js --phases` to find the phase.",
        source_bytes,
        delta.allocations,
    );
    assert!(
        delta.bytes_allocated <= byte_ceiling,
        "linting {} bytes of router runtime allocated {} bytes, over the \
         {byte_ceiling} ceiling ({RUNTIME_JS_BYTES_PER_BYTE_CEILING} bytes per \
         source byte). Run `cargo run --release --example alloc_report -p \
         uf_lint -- packages/router/internal/runtime.js --phases` to find the \
         phase.",
        source_bytes,
        delta.bytes_allocated,
    );
}

fn runtime_fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/router/internal/runtime.js")
}
