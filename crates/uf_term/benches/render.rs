//! What it costs to render a large report.
//!
//! The lint runner can produce thousands of diagnostics in one pass, and the
//! product requirement is that formatting them never becomes the expensive
//! half. The benchmark renders 10 000 code frames into one reused buffer, which
//! is exactly what `uf lint` does.

use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_term::{
    Capabilities, Cell, CodeFrame, ColorLevel, Column, DiagnosticLevel, GlyphSet, KeyValue, Phase,
    Renderer, Status, Table, Tree, Tty,
};

const DIAGNOSTIC_COUNT: usize = 10_000;

const RULES: [&str; 4] = [
    "flow/unclear-type",
    "uf/no-default-export",
    "uf/server-only-import",
    "flow/sketchy-null",
];

const SOURCE_LINES: [&str; 4] = [
    "const value: any = await load(props.id);",
    "export default function Page({ params }: Props) {",
    "  import { secret } from '../server/secret.js';",
    "\tif (value) { return renderMarkdown(value); }",
];

const PATHS: [&str; 4] = [
    "app/routes/index.js",
    "app/client/Counter.js",
    "app/日本語/ページ.js",
    "server/actions.js",
];

fn frames() -> Vec<CodeFrame<'static>> {
    (0..DIAGNOSTIC_COUNT)
        .map(|index| {
            let level = if index % 3 == 0 {
                DiagnosticLevel::Error
            } else {
                DiagnosticLevel::Warning
            };
            CodeFrame::new(
                level,
                "value is not typed precisely enough for the router",
                PATHS[index % PATHS.len()],
                index % 997 + 1,
                index % 37 + 1,
            )
            .with_rule(RULES[index % RULES.len()])
            .with_span(3)
            .with_source_line(SOURCE_LINES[index % SOURCE_LINES.len()])
        })
        .collect()
}

fn renderer(color: ColorLevel) -> Renderer {
    Renderer::new(Capabilities::new(color, GlyphSet::Unicode, Tty::Piped))
}

fn bench_diagnostics(criterion: &mut Criterion) {
    let frames = frames();
    let mut group = criterion.benchmark_group("diagnostics");
    group.throughput(Throughput::Elements(DIAGNOSTIC_COUNT as u64));

    for (name, color) in [
        ("plain", ColorLevel::Never),
        ("ansi16", ColorLevel::Ansi16),
        ("truecolor", ColorLevel::TrueColor),
    ] {
        let renderer = renderer(color);
        group.bench_function(name, |bencher| {
            let mut out = String::with_capacity(1 << 20);
            bencher.iter(|| {
                out.clear();
                for frame in &frames {
                    renderer.code_frame(&mut out, frame);
                }
                black_box(out.len())
            });
        });
    }
    group.finish();
}

fn bench_primitives(criterion: &mut Criterion) {
    let renderer = renderer(ColorLevel::Ansi256);
    let paths: Vec<String> = (0..512)
        .map(|index| format!("app/routes/{index}/page.js"))
        .collect();
    let borrowed: Vec<&str> = paths.iter().map(String::as_str).collect();
    let counts: Vec<String> = (0..512).map(|index| index.to_string()).collect();

    let mut group = criterion.benchmark_group("primitives");

    group.bench_function("table_512_rows", |bencher| {
        let mut out = String::with_capacity(1 << 16);
        bencher.iter(|| {
            out.clear();
            let mut table = Table::new(vec![Column::left("file"), Column::right("problems")]);
            for (path, count) in borrowed.iter().zip(&counts) {
                table.push(vec![Cell::new(path), Cell::new(count)]);
            }
            renderer.table(&mut out, 2, &table);
            black_box(out.len())
        });
    });

    group.bench_function("tree_512_paths", |bencher| {
        let mut out = String::with_capacity(1 << 16);
        bencher.iter(|| {
            out.clear();
            let tree = Tree::from_paths("demo-app", borrowed.iter().copied());
            renderer.tree(&mut out, 2, &tree);
            black_box(out.len())
        });
    });

    group.bench_function("summary_block", |bencher| {
        let phases = [
            Phase {
                label: "config",
                duration: Duration::from_micros(1_200),
            },
            Phase {
                label: "routes",
                duration: Duration::from_micros(800),
            },
            Phase {
                label: "rsc analysis",
                duration: Duration::from_millis(31),
            },
        ];
        let mut out = String::with_capacity(4_096);
        bencher.iter(|| {
            out.clear();
            renderer.banner(&mut out, "uf build", Some("demo-app"));
            renderer.timings(&mut out, 2, &phases, Some(Duration::from_millis(33)));
            renderer.key_values(
                &mut out,
                2,
                &[
                    KeyValue::new("entries", "app.js"),
                    KeyValue::new("routes", "12"),
                    KeyValue::new("modules", "84"),
                ],
            );
            renderer.status(&mut out, Status::Success, "build succeeded");
            black_box(out.len())
        });
    });

    group.finish();
}

criterion_group!(benches, bench_diagnostics, bench_primitives);
criterion_main!(benches);
