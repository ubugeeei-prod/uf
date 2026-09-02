//! Throughput of the syntax-mode validator over a synthetic 500-module project.
//!
//! `Throughput::Elements` is set to the module count, so criterion reports the
//! number that matters for a build: modules per second.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_plugin::{PipelineMode, PluginContainer};
use uf_react_compiler::{OnFinding, plugin, validate};

/// Modules in the synthetic project.
const MODULES: usize = 500;

/// One synthetic module: a component with hooks, handlers, an effect and JSX.
/// Every seventh one is written with a mistake, so the reporting path is
/// exercised rather than only the accepting one.
fn module_source(index: usize) -> String {
    let mut source = String::with_capacity(1024);
    source.push_str("// @flow\n");
    source.push_str("import { useEffect, useMemo, useRef, useState } from \"@uniflowed/react\";\n");
    source.push_str(&format!(
        "component Page{index}(items: Array<string>, title: string) {{\n"
    ));
    source.push_str("  const [count, setCount] = useState(0);\n");
    source.push_str("  const box = useRef(null);\n");
    source.push_str("  const names = useMemo(() => items.map((item) => item.trim()), [items]);\n");
    source.push_str(
        "  const onClick = () => {\n    setCount(count + 1);\n    measure(box.current);\n  };\n",
    );
    source.push_str("  useEffect(() => {\n    document.title = title;\n  }, [title]);\n");
    if index.is_multiple_of(7) {
        source.push_str("  items.push(title);\n");
        source.push_str("  console.log(count);\n");
    }
    source.push_str("  return <main onClick={onClick}>{names.length + count}</main>;\n");
    source.push_str("}\n");
    source
}

fn sources() -> Vec<String> {
    (0..MODULES).map(module_source).collect()
}

fn bench_react_compiler(criterion: &mut Criterion) {
    let sources = sources();
    let sound: Vec<&String> = sources
        .iter()
        .enumerate()
        .filter(|(index, _)| !index.is_multiple_of(7))
        .map(|(_, source)| source)
        .collect();

    let mut group = criterion.benchmark_group("uf_react_compiler");
    group.throughput(Throughput::Elements(MODULES as u64));

    group.bench_function("validate 500 modules", |bencher| {
        bencher.iter(|| {
            let mut findings = 0usize;
            for source in &sources {
                findings += validate(source).expect("the fixture validates").len();
            }
            black_box(findings)
        });
    });

    group.throughput(Throughput::Elements(sound.len() as u64));
    group.bench_function("validate modules with no findings", |bencher| {
        bencher.iter(|| {
            let mut findings = 0usize;
            for source in &sound {
                findings += validate(source).expect("the fixture validates").len();
            }
            black_box(findings)
        });
    });

    let (compiler, findings) = plugin(OnFinding::Report);
    let container = PluginContainer::build(PipelineMode::Build, vec![Box::new(compiler)])
        .expect("one plugin resolves");

    group.throughput(Throughput::Elements(MODULES as u64));
    group.bench_function("run 500 modules through the container", |bencher| {
        bencher.iter(|| {
            let mut handled = 0usize;
            for (index, source) in sources.iter().enumerate() {
                let outcome = container
                    .transform(&format!("app/page{index}.js"), source)
                    .expect("reporting mode does not fail");
                handled += usize::from(outcome.is_handled());
            }
            let _ = findings.drain();
            black_box(handled)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_react_compiler);
criterion_main!(benches);
