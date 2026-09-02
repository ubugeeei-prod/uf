//! What a thousand-file suite costs: discovery, scheduling, and the whole run.
//!
//! The suite is synthetic but shaped like a real one — a long tail of small
//! files, a handful of large ones — because a uniform suite hides the only
//! thing the scheduler exists to fix. Throughput is reported in files, so the
//! headline number is files/second.

use std::hint::black_box;
use std::num::NonZeroUsize;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_test::{
    Concurrency, ImportGraph, RunOptions, TestRunner, TestTimings, discover_tests, merge_plans,
    schedule_files,
};

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

    group.bench_function("run serial", |b| {
        let runner = TestRunner::new().with_options(RunOptions::serial());
        b.iter(|| black_box(runner.run(&sources)));
    });

    group.bench_function("run parallel", |b| {
        let runner = TestRunner::new().with_options(RunOptions {
            concurrency: Concurrency::Auto,
            ..RunOptions::default()
        });
        b.iter(|| black_box(runner.run(&sources)));
    });

    group.bench_function("run parallel warm schedule", |b| {
        let runner = TestRunner::new()
            .with_timings(warm.clone())
            .with_options(RunOptions {
                concurrency: Concurrency::Auto,
                ..RunOptions::default()
            });
        b.iter(|| black_box(runner.run(&sources)));
    });

    for threads in [2usize, 4, 8] {
        let Some(threads) = NonZeroUsize::new(threads) else {
            continue;
        };
        group.bench_function(format!("run on {threads} workers"), |b| {
            let runner = TestRunner::new().with_options(RunOptions {
                concurrency: Concurrency::Fixed(threads),
                ..RunOptions::default()
            });
            b.iter(|| black_box(runner.run(&sources)));
        });
    }

    group.finish();

    let mut bytes = c.benchmark_group("uf_test bytes");
    bytes.throughput(Throughput::Bytes(total_bytes));
    bytes.bench_function("run parallel", |b| {
        let runner = TestRunner::new();
        b.iter(|| black_box(runner.run(&sources)));
    });
    bytes.finish();
}

criterion_group!(benches, bench_runner);
criterion_main!(benches);
