use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_config::{RuleLevel, UniflowedConfig};
use uf_infra::CompactString;
use uf_lint::{SourceFile, lint_sources};

/// A module that touches every source-text rule in the catalogue, so the scan
/// benchmark measures the real rule set rather than a happy-path fast exit.
const RULE_HEAVY_MODULE: &str = r#"// @flow
'use client';
import { format } from '@uniflowed/intl';
import { renderMarkdown } from '@uniflowed/markdown';

type Props = {| +id: string, +title: string |};
type Meta = { +tags: Array<string>, ... };

opaque type Token = string;

hook useTitle(props: Props): string {
  const [title, setTitle] = useState(props.title);
  useEffect(() => {
    setTitle(format(props.title));
  });
  return title;
}

export component Article(props: Props) {
  const title = useTitle(props);
  const body = renderMarkdown(props.id);
  return (
    <article>
      <h1>{title}</h1>
      <div dangerouslySetInnerHTML={{ __html: body }} />
    </article>
  );
}

export const helpers = {
  merge(base: Meta, patch: Meta): Meta {
    return { ...base, ...patch };
  },
};
"#;

/// The same module with a violation of most source-text rules, so the benchmark
/// also measures the diagnostic-building path.
const VIOLATING_MODULE: &str = r#"// @flow
import { a } from './a.js';
'use client';

type Props = { id: any };

export let counter = 0;

const div = 1;

class Box {
  get value(): bool { return true; }
}

component Outer() {
  component Inner() { return null; }
  if (counter) {
    const [x] = useState(0);
  }
  return <Inner />;
}

export default Outer;

const merged = Object.assign({}, a);
const legacy = require('./legacy.js');
eval('boom');
spawn('npm run build');
"#;

/// The default rule set with `flow/syntax` switched off.
///
/// `flow/syntax` hands the file to the official Flow parser (a QuickJS runtime
/// inside `uf_flow`), which is a different subsystem with its own cost and its
/// own scaling behaviour — at the time of writing it exhausts the JS stack when
/// rayon fans a few hundred files across worker threads in an optimized build.
/// Every other rule in the catalogue stays on, so these benchmarks measure the
/// native scan on the real rule set rather than the embedded parser.
fn scan_only_config() -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config
        .lint
        .rules
        .insert(CompactString::const_new("flow/syntax"), RuleLevel::Off);
    config
}

fn corpus(module: &str, files: usize) -> Vec<SourceFile> {
    (0..files)
        .map(|index| SourceFile {
            path: format!("app/route{index}/_uf.page.js"),
            source: module.to_string(),
        })
        .collect()
}

fn bench_lint_scan(c: &mut Criterion) {
    let config = scan_only_config();
    let files = corpus("// @flow\ncomponent Page() { return <main />; }\n", 1_000);

    c.bench_function("lint 1000 flow route files", |b| {
        b.iter(|| black_box(lint_sources(&files, &config).expect("lint")));
    });
}

fn bench_rule_set_throughput(c: &mut Criterion) {
    let config = scan_only_config();
    let mut group = c.benchmark_group("lint_rule_set");

    for (name, module) in [
        ("clean", RULE_HEAVY_MODULE),
        ("violating", VIOLATING_MODULE),
    ] {
        let files = corpus(module, 1_000);
        let bytes = files.iter().map(|file| file.source.len() as u64).sum();
        group.throughput(Throughput::Bytes(bytes));
        group.bench_function(name, |b| {
            b.iter(|| black_box(lint_sources(&files, &config).expect("lint")));
        });
    }

    group.finish();
}

fn bench_suppression_scan(c: &mut Criterion) {
    let config = scan_only_config();
    let module = format!(
        "// @flow\n// uf-lint-disable flow/unclear-type\n{}// uf-lint-enable flow/unclear-type\n",
        "type A = any;\n".repeat(200)
    );
    let files = corpus(&module, 200);
    let bytes = files.iter().map(|file| file.source.len() as u64).sum();

    let mut group = c.benchmark_group("lint_suppressions");
    group.throughput(Throughput::Bytes(bytes));
    group.bench_function("block suppressed", |b| {
        b.iter(|| black_box(lint_sources(&files, &config).expect("lint")));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_lint_scan,
    bench_rule_set_throughput,
    bench_suppression_scan
);
criterion_main!(benches);
