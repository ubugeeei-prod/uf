use std::hint::black_box;

use compact_str::{CompactString, ToCompactString};
use criterion::{Criterion, criterion_group, criterion_main};
use uf_bundle::{
    AssetEntry, AssetKind, AssetSize, BudgetMetric, BundleBudgets, ByteSize, SizeBudget,
    build_report, evaluate, measure,
};

fn synthetic_assets(count: usize) -> Vec<AssetEntry> {
    (0..count)
        .map(|index| AssetEntry {
            path: format!("assets/chunk-{index:05}.js").to_compact_string(),
            kind: AssetKind::JavaScript,
            size: AssetSize {
                raw: ByteSize::from_bytes(4_096 + index as u64),
                gzip: ByteSize::from_bytes(1_024 + index as u64),
                brotli: ByteSize::from_bytes(900 + index as u64),
            },
        })
        .collect()
}

fn synthetic_routes(count: usize) -> Vec<(CompactString, Vec<CompactString>, Vec<CompactString>)> {
    (0..count)
        .map(|index| {
            (
                format!("/route-{index:04}").to_compact_string(),
                vec![format!("assets/chunk-{index:05}.js").to_compact_string()],
                vec![format!("assets/chunk-{:05}.js", index + 1).to_compact_string()],
            )
        })
        .collect()
}

fn bench_build_report(c: &mut Criterion) {
    let assets = synthetic_assets(2_000);
    let routes = synthetic_routes(500);

    c.bench_function("build report over 2000 assets and 500 routes", |b| {
        b.iter(|| black_box(build_report(assets.clone(), &routes)));
    });
}

fn bench_evaluate_budgets(c: &mut Criterion) {
    let report = build_report(synthetic_assets(2_000), &synthetic_routes(500));
    let budgets = BundleBudgets {
        total: Some(SizeBudget::new(ByteSize::from_bytes(1))),
        initial_js: Some(SizeBudget::with_metric(
            ByteSize::from_bytes(1),
            BudgetMetric::Brotli,
        )),
        per_route: Some(SizeBudget::new(ByteSize::from_bytes(1))),
        per_asset: Some(SizeBudget::new(ByteSize::from_bytes(1))),
    };

    c.bench_function("evaluate four budgets over 2000 assets", |b| {
        b.iter(|| black_box(evaluate(&report, &budgets)));
    });
}

fn bench_measure(c: &mut Criterion) {
    let contents = "export const value = compute(1, 2, 3);\n".repeat(4_000);

    c.bench_function("measure a 152 kB javascript chunk", |b| {
        b.iter(|| black_box(measure(contents.as_bytes()).expect("measures")));
    });
}

criterion_group!(
    benches,
    bench_build_report,
    bench_evaluate_budgets,
    bench_measure
);
criterion_main!(benches);
