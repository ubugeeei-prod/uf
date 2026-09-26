//! Measure pooled import/export names over a shared-name lint batch.
use std::hint::black_box;
use std::time::Instant;
use uf_config::{RuleLevel, UniflowedConfig};
use uf_lint::{SourceFile, lint_sources};
use uf_profiler::{CountingAllocator, ThreadWindow};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

fn main() {
    let mut config = UniflowedConfig::default();
    for rule in uf_lint::rules() {
        config.lint.rules.insert(rule.id.into(), RuleLevel::Off);
    }
    config
        .lint
        .rules
        .insert("import/no-unused-modules".into(), RuleLevel::Error);
    let files: Vec<_> = (0..128)
        .map(|module| {
            let mut source = String::from("// @flow\nimport { ");
            for name in 0..24 {
                if name > 0 {
                    source.push_str(", ");
                }
                write_numbered(&mut source, "shared", name);
            }
            source.push_str(" } from './module");
            write_numbered(&mut source, "", (module + 1) % 128);
            source.push_str(".js';\n");
            for name in 0..24 {
                source.push_str("export const ");
                write_numbered(&mut source, "shared", name);
                source.push_str(" = 1;\n");
            }
            let mut path = String::from("module");
            write_numbered(&mut path, "", module);
            path.push_str(".js");
            SourceFile { path, source }
        })
        .collect();
    let warm = lint_sources(&files, &config).expect("warm lint");
    assert!(warm.diagnostics.is_empty());
    let window = ThreadWindow::open();
    let measured = lint_sources(&files, &config).expect("measured lint");
    let delta = window.close();
    assert!(measured.diagnostics.is_empty());
    let mut nanos = Vec::new();
    for _ in 0..11 {
        let start = Instant::now();
        let report = lint_sources(black_box(&files), black_box(&config)).expect("timed lint");
        black_box(report);
        nanos.push(start.elapsed().as_nanos());
    }
    nanos.sort_unstable();
    println!(
        "import-graph allocations={} bytes={} median_ns={}",
        delta.allocations,
        delta.bytes_allocated,
        nanos[nanos.len() / 2]
    );
}

fn write_numbered(buffer: &mut String, prefix: &str, number: usize) {
    use std::fmt::Write as _;
    write!(buffer, "{prefix}{number}").expect("write to String");
}
