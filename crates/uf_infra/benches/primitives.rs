#![allow(clippy::disallowed_macros)]

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;
use uf_infra::{CompactString, LineIndex, append, cstr, normalize_slashes};

fn primitives(c: &mut Criterion) {
    let mut group = c.benchmark_group("primitives");
    group.warm_up_time(Duration::from_millis(250));
    group.measurement_time(Duration::from_secs(1));
    group.sample_size(30);
    for path in [
        "src/app.js",
        "src\\app.js",
        "long/path/with/many/segments/to/the/module.js",
    ] {
        group.bench_function(format!("normalize/before/{path}"), |b| {
            b.iter(|| CompactString::from(black_box(path).replace('\\', "/")))
        });
        group.bench_function(format!("normalize/after/{path}"), |b| {
            b.iter(|| normalize_slashes(black_box(path)))
        });
    }
    group.bench_function("append/before", |b| {
        b.iter(|| {
            let mut output = String::with_capacity(4096);
            for index in 0..64 {
                output.push_str(&format!("export const field{index} = {index};\n"));
            }
            black_box(output)
        })
    });
    group.bench_function("append/after", |b| {
        b.iter(|| {
            let mut output = String::with_capacity(4096);
            for index in 0..64 {
                append!(output, "export const field{index} = {index};\n");
            }
            black_box(output)
        })
    });
    group.bench_function("owned/string", |b| {
        b.iter(|| format!("item-{}", black_box(12)))
    });
    group.bench_function("owned/compact", |b| {
        b.iter(|| cstr!("item-{}", black_box(12)))
    });
    group.bench_function("line-index/before", |b| {
        b.iter(|| {
            let source = black_box("one\ntwo\n日本語\nfour\n");
            let mut starts =
                Vec::with_capacity(source.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1);
            starts.push(0);
            starts.extend(uf_infra::memchr_iter(b'\n', source.as_bytes()).map(|i| i + 1));
            black_box(starts)
        })
    });
    group.bench_function("line-index/after", |b| {
        b.iter(|| LineIndex::new(black_box("one\ntwo\n日本語\nfour\n")))
    });
    let long_source = "let value = 1;\n".repeat(2048);
    group.bench_function("line-index/long-before", |b| {
        b.iter(|| {
            let source = black_box(long_source.as_str());
            let mut starts =
                Vec::with_capacity(source.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1);
            starts.push(0);
            starts.extend(uf_infra::memchr_iter(b'\n', source.as_bytes()).map(|i| i + 1));
            black_box(starts)
        })
    });
    group.bench_function("line-index/long-after", |b| {
        b.iter(|| LineIndex::new(black_box(long_source.as_str())))
    });
    group.bench_function("float/before", |b| {
        b.iter(|| black_box("-123.456789e-12").parse::<f32>().unwrap())
    });
    group.bench_function("float/after", |b| {
        b.iter(|| uf_infra::parse_float::<f32, _>(black_box("-123.456789e-12")).unwrap())
    });
    group.finish();
}
criterion_group!(benches, primitives);
criterion_main!(benches);
