//! Throughput of the StyleX pass over a synthetic 500-module project.
//!
//! `Throughput::Elements` is set to the module count, so criterion reports the
//! number that matters for a build: modules per second.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_stylex::{StyleSheet, compile_module, parse_module, props_of};

/// Modules in the synthetic project.
const MODULES: usize = 500;

/// One synthetic module: a component that declares styles, with every fourth
/// one also declaring variables and every third one using conditional values.
fn module_source(index: usize) -> String {
    let mut source = String::with_capacity(1024);
    source.push_str("// @flow\n");
    source.push_str("import { stylex } from \"@uniflowed/stylex\";\n");
    source.push_str("import { tokens } from \"../styles/tokens.stylex.js\";\n");
    if index.is_multiple_of(4) {
        source.push_str(&format!(
            "export const vars{index} = stylex.defineVars({{ ink{index}: \"#151b1f\", canvas{index}: \"#f7f7f2\" }});\n"
        ));
    }
    source.push_str("const styles = stylex.create({\n");
    for namespace in 0..4 {
        source.push_str(&format!("  ns{namespace}: {{\n"));
        source.push_str("    display: \"grid\",\n");
        source.push_str(&format!("    marginTop: {},\n", index % 32));
        source.push_str("    padding: 0,\n");
        source.push_str("    backgroundColor: tokens.canvas,\n");
        if index.is_multiple_of(3) {
            source.push_str(
                "    color: { default: tokens.ink, \":hover\": \"red\", \"@media (min-width: 600px)\": \"blue\" },\n",
            );
        } else {
            source.push_str("    color: tokens.ink,\n");
        }
        source.push_str("  },\n");
    }
    source.push_str("});\n");
    source.push_str("export component Page() { return null; }\n");
    source
}

fn sources() -> Vec<String> {
    (0..MODULES).map(module_source).collect()
}

fn bench_stylex(criterion: &mut Criterion) {
    let sources = sources();
    let compiled: Vec<_> = sources
        .iter()
        .map(|source| compile_module(source).expect("the fixture compiles"))
        .collect();

    let mut group = criterion.benchmark_group("uf_stylex");
    group.throughput(Throughput::Elements(MODULES as u64));

    group.bench_function("extract 500 modules", |bencher| {
        bencher.iter(|| {
            let mut declarations = 0usize;
            for source in &sources {
                let parsed = parse_module(source).expect("the fixture parses");
                declarations += parsed.creates.len();
            }
            black_box(declarations)
        });
    });

    group.bench_function("compile 500 modules", |bencher| {
        bencher.iter(|| {
            let mut rules = 0usize;
            for source in &sources {
                rules += compile_module(source)
                    .expect("the fixture compiles")
                    .sheet
                    .len();
            }
            black_box(rules)
        });
    });

    group.bench_function("fold 500 modules into one sheet", |bencher| {
        bencher.iter(|| {
            let mut sheet = StyleSheet::new();
            for module in &compiled {
                sheet.extend(&module.sheet);
            }
            black_box(sheet.len())
        });
    });

    let mut sheet = StyleSheet::new();
    for module in &compiled {
        sheet.extend(&module.sheet);
    }

    group.bench_function("render the sheet", |bencher| {
        bencher.iter(|| black_box(sheet.to_css().len()));
    });

    group.bench_function("merge props over 500 modules", |bencher| {
        bencher.iter(|| {
            let mut classes = 0usize;
            for module in &compiled {
                classes += props_of(&module.styles).len();
            }
            black_box(classes)
        });
    });

    // A module the pass has nothing to do with still has to be cheap: this is
    // the common case in a real project.
    let untouched = "// @flow\nexport component Page() { return null; }\n";
    group.bench_function("skip 500 modules with no styles", |bencher| {
        bencher.iter(|| {
            let mut skipped = 0usize;
            for _ in 0..MODULES {
                skipped += usize::from(!compile_module(untouched).expect("no styles").changed);
            }
            black_box(skipped)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_stylex);
criterion_main!(benches);
