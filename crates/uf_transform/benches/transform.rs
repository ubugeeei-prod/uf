//! Throughput of the whole Flow → JavaScript pipeline on a realistic module.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use uf_transform::{ReactCompilerMode, TransformOptions, transform};

const COMPONENT: &str = r#"// @flow
import * as React from "react";
import { useState } from "react";
import type { Node } from "react";

type Item = {| +id: string, +label: string, +done: boolean |};

enum Filter { All, Active, Done }

export component TodoList(items: $ReadOnlyArray<Item>, onToggle: (id: string) => void) {
  const [filter, setFilter] = useState<Filter>(Filter.All);
  const visible = items.filter((item) =>
    match (filter) {
      Filter.All => true,
      Filter.Active => !item.done,
      Filter.Done => item.done,
    },
  );
  return (
    <section className="todos">
      <header>
        <button onClick={() => setFilter(Filter.All)}>All</button>
        <button onClick={() => setFilter(Filter.Active)}>Active</button>
        <button onClick={() => setFilter(Filter.Done)}>Done</button>
      </header>
      <ul>
        {visible.map((item) => (
          <li key={item.id} className={item.done ? "done" : ""}>
            <label>
              <input type="checkbox" checked={item.done} onChange={() => onToggle(item.id)} />
              {item.label}
            </label>
          </li>
        ))}
      </ul>
    </section>
  );
}

export hook useCounter(initial: number = 0): [number, () => void] {
  const [value, setValue] = useState(initial);
  return [value, () => setValue((v) => v + 1)];
}
"#;

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("transform");
    group.throughput(Throughput::Bytes(COMPONENT.len() as u64));
    group.bench_function("component (compiler on)", |b| {
        let options = TransformOptions::new("todo.js");
        b.iter(|| transform(COMPONENT, &options).unwrap());
    });
    group.bench_function("component (compiler off)", |b| {
        let options = TransformOptions {
            react_compiler: ReactCompilerMode::Off,
            ..TransformOptions::new("todo.js")
        };
        b.iter(|| transform(COMPONENT, &options).unwrap());
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
