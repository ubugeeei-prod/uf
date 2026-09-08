//! What the profiler costs, which decides where it is allowed to live.
//!
//! `profile_span!` is meant to stay in hot paths permanently — a profiler you
//! have to add before you can use it is one that is never there when the
//! question arrives. That is only defensible if a disabled span is free, so
//! that is the first thing measured here, and the gap between the two numbers
//! is the argument.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use uf_profiler::{ScopeGuard, scope};

fn spans(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("span");

    // The number that has to stay tiny: what every `profile_span!` in the
    // toolchain costs on a normal run. One relaxed-acquire load of a static
    // bool, and a guard holding a `bool`.
    scope::disable();
    group.bench_function("disabled", |bencher| {
        bencher.iter(|| {
            let guard = ScopeGuard::enter("bench");
            black_box(&guard);
        });
    });

    // And what it costs when somebody asks for a profile. This is allowed to
    // be much larger; it is paid only on a `--profile` run, and
    // `scope::calibrate_overhead_ns` exists so a report can subtract it.
    scope::enable();
    scope::reset_thread_spans();
    group.bench_function("enabled", |bencher| {
        bencher.iter(|| {
            let guard = ScopeGuard::enter("bench");
            black_box(&guard);
        });
    });
    scope::disable();
    scope::reset_thread_spans();

    group.finish();
}

fn nesting(criterion: &mut Criterion) {
    // Self-time attribution walks the stack on close, so depth is the axis
    // that could turn linear cost into something worse.
    let mut group = criterion.benchmark_group("span/nesting");
    scope::enable();
    for depth in [1_usize, 4, 16] {
        group.bench_function(format!("depth-{depth}"), |bencher| {
            bencher.iter(|| {
                let mut guards = Vec::with_capacity(depth);
                for _ in 0..depth {
                    guards.push(ScopeGuard::enter("nested"));
                }
                black_box(&guards);
            });
        });
    }
    scope::disable();
    scope::reset_thread_spans();
    group.finish();
}

/// What the parts of an enabled span cost, so the total has an explanation.
///
/// Without this the enabled figure is a number with nowhere to go: is it the
/// clock, the thread-local, the borrow check, or the allocator counters? The
/// answer decides whether there is anything left to optimise, and "two clock
/// reads" is a floor rather than a defect.
fn parts(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("span/parts");

    group.bench_function("instant-now-pair", |bencher| {
        bencher.iter(|| {
            let start = std::time::Instant::now();
            black_box(start.elapsed())
        });
    });

    group.bench_function("alloc-counter-pair", |bencher| {
        bencher.iter(|| {
            let counter = uf_profiler::AllocCounter::start();
            black_box(counter.delta())
        });
    });

    group.finish();
}

criterion_group!(benches, spans, nesting, parts);
criterion_main!(benches);
