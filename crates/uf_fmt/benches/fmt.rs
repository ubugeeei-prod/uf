use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_config::FmtConfig;
use uf_fmt::format_source;

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

/// A module that is mostly comments, which is what this repository's own
/// source looks like and what the synthetic module above has none of.
///
/// Comment attachment descends the tree once per comment, so its cost is the
/// product of the two — a module with many comments *and* many nodes is the
/// only shape that shows it, and neither half alone does. Before the children
/// of a visited node were cached, formatting
/// `packages/router/internal/runtime.js` allocated 12.41 MiB; after, 7.95 MiB,
/// with the peak unchanged. This is the bench that would notice that going
/// away again.
fn commented_source(components: usize) -> String {
    let mut source = String::with_capacity(components * 1024);
    source.push_str("// @flow\n\nimport * as React from \"react\";\n\n");
    for index in 0..components {
        source.push_str(&format!(
            r#"
/**
 * The {index}th panel, and why it is written the way it is.
 *
 * A block comment above a declaration is the commonest shape there is, and
 * the one comment attachment has to place by descending from the root.
 */
type Props{index} = {{|
  // The identity, which the parent owns.
  +id: string,
  // How many times it has been asked to count, or nothing yet.
  +count?: ?number,
|}};

// A leading line comment, which attaches differently from a block.
component Panel{index}(props: Props{index}) renders React.Node {{
  // Inside the body, where the enclosing node is not the program.
  const [count, setCount] = React.useState<number>(props.count ?? 0); // and trailing
  /* an inline block */
  return <div id={{props.id}}>{{count}}</div>;
}}
"#
        ));
    }
    source
}

fn bench_format(criterion: &mut Criterion) {
    let config = FmtConfig::default();
    let source = synthetic_source(120);
    let commented = commented_source(120);
    let bytes = source.len() as u64;

    let mut group = criterion.benchmark_group("uf_fmt");
    group.throughput(Throughput::Bytes(bytes));

    // The parser is the floor the formatter builds on: everything the
    // printer does happens after this, so the two numbers together say
    // where the time goes.
    group.bench_function("parse large flow react file", |bencher| {
        bencher.iter(|| black_box(uf_flow::parse(black_box(&source)).expect("parses")));
    });
    group.bench_function("format large flow react file", |bencher| {
        bencher.iter(|| black_box(format_source(black_box(&source), &config).expect("format")));
    });

    // Already-formatted input is the common `uf fmt --check` case.
    let formatted = format_source(&source, &config).expect("format").output;
    group.throughput(Throughput::Bytes(formatted.len() as u64));
    group.bench_function("format heavily commented file", |bencher| {
        bencher.iter(|| {
            black_box(uf_fmt::format_source(black_box(&commented), black_box(&config)).unwrap())
        });
    });

    group.bench_function("format already formatted file", |bencher| {
        bencher.iter(|| black_box(format_source(black_box(&formatted), &config).expect("format")));
    });
    group.finish();
}

criterion_group!(benches, bench_format);
criterion_main!(benches);
