//! Allocation guard for ubugeeei-prod/uf#678.
//!
//! The issue was filed from `npm/router/internal/runtime.js`, where a cold
//! check once allocated about two million times and more than 330 MiB. The
//! exact number may drift with the router fixture, but it must stay below the
//! pre-SSA-normal-form-fix cost. Warm cache hits and graph/cache allocation
//! shapes have their own tests; this one guards the cold path that CI and clean
//! checkouts still pay.

#![cfg(feature = "upstream-typecheck")]

use uf_check::{CheckLimits, Source, check_sources};
use uf_profiler::{CountingAllocator, ThreadWindow};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

/// Above the current cost with room for fixture and allocator drift, below the
/// 17.3 allocations/source-byte figure measured before the SSA normal-form
/// patch. The router runtime grows often, so the guard is per byte rather than
/// an absolute number that goes stale as the fixture changes.
const RUNTIME_JS_ALLOCATIONS_PER_BYTE_CEILING: u64 = 17;

/// Above the current cost with room for fixture and allocator drift, below the
/// 3 KiB/source-byte figure measured before the SSA normal-form patch.
const RUNTIME_JS_BYTES_PER_BYTE_CEILING: u64 = 2_950;

#[test]
fn runtime_js_check_stays_below_the_cold_allocation_budget() {
    let path = runtime_fixture();
    let source = std::fs::read_to_string(&path).expect("read router runtime fixture");
    let sources = [Source::new("npm/router/internal/runtime.js", &source)];
    let limits = CheckLimits::default().without_timeout();

    // Warm process-global parser/compiler state before measuring the check
    // path itself. The issue's `alloc_report` example does the same, so this
    // guard tracks the cold module check rather than one-time process setup.
    let warm = check_sources(&sources, &[], &limits).expect("warm check");
    assert_eq!(warm.files_checked, 1, "the fixture should opt in to Flow");

    // This thread, and the check thread `uf_check` runs the module on, which
    // hands its allocations back — not any other thread in the binary. See
    // `uf_profiler::Handover`.
    let window = ThreadWindow::open();
    let report = check_sources(&sources, &[], &limits).expect("measured check");
    let delta = window.close();

    assert_eq!(report.files_checked, 1, "the fixture should be checked");

    let source_bytes = u64::try_from(source.len()).expect("source length fits in u64");
    let allocation_ceiling = source_bytes * RUNTIME_JS_ALLOCATIONS_PER_BYTE_CEILING;
    let byte_ceiling = source_bytes * RUNTIME_JS_BYTES_PER_BYTE_CEILING;
    assert!(
        delta.allocations <= allocation_ceiling,
        "checking {} bytes of router runtime took {} allocations, over the \
         {allocation_ceiling} ceiling of {RUNTIME_JS_ALLOCATIONS_PER_BYTE_CEILING} \
         allocations per source byte. This usually means the #678 SSA normal-form \
         allocation fix regressed. Run `cargo run --release --example alloc_report \
         -p uf_check -- npm/router/internal/runtime.js --phases` to find the phase.",
        source.len(),
        delta.allocations
    );
    assert!(
        delta.bytes_allocated <= byte_ceiling,
        "checking {} bytes of router runtime allocated {} bytes, over the \
         {byte_ceiling} ceiling of {RUNTIME_JS_BYTES_PER_BYTE_CEILING} bytes per \
         source byte. Run `cargo run --release --example alloc_report -p uf_check \
         -- npm/router/internal/runtime.js --phases` to find the phase.",
        source.len(),
        delta.bytes_allocated
    );
}

fn runtime_fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../npm/router/internal/runtime.js")
}
