use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use uf_config::{UniflowedConfig, extract_config_object};

fn bench_define_config_parse(c: &mut Criterion) {
    let source = r#"
        import { defineConfig } from '@uniflowed/config';

        export default defineConfig({
          dev: { port: 3000 },
          app: {
            router: { entry: 'app.js', root: 'app' },
            builtins: {
              relay: true,
              cell: true,
            },
          },
          lint: {
            rules: {
              'flow/type-aware/no-explicit-any': 'error',
              'react/no-render-side-effects': 'error',
            },
          },
        });
    "#;

    c.bench_function("parse defineConfig object", |b| {
        b.iter(|| {
            let object = extract_config_object(source).expect("config object");
            let config: UniflowedConfig = json5::from_str(&object).expect("config");
            black_box(config)
        });
    });
}

criterion_group!(benches, bench_define_config_parse);
criterion_main!(benches);
