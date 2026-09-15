//! Benchmarks: what discovery reads, what each kind of run skips, and what a
//! benchmark's samples summarise to.

use crate::discovery::discover_tests;
use crate::plan::{Selection, SkipReason, TestKind};
use crate::report::{BenchStats, MAX_BENCH_SAMPLES};

#[test]
fn a_benchmark_is_summarised_by_nearest_rank_percentiles_of_its_samples() {
    let stats = BenchStats::from_samples(&[40, 10, 30, 20]).expect("four samples");
    assert_eq!(
        stats,
        BenchStats {
            samples: 4,
            min_micros: 10,
            median_micros: 20,
            mean_micros: 25,
            p75_micros: 30,
            p99_micros: 40,
            max_micros: 40,
        }
    );

    let one = BenchStats::from_samples(&[7]).expect("one sample");
    assert_eq!(
        (
            one.min_micros,
            one.median_micros,
            one.p99_micros,
            one.max_micros
        ),
        (7, 7, 7, 7)
    );
    assert_eq!(
        BenchStats::from_samples(&[5, 1, 3])
            .expect("three samples")
            .median_micros,
        3
    );
    assert_eq!(BenchStats::from_samples(&[]), None);
}

#[test]
fn only_the_samples_up_to_the_ceiling_are_kept() {
    let mut samples = vec![1; MAX_BENCH_SAMPLES];
    samples.push(1_000_000);

    let stats = BenchStats::from_samples(&samples).expect("samples");

    assert_eq!((stats.samples, stats.max_micros), (MAX_BENCH_SAMPLES, 1));
}

#[test]
fn discovery_reads_a_benchmark_and_its_modifiers() {
    let plan = discover_tests(
        "src/sum.bench.js",
        "import { bench, it } from '@uniflowed/test';\n\
         bench('sums', () => {});\n\
         bench.skip('later', () => {});\n\
         it('adds', () => {});\n",
    );

    assert_eq!((plan.bench_count(), plan.runnable_count()), (2, 1));
}

#[test]
fn a_benchmark_imported_from_another_runner_is_named_as_that_runners() {
    let plan = discover_tests(
        "src/sum.bench.js",
        "import { bench } from 'vitest';\nbench('sums', () => {});\n",
    );

    assert_eq!((plan.bench_count(), plan.foreign_count()), (0, 1));
}

#[test]
fn a_run_of_the_benchmarks_skips_the_tests_and_a_run_of_the_tests_skips_the_benchmarks() {
    let plan = discover_tests(
        "src/mixed.test.js",
        "bench('sums', () => {});\n\
         it('adds', () => {});\n\
         bench.todo('later');\n\
         it.skip('off', () => {});\n",
    );
    let selections = |benches: bool| -> Vec<(TestKind, Selection)> {
        let mut resolution = plan.resolve();
        resolution.select_run_kind(&plan, benches);
        plan.cases
            .iter()
            .enumerate()
            .map(|(index, case)| (case.kind, resolution.selection(index)))
            .collect()
    };

    let tests = selections(false);
    for expected in [
        (TestKind::Bench, Selection::Skipped(SkipReason::Bench)),
        (TestKind::Test, Selection::Run),
        (TestKind::Bench, Selection::Todo),
        (TestKind::Test, Selection::Skipped(SkipReason::Explicit)),
    ] {
        assert!(tests.contains(&expected), "{expected:?} in {tests:?}");
    }

    let benches = selections(true);
    for expected in [
        (TestKind::Bench, Selection::Run),
        (TestKind::Test, Selection::Skipped(SkipReason::NotBench)),
        (TestKind::Bench, Selection::Todo),
        (TestKind::Test, Selection::Skipped(SkipReason::Explicit)),
    ] {
        assert!(benches.contains(&expected), "{expected:?} in {benches:?}");
    }
}
