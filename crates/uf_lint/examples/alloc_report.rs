//! What `uf lint` costs, in allocations and bytes, for ubugeeei-prod/uf#668.
//!
//! The counterpart to `uf_transform`'s example of the same name, one level up:
//! that one measures the Babel tree, this one measures the linter that asks for
//! it. Run it as
//! `cargo run --release --example alloc_report -p uf_lint -- <file>`.

use uf_config::UniflowedConfig;
use uf_lint::SourceFile;
use uf_profiler::{AllocSnapshot, CountingAllocator, Window};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("packages/router/internal/runtime.js"));
    let source = std::fs::read_to_string(&path).expect("read the module");
    let file = SourceFile {
        path: path.clone(),
        source,
    };
    let config = UniflowedConfig::default();
    println!("{path}: {} bytes", file.source.len());

    // Warm up: the first run pays for whatever is initialised once.
    let _ = uf_lint::lint_source(&file, &config).expect("lints");

    CountingAllocator::enable();
    let window = Window::open();
    let before = AllocSnapshot::capture();
    let runs = 5;
    for _ in 0..runs {
        let report = uf_lint::lint_source(&file, &config).expect("lints");
        std::hint::black_box(&report);
    }
    let after = AllocSnapshot::capture();
    drop(window);
    CountingAllocator::disable();

    let delta = after.delta_from(&before);
    println!("allocs / run   {}", delta.allocations / runs);
    println!(
        "bytes / run    {:.2} MiB",
        delta.bytes_allocated as f64 / runs as f64 / (1024.0 * 1024.0)
    );
    println!(
        "peak           {:.2} MiB",
        delta.peak_above_baseline as f64 / (1024.0 * 1024.0)
    );
}
