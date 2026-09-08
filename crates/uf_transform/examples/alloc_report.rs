//! What `babel_ast` costs, in allocations and bytes, for ubugeeei-prod/uf#668.
//!
//! Not a test and not a bench: a number to put in a commit message, and the
//! thing to re-run after a change to see whether it moved. Run it as
//! `cargo run --release --example alloc_report -p uf_transform -- <file>`.

use uf_profiler::{AllocSnapshot, CountingAllocator, Window};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("packages/router/internal/runtime.js"));
    let source = std::fs::read_to_string(&path).expect("read the module");

    // `--dump`: the tree itself, so a change meant only to cost less can be
    // held against the tree it used to produce. `serde_json` is built with
    // `preserve_order`, so this is sensitive to key order as well as to
    // content — which is how the one difference this change does make was
    // found rather than assumed away.
    if std::env::args().any(|arg| arg == "--dump") {
        let (file, _) = uf_transform::babel_ast(&source).expect("parses");
        println!("{}", serde_json::to_string(&file).expect("serializes"));
        return;
    }

    println!("{path}: {} bytes", source.len());

    // Warm up: the first run pays for whatever the parser initialises once.
    let _ = uf_transform::babel_ast(&source).expect("parses");

    CountingAllocator::enable();
    let window = Window::open();
    let before = AllocSnapshot::capture();
    let runs = 5;
    for _ in 0..runs {
        let built = uf_transform::babel_ast(&source).expect("parses");
        std::hint::black_box(&built);
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
