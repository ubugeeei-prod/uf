//! What `uf lint` costs, in allocations and bytes, for ubugeeei-prod/uf#668.
//!
//! The counterpart to `uf_transform`'s example of the same name, one level up:
//! that one measures the Babel tree, this one measures the linter that asks for
//! it. Run it as
//! `cargo run --release --example alloc_report -p uf_lint -- <file>`.
//!
//! # The per-phase table
//!
//! The headline says how much; `--phases` says where. It turns
//! [`uf_profiler::scope`] on and prints one row per `profile_span!` in the
//! linter and in the transform pipeline underneath it, which is the table
//! ubugeeei-prod/uf#668 was reduced to taking by editing the source and
//! recompiling. The spans are compiled in unconditionally and cost a relaxed
//! atomic load each when the gate is shut, so the headline is measured with
//! them present either way and the two numbers are comparable.
//!
//! **The rows do not sum to the total, and the parent rows are inclusive.**
//! `run_react_tree_rules` parses on a thread of its own — `uf_flow` says why —
//! and a worker's spans reach the report through
//! [`uf_profiler::scope::flush_thread_spans`], which folds them in as roots
//! rather than as children of the span that spawned the thread. So
//! `estree::parse` and `lower::lower` appear both in their own rows and inside
//! `run_react_tree_rules`'s, and that row's `self` column is really its
//! inclusive time. Read a parent against the headline, and the children
//! against the parent.

use uf_config::UniflowedConfig;
use uf_lint::SourceFile;
use uf_profiler::{
    AllocSnapshot, CountingAllocator, Recorder, ReportConfig, SortBy, Window, scope,
};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

fn main() {
    let mut path = None;
    let mut phases = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--phases" => phases = true,
            other => path = Some(other.to_owned()),
        }
    }
    let path = path.unwrap_or_else(|| String::from("packages/router/internal/runtime.js"));
    let source = std::fs::read_to_string(&path).expect("read the module");
    let file = SourceFile {
        path: path.clone(),
        source,
    };
    let config = UniflowedConfig::default();
    println!("{path}: {} bytes", file.source.len());

    // Warm up: the first run pays for whatever is initialised once.
    let _ = uf_lint::lint_source(&file, &config).expect("lints");

    let runs = 5;
    CountingAllocator::enable();
    let window = Window::open();
    let before = AllocSnapshot::capture();
    for _ in 0..runs {
        let report = uf_lint::lint_source(&file, &config).expect("lints");
        std::hint::black_box(&report);
    }
    let after = AllocSnapshot::capture();
    drop(window);

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

    if phases {
        // Measured in a second pass rather than the one above: the spans read
        // the allocator's counters as they open and close, and that is work
        // the headline should not be made to carry.
        scope::enable();
        let mut recorder = Recorder::new("lint_source").with_config(ReportConfig {
            sort_by: SortBy::Allocations,
            ..ReportConfig::default()
        });
        for _ in 0..runs {
            recorder.record(|| {
                let report = uf_lint::lint_source(&file, &config).expect("lints");
                std::hint::black_box(report)
            });
        }
        let report = recorder.finish();
        scope::disable();
        println!();
        println!("{}", report.render_table());
    }
    CountingAllocator::disable();
}
