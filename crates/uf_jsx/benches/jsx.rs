//! Lowering throughput, and the cost of the modules that hold no JSX at all.
//!
//! Most modules in a build are the second kind, so the fast path they take
//! matters as much as the transform itself.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_flow::scan::{tokenize, tokenize_jsx};
use uf_jsx::{JsxOptions, transform};

/// One component's worth of JSX, of the shape a real page holds.
const COMPONENT: &str = r#"
function Row({ item, onPick }) {
  return (
    <li key={item.id} className="row" onClick={() => onPick(item.id)}>
      <span className="name">{item.name}</span>
      <span className="count">count: {item.count}</span>
      {item.note ? <em>{item.note}</em> : null}
      <Actions {...item.actions} />
    </li>
  );
}
"#;

/// The same shape with no JSX in it, for the fast path.
const PLAIN: &str = r#"
function total(items) {
  let sum = 0;
  for (const item of items) {
    sum = sum + item.count;
  }
  return sum;
}
"#;

fn bench_transform(criterion: &mut Criterion) {
    let source = COMPONENT.repeat(200);
    let options = JsxOptions::default();

    let mut group = criterion.benchmark_group("jsx");
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function("lower 200 components", |bencher| {
        bencher.iter(|| black_box(transform(&source, &options).expect("lowers")));
    });
    group.finish();
}

fn bench_passthrough(criterion: &mut Criterion) {
    let source = PLAIN.repeat(200);
    let options = JsxOptions::default();

    let mut group = criterion.benchmark_group("jsx");
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function("pass 200 modules with no jsx", |bencher| {
        bencher.iter(|| black_box(transform(&source, &options).expect("lowers")));
    });
    group.finish();
}

fn bench_scan(criterion: &mut Criterion) {
    let source = COMPONENT.repeat(200);

    let mut group = criterion.benchmark_group("scan");
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function("tokenize with jsx modes", |bencher| {
        bencher.iter(|| black_box(tokenize_jsx(&source)));
    });
    group.bench_function("tokenize without them", |bencher| {
        bencher.iter(|| black_box(tokenize(&source)));
    });
    group.finish();
}

criterion_group!(benches, bench_transform, bench_passthrough, bench_scan);
criterion_main!(benches);
