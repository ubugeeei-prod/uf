//! What a thousand-file suite costs uf: discovery, scheduling, and the import
//! graph — the work that happens before any JavaScript runs.
//!
//! The suite is synthetic but shaped like a real one — a long tail of small
//! files, a handful of large ones — because a uniform suite hides the only
//! thing the scheduler exists to fix. Throughput is reported in files, so the
//! headline number is files/second.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_test::{ImportGraph, TestTimings, discover_tests, merge_plans, schedule_files};

/// How many files the synthetic suite holds.
const SUITE_FILES: usize = 1_000;

/// One in this many files is an order of magnitude larger than the rest.
const HEAVY_EVERY: usize = 25;

/// Build one test file with `cases` declarations, half of them nested.
fn test_file(index: usize, cases: usize) -> String {
    let mut source = String::with_capacity(cases * 160);
    source.push_str("// @flow\nimport { describe, expect, it } from '@uniflowed/test';\n");
    if index > 0 {
        source.push_str("import { helper } from './helper.js';\n");
    }
    source.push_str("describe('suite ");
    source.push_str(&index.to_string());
    source.push_str("', () => {\n");
    for case in 0..cases {
        source.push_str("  it('case ");
        source.push_str(&case.to_string());
        source.push_str("', () => {\n    expect(");
        source.push_str(&case.to_string());
        source.push_str(" + 1).toBe(");
        source.push_str(&(case + 1).to_string());
        source.push_str(");\n    expect(label('flow')).toEqual('flow');\n  });\n");
    }
    source.push_str("  it.skip('disabled', () => {});\n  it.todo('unwritten');\n});\n");
    source
}

/// The synthetic suite: 1 000 files, every 25th of them 20x the size.
fn suite() -> Vec<(String, String)> {
    let mut files = Vec::with_capacity(SUITE_FILES + 1);
    files.push((
        "src/helper.js".to_string(),
        "// @flow\nexport const helper = (value: string): string => value;\n".to_string(),
    ));
    for index in 0..SUITE_FILES {
        let cases = if index % HEAVY_EVERY == 0 { 200 } else { 10 };
        files.push((format!("src/f{index:04}.test.js"), test_file(index, cases)));
    }
    files
}

fn pairs(owned: &[(String, String)]) -> Vec<(&str, &str)> {
    owned
        .iter()
        .map(|(file, source)| (file.as_str(), source.as_str()))
        .collect()
}

/// Recorded timings for the whole suite, so the warm schedule is measured too.
fn warm_timings(sources: &[(&str, &str)]) -> TestTimings {
    let mut timings = TestTimings::new();
    for (file, source) in sources {
        timings.record(file, (source.len() as u64) / 4);
    }
    timings
}

fn bench_runner(c: &mut Criterion) {
    let owned = suite();
    let sources = pairs(&owned);
    let warm = warm_timings(&sources);
    let total_bytes: u64 = sources.iter().map(|(_, source)| source.len() as u64).sum();

    let mut group = c.benchmark_group("uf_test suite");
    group.throughput(Throughput::Elements(sources.len() as u64));

    group.bench_function("discovery", |b| {
        b.iter(|| {
            let plans = sources
                .iter()
                .map(|(file, source)| discover_tests(file, source));
            black_box(merge_plans(plans))
        });
    });

    group.bench_function("schedule cold", |b| {
        let cold = TestTimings::new();
        b.iter(|| black_box(schedule_files(&sources, &cold)));
    });

    group.bench_function("schedule warm", |b| {
        b.iter(|| black_box(schedule_files(&sources, &warm)));
    });

    group.bench_function("import graph", |b| {
        b.iter(|| black_box(ImportGraph::build(sources.iter().copied())));
    });

    group.bench_function("invalidate one shared edit", |b| {
        let graph = ImportGraph::build(sources.iter().copied());
        b.iter(|| {
            black_box(graph.affected_tests(["src/helper.js"], |path| path.ends_with(".test.js")))
        });
    });

    // What a *run* costs is not measured here. It is dominated by starting
    // worker processes and by the host executing the code, neither of which
    // criterion can measure meaningfully from inside a Rust process; the
    // end-to-end numbers, against Bun and Vitest, are in
    // `docs/architecture.md`. What is measured above is exactly the work uf
    // does itself.
    group.finish();

    // Discovery is the one stage whose cost is a function of source size
    // rather than file count, so it is also reported per byte.
    let mut bytes = c.benchmark_group("uf_test bytes");
    bytes.throughput(Throughput::Bytes(total_bytes));
    bytes.bench_function("discover", |b| {
        b.iter(|| {
            black_box(merge_plans(
                sources
                    .iter()
                    .map(|(file, source)| discover_tests(file, source)),
            ))
        });
    });
    bytes.finish();
}

criterion_group!(benches, bench_runner);
criterion_main!(benches);
