use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use uf_rsc::{
    BuildId, EntryKind, RscGraphBuilder, RscManifest, ServerActionRegistry, module_environment,
};

const MODULES: usize = 5_000;

/// One synthetic module per index: mostly Server Components, every eighth a
/// Client Component, every twentieth a `"use server"` module.
fn module_source(index: usize) -> String {
    let mut source = String::with_capacity(256);
    if index.is_multiple_of(20) {
        source.push_str("\"use server\";\n");
    } else if index.is_multiple_of(8) {
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
    if index.is_multiple_of(20) {
        source.push_str("export async function refresh() {}\n");
    } else {
        source.push_str("export default function Page() { return null; }\n");
    }
    source
}

fn sources() -> Vec<(String, String)> {
    (0..MODULES)
        .map(|index| (format!("app/m{index}.js"), module_source(index)))
        .collect()
}

fn bench_rsc(criterion: &mut Criterion) {
    let sources = sources();
    let build_id = BuildId::new("uf-rsc-benchmark-build-id").expect("valid build id");

    criterion.bench_function("module_environment over 5000 modules", |bencher| {
        bencher.iter(|| {
            let mut clients = 0usize;
            for (_, source) in &sources {
                clients +=
                    usize::from(module_environment(source) == uf_rsc::ModuleEnvironment::Client);
            }
            black_box(clients)
        });
    });

    criterion.bench_function("build a 5000 module graph", |bencher| {
        bencher.iter(|| {
            let mut builder = RscGraphBuilder::new();
            for (path, source) in &sources {
                builder.add_source(path.as_str(), source);
            }
            builder.add_entry("app/m0.js", EntryKind::Server);
            black_box(builder.build())
        });
    });

    let mut builder = RscGraphBuilder::new();
    for (path, source) in &sources {
        builder.add_source(path.as_str(), source);
    }
    builder.add_entry("app/m0.js", EntryKind::Server);
    let graph = builder.build();

    criterion.bench_function("register server actions for 5000 modules", |bencher| {
        bencher.iter(|| black_box(ServerActionRegistry::from_graph(&graph, &build_id)));
    });

    let registry = ServerActionRegistry::from_graph(&graph, &build_id);
    let known = registry
        .callable_actions()
        .next()
        .map(|action| action.id.to_hex())
        .unwrap_or_else(|| "0".repeat(64).into());
    let unknown = "0".repeat(64);

    criterion.bench_function("resolve a known action id", |bencher| {
        bencher.iter(|| black_box(registry.resolve(&known).is_ok()));
    });

    criterion.bench_function("reject an unknown action id", |bencher| {
        bencher.iter(|| black_box(registry.resolve(&unknown).is_err()));
    });

    criterion.bench_function("serialize the manifest", |bencher| {
        bencher.iter(|| black_box(RscManifest::new(&graph, &registry).to_json()));
    });
}

criterion_group!(benches, bench_rsc);
criterion_main!(benches);
