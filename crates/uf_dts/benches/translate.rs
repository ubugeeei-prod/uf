use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

/// A declaration file in the shape `tsc` writes for a schema library: an
/// interface and a constructor per schema, generics with bounds and defaults,
/// conditional and mapped types, and re-exports at the end.
fn declaration_file(schemas: usize) -> String {
    let mut source = String::from(
        "import type { $ZodTypeInternals } from \"./core.js\";\n\
         export type output<T> = T extends { _output: infer O } ? O : unknown;\n\
         export type Partialize<T> = { [K in keyof T]?: T[K] };\n",
    );
    for index in 0..schemas {
        source.push_str(&format!(
            "export interface Schema{index}<out Output = unknown, out Input = unknown> {{\n\
             \x20   readonly _output: Output;\n\
             \x20   parse(data: unknown): Output;\n\
             \x20   optional(): Schema{index}<Output | undefined, Input | undefined>;\n\
             \x20   refine<R extends Output>(check: (value: Output) => value is R): Schema{index}<R, Input>;\n\
             \x20   [key: string]: unknown;\n\
             }}\n\
             export declare const Schema{index}: {{ new (): Schema{index}; readonly name: \"Schema{index}\" }};\n"
        ));
    }
    source.push_str("export {};\n");
    source
}

fn bench_translate(c: &mut Criterion) {
    let entry = declaration_file(200);
    let core = "export interface $ZodTypeInternals { output: unknown }\n".to_owned();
    c.bench_function("translate a 200-schema declaration file", |b| {
        b.iter(|| {
            let translation = uf_dts::translate(&["index.d.ts"], &mut |path| match path {
                "index.d.ts" => Some(entry.clone()),
                "core.d.ts" => Some(core.clone()),
                _ => None,
            });
            black_box(translation)
        });
    });
}

criterion_group!(benches, bench_translate);
criterion_main!(benches);
