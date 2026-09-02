//! A whole build over a synthetic 2 000-module, 50-route project.
//!
//! The project is written to a temporary directory once and bundled repeatedly,
//! so the measurement is the bundler and the file system it really reads, not a
//! graph handed to it in memory. Throughput is declared in modules, so criterion
//! reports modules per second next to the wall time for the whole build.

use std::hint::black_box;

use camino::Utf8PathBuf;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_bundler::{BundleOptions, BundlerLimits, build_pipeline, bundle};
use uf_config::{PipelineMode, UniflowedConfig};
use uf_plugin::PluginContainer;

/// Routes in the synthetic project.
const ROUTES: usize = 50;

/// Leaf modules each route imports.
const LEAVES_PER_ROUTE: usize = 38;

/// Shared modules every route reaches.
const SHARED: usize = 50;

/// A project of roughly `ROUTES * (LEAVES_PER_ROUTE + 1) + SHARED` modules.
struct Project {
    _directory: tempfile::TempDir,
    root: Utf8PathBuf,
    entries: Vec<Utf8PathBuf>,
    modules: u64,
}

fn write(root: &Utf8PathBuf, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create directory");
    }
    std::fs::write(path, contents).expect("write module");
}

fn project() -> Project {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).expect("utf-8 path");
    let mut entries = Vec::with_capacity(ROUTES);

    for index in 0..SHARED {
        write(
            &root,
            &format!("shared/s{index}.js"),
            &format!(
                "// @flow\nexport type S{index} = string;\nexport const shared{index} = (value: S{index}): S{index} => value;\n"
            ),
        );
    }

    for route in 0..ROUTES {
        for leaf in 0..LEAVES_PER_ROUTE {
            write(
                &root,
                &format!("app/r{route}/leaf{leaf}.js"),
                &format!(
                    "// @flow\nimport {{ shared{shared} }} from \"../../shared/s{shared}.js\";\nexport const leaf{leaf} = (value: string): string => shared{shared}(value);\nexport const unused{leaf} = 1;\n",
                    shared = leaf % SHARED
                ),
            );
        }

        let imports = (0..LEAVES_PER_ROUTE)
            .map(|leaf| format!("import {{ leaf{leaf} }} from \"./leaf{leaf}.js\";\n"))
            .collect::<String>();
        let uses = (0..LEAVES_PER_ROUTE)
            .map(|leaf| format!("leaf{leaf}(\"x\")"))
            .collect::<Vec<_>>()
            .join(", ");
        let page = format!("app/r{route}/_uf.page.js");
        write(
            &root,
            &page,
            &format!(
                "// @flow\n{imports}\ncomponent Page() renders Node {{\n  return [{uses}];\n}}\nexport default Page;\n"
            ),
        );
        entries.push(Utf8PathBuf::from(page));
    }

    let modules = (ROUTES * (LEAVES_PER_ROUTE + 1) + SHARED) as u64;
    Project {
        _directory: directory,
        root,
        entries,
        modules,
    }
}

fn container(root: &Utf8PathBuf) -> PluginContainer {
    build_pipeline(&UniflowedConfig::default(), root, PipelineMode::Build, &[])
        .expect("pipeline resolves")
}

fn bench_build(criterion: &mut Criterion) {
    let project = project();
    let container = container(&project.root);
    let options = BundleOptions::new(project.root.clone(), project.root.join("dist"))
        .with_entries(project.entries.clone())
        .with_sourcemap(false)
        .with_limits(BundlerLimits::default());

    let mut group = criterion.benchmark_group("bundle");
    group.throughput(Throughput::Elements(project.modules));
    group.sample_size(10);
    group.bench_function(
        format!("{} modules over {ROUTES} routes", project.modules),
        |bencher| {
            bencher.iter(|| black_box(bundle(&options, &container).expect("bundle succeeds")));
        },
    );
    group.finish();
}

fn bench_build_with_source_maps(criterion: &mut Criterion) {
    let project = project();
    let container = container(&project.root);
    let options = BundleOptions::new(project.root.clone(), project.root.join("dist"))
        .with_entries(project.entries.clone())
        .with_sourcemap(true)
        .with_limits(BundlerLimits::default());

    let mut group = criterion.benchmark_group("bundle-sourcemap");
    group.throughput(Throughput::Elements(project.modules));
    group.sample_size(10);
    group.bench_function(
        format!("{} modules with source maps", project.modules),
        |bencher| {
            bencher.iter(|| black_box(bundle(&options, &container).expect("bundle succeeds")));
        },
    );
    group.finish();
}

fn bench_flow_strip(criterion: &mut Criterion) {
    let source = "// @flow\nimport type { Id } from \"./id.js\";\nexport type Box<T> = { value: T };\ncomponent Card(title: string, body: string) renders Node {\n  return [title, body];\n}\nexport const make = (raw: string): Id => raw;\n".repeat(200);

    let mut group = criterion.benchmark_group("flow");
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function("strip a 1400-line flow module", |bencher| {
        bencher.iter(|| black_box(uf_flow::strip_types(&source).expect("strips")));
    });
    group.finish();
}

fn bench_scan_module(criterion: &mut Criterion) {
    let source = "import { a, b as c } from \"./x.js\";\nexport const d = a + c;\nexport { d as e };\nexport * from \"./y.js\";\n".repeat(200);

    let mut group = criterion.benchmark_group("record");
    group.throughput(Throughput::Bytes(source.len() as u64));
    group.bench_function("scan an 800-line module record", |bencher| {
        bencher.iter(|| black_box(uf_bundler::scan_module(&source)));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_build,
    bench_build_with_source_maps,
    bench_flow_strip,
    bench_scan_module
);
criterion_main!(benches);
