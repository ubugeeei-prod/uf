//! What a single edit costs on a project-sized dev graph.
//!
//! The shape mirrors `uf_rsc`'s benchmark so the two are comparable: 5 000
//! modules, every eighth a `"use client"` component, each importing its
//! successor and a module eight ahead so the graph has real fan-in rather than
//! being a chain.
//!
//! Three numbers matter, and they are the three measured here:
//!
//! * **invalidate one file** — the number `uf dev` pays on every keystroke;
//! * **rescan one file** — the incremental claim: re-scanning one module must
//!   not cost anything proportional to the project;
//! * **build the graph** — the one-time start-up cost, for context.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use uf_devserver::hmr::{ChangeKind, DevGraph, invalidate};

const MODULES: usize = 5_000;

fn module_source(index: usize) -> String {
    let mut source = String::with_capacity(256);
    if index.is_multiple_of(8) {
        source.push_str("\"use client\";\n");
    }
    source.push_str("// @flow\n");
    source.push_str("import * as React from \"@uniflowed/react\";\n");
    if index + 1 < MODULES {
        source.push_str(&format!("import next from \"./m{}.js\";\n", index + 1));
    }
    if index + 8 < MODULES {
        source.push_str(&format!("import wide from \"./m{}.js\";\n", index + 8));
    }
    if index.is_multiple_of(8) {
        source.push_str(&format!("export function M{index}() {{ return null; }}\n"));
    } else {
        source.push_str(&format!("export function m{index}() {{ return 1; }}\n"));
    }
    source
}

fn sources() -> Vec<(String, String)> {
    (0..MODULES)
        .map(|index| (format!("app/m{index}.js"), module_source(index)))
        .collect()
}

fn build(sources: &[(String, String)]) -> DevGraph {
    let mut graph = DevGraph::new();
    for (path, source) in sources {
        graph.insert(path, source).expect("module fits the bounds");
    }
    graph
}

fn bench_hmr(criterion: &mut Criterion) {
    let sources = sources();
    let graph = build(&sources);
    // The deepest module: every other module is above it, so the upward walk
    // has the most importers it will ever have to consider.
    let deepest = graph.find("app/m4999.js").expect("scanned");
    // A leaf near the top of the import order, which is the common edit.
    let leaf = graph.find("app/m0.js").expect("scanned");

    criterion.bench_function("invalidate one file in a 5000 module graph", |bencher| {
        bencher.iter(|| black_box(invalidate(&graph, deepest, ChangeKind::Modified)).len());
    });

    criterion.bench_function(
        "invalidate a graph root in a 5000 module graph",
        |bencher| {
            bencher.iter(|| black_box(invalidate(&graph, leaf, ChangeKind::Modified)).len());
        },
    );

    criterion.bench_function("rescan one file in a 5000 module graph", |bencher| {
        let mut graph = build(&sources);
        let (path, source) = &sources[2_500];
        bencher.iter(|| {
            black_box(graph.insert(path, source).expect("rescans")).id();
        });
    });

    criterion.bench_function("build a 5000 module dev graph", |bencher| {
        bencher.iter(|| black_box(build(&sources)).len());
    });
}

criterion_group!(benches, bench_hmr);
criterion_main!(benches);
