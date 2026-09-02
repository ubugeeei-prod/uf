//! Per-module dispatch cost through a realistically sized pipeline.
//!
//! Dispatch runs once per module per hook, so its cost is multiplied by the
//! size of the app. The two cases below are the ones that matter: the common
//! one, where a module passes through every plugin untouched and the container
//! must not allocate at all, and the worst one, where every plugin rewrites the
//! module and each rewrite is a real string.

use std::hint::black_box;

use compact_str::CompactString;
use uf_config::{ApplyCondition, HookOrder, PipelineMode};
use uf_plugin::{
    FnPlugin, HookOutcome, HookSet, ModuleCode, Plugin, PluginContainer, PluginDescriptor,
    PluginHook, PluginSource, ResolvedId,
};

use criterion::{Criterion, criterion_group, criterion_main};

const PLUGINS: usize = 20;
const MODULES: usize = 100_000;

fn descriptor(name: String, order: HookOrder) -> PluginDescriptor {
    let specifier = CompactString::new(&name);
    PluginDescriptor::project(
        name,
        PluginSource::Package { specifier },
        order,
        ApplyCondition::Always,
        HookSet::EMPTY,
    )
}

fn band(index: usize) -> HookOrder {
    match index % 3 {
        0 => HookOrder::Pre,
        1 => HookOrder::Normal,
        _ => HookOrder::Post,
    }
}

/// Twenty plugins that all decline: the common case for most modules.
fn declining_pipeline() -> PluginContainer {
    let plugins = (0..PLUGINS)
        .map(|index| {
            Box::new(
                FnPlugin::new(descriptor(format!("declines-{index}"), band(index)))
                    .on_transform(|_| Ok(HookOutcome::Passthrough))
                    .on_resolve_id(|_| Ok(HookOutcome::Passthrough)),
            ) as Box<dyn Plugin>
        })
        .collect();
    PluginContainer::build(PipelineMode::Build, plugins).expect("container")
}

/// Twenty plugins that all rewrite the module.
fn rewriting_pipeline() -> PluginContainer {
    let plugins = (0..PLUGINS)
        .map(|index| {
            Box::new(
                FnPlugin::new(descriptor(format!("rewrites-{index}"), band(index))).on_transform(
                    |input| {
                        let mut code = String::with_capacity(input.code.len() + 8);
                        code.push_str(input.code);
                        code.push_str("\n// pass\n");
                        Ok(HookOutcome::Handled(ModuleCode::new(code)))
                    },
                ),
            ) as Box<dyn Plugin>
        })
        .collect();
    PluginContainer::build(PipelineMode::Build, plugins).expect("container")
}

/// One plugin near the end of the order answers; the rest decline.
fn resolving_pipeline() -> PluginContainer {
    let mut plugins = (0..PLUGINS - 1)
        .map(|index| {
            Box::new(
                FnPlugin::new(descriptor(format!("declines-{index}"), HookOrder::Pre))
                    .on_resolve_id(|_| Ok(HookOutcome::Passthrough)),
            ) as Box<dyn Plugin>
        })
        .collect::<Vec<_>>();
    plugins.push(Box::new(
        FnPlugin::new(descriptor("answers".to_string(), HookOrder::Post))
            .on_resolve_id(|input| Ok(HookOutcome::Handled(ResolvedId::bundled(input.specifier)))),
    ));
    PluginContainer::build(PipelineMode::Build, plugins).expect("container")
}

fn module_ids() -> Vec<String> {
    (0..MODULES)
        .map(|index| format!("app/routes/section{}/module{index}.js", index % 64))
        .collect()
}

fn bench_dispatch(criterion: &mut Criterion) {
    let ids = module_ids();
    let source = "// @flow\nexport default function Page() { return null; }\n";

    let declining = declining_pipeline();
    assert_eq!(declining.len(), PLUGINS);
    criterion.bench_function("transform 100k modules through 20 declining plugins", |b| {
        b.iter(|| {
            for id in &ids {
                let outcome = declining
                    .transform(black_box(id), black_box(source))
                    .expect("transforms");
                black_box(outcome.is_passthrough());
            }
        });
    });

    let rewriting = rewriting_pipeline();
    criterion.bench_function("transform 100k modules through 20 rewriting plugins", |b| {
        b.iter(|| {
            for id in &ids {
                let outcome = rewriting
                    .transform(black_box(id), black_box(source))
                    .expect("transforms");
                black_box(outcome.handled().map(|code| code.code.len()));
            }
        });
    });

    let resolving = resolving_pipeline();
    criterion.bench_function("resolve 100k specifiers through 20 plugins", |b| {
        b.iter(|| {
            for id in &ids {
                let outcome = resolving
                    .resolve_id(black_box(id), black_box(Some("app/page.js")))
                    .expect("resolves");
                black_box(outcome.is_handled());
            }
        });
    });

    criterion.bench_function("hook mask test", |b| {
        b.iter(|| black_box(declining.implements(black_box(PluginHook::Transform))));
    });
}

criterion_group!(benches, bench_dispatch);
criterion_main!(benches);
