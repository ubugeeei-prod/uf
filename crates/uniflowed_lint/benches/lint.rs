use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use uniflowed_config::UniflowedConfig;
use uniflowed_lint::{SourceFile, lint_sources};

fn bench_lint_scan(c: &mut Criterion) {
    let config = UniflowedConfig::default();
    let files = (0..1_000)
        .map(|index| SourceFile {
            path: format!("app/route{index}/_uf.page.flow"),
            source: "// @flow\ncomponent Page() { return <main />; }\n".to_string(),
        })
        .collect::<Vec<_>>();

    c.bench_function("lint 1000 flow route files", |b| {
        b.iter(|| black_box(lint_sources(&files, &config).expect("lint")));
    });
}

criterion_group!(benches, bench_lint_scan);
criterion_main!(benches);
