use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_config::FmtConfig;
use uf_fmt::{format_source, lexer::tokenize};

/// Build a synthetic Flow + JSX source file of roughly `components` components.
fn synthetic_source(components: usize) -> String {
    let mut source = String::with_capacity(components * 512);
    source.push_str("// @flow\n\"use client\";\n\nimport * as React from \"react\";\n\n");
    for index in 0..components {
        source.push_str(&format!(
            r#"
type Props{index} = {{|
  +id: string,
  +count?: ?number,
  +items: Array<Map<string, number>>,
  +onSelect: (value: string) => void,
|}};

opaque type Id{index} = string;

const RATIO_{index} = (1_000 + 0x1f) / 2;
const PATTERN_{index} = /^[a-z]+\/(\d+)$/gi;

hook useCounter{index}(initial: number): [number, () => void] {{
  const [value, setValue] = React.useState<number>(initial);
  const bump = React.useCallback(() => setValue((previous) => previous + 1), []);
  return [value, bump];
}}

component Panel{index}(props: Props{index}) renders React.Node {{
  const [count, bump] = useCounter{index}(props.count ?? 0);
  const label = `panel-${{props.id}}-${{count > 0 ? "on" : "off"}}`;

  switch (count % 3) {{
    case 0:
      break;
    case 1:
      bump();
      break;
    default:
      // Nothing to do for the remaining case.
      break;
  }}

  return (
    <section className="panel" data-testid={{label}} onClick={{bump}}>
      <h2>Panel {{props.id}} — {{count}} items</h2>
      <ul>
        {{props.items.map((item, position) => (
          <li key={{position}} title={{item.get("name") ?? "unknown"}}>
            {{position}}: {{item.size}}
          </li>
        ))}}
      </ul>
    </section>
  );
}}
"#
        ));
    }
    source
}

fn bench_format(criterion: &mut Criterion) {
    let config = FmtConfig::default();
    let source = synthetic_source(120);
    let bytes = source.len() as u64;

    let mut group = criterion.benchmark_group("uf_fmt");
    group.throughput(Throughput::Bytes(bytes));
    group.bench_function("tokenize large flow react file", |bencher| {
        bencher.iter(|| black_box(tokenize(black_box(&source)).len()));
    });
    group.bench_function("format large flow react file", |bencher| {
        bencher.iter(|| black_box(format_source(black_box(&source), &config).expect("format")));
    });

    // Already-formatted input is the common `uf fmt --check` case.
    let formatted = format_source(&source, &config).expect("format").output;
    group.throughput(Throughput::Bytes(formatted.len() as u64));
    group.bench_function("format already formatted file", |bencher| {
        bencher.iter(|| black_box(format_source(black_box(&formatted), &config).expect("format")));
    });
    group.finish();
}

criterion_group!(benches, bench_format);
criterion_main!(benches);
