use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use uf_test::discover_tests;

fn bench_test_discovery(c: &mut Criterion) {
    let mut source = String::from("// @flow\nimport { describe, it } from '@uniflowed/testing';\n");
    for index in 0..5_000 {
        source.push_str(&format!("it('case {index}', () => {{}});\n"));
    }

    c.bench_function("discover 5000 native tests", |b| {
        b.iter(|| black_box(discover_tests("index.test.flow", &source)));
    });
}

criterion_group!(benches, bench_test_discovery);
criterion_main!(benches);
