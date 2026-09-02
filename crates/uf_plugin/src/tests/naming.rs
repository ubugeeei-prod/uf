//! uf borrows the plugin *semantics* of the existing ecosystem so those plugins
//! keep working. It does not borrow the names.
//!
//! A developer using uf writes `uf.config.js` and `.js` files with `// @flow`.
//! They never chose an underlying bundler, so no id, no diagnostic, no report
//! field, and no generated file may hand them one to reason about. These tests
//! read this crate's own sources and the project templates and fail if an
//! engine name appears anywhere a user could see it.

use std::fs;
use std::path::{Path, PathBuf};

use camino::Utf8PathBuf;
use uf_config::{HookOrder, PipelineMode, UniflowedConfig};

use crate::builtin::{BuiltinPlugin, BuiltinSet};
use crate::container::PluginContainer;
use crate::descriptor::{PluginOrigin, PluginSource};
use crate::hook::{HookDispatch, PluginHook};

/// The engines uf drives internally and never names to a user.
const ENGINE_NAMES: [&str; 3] = ["vite", "rolldown", "rollup"];

/// Source trees whose user-visible strings are checked.
///
/// This crate, because every id it invents ends up in `uf inspect --json`, and
/// the project templates, because they become the files in someone's
/// repository.
const CHECKED_TREES: [&str; 3] = ["src", "benches", "../uf_project/src"];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Collect the `.rs` files under `dir` that end up in a shipped binary.
///
/// `#[cfg(test)]` trees are skipped: they are the one place the engines have to
/// be named, because a test for "this name never reaches a user" has to say the
/// name to look for it, and none of that code is ever compiled into `uf`.
fn rust_sources(dir: &Path, into: &mut Vec<PathBuf>) {
    if dir.file_name().is_some_and(|name| name == "tests") {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(path);
        }
    }
}

/// Strip `//` line comments, keeping everything a user could see.
///
/// Comments are where the engines *should* be named: this crate has to explain
/// which semantics it matches, and that explanation is for people reading the
/// code, not for people running `uf`.
fn without_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.find("//") {
            Some(index) => &line[..index],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn no_user_visible_string_names_the_underlying_engines() {
    let root = crate_dir();
    let mut sources = Vec::new();
    for tree in CHECKED_TREES {
        rust_sources(&root.join(tree), &mut sources);
    }
    assert!(
        sources.len() > 8,
        "the grep found almost nothing, so it is not actually checking anything: {sources:?}"
    );

    let mut leaks = Vec::new();
    for path in sources {
        let source = fs::read_to_string(&path).expect("readable source");
        let checked = without_line_comments(&source).to_ascii_lowercase();
        for engine in ENGINE_NAMES {
            for (line_number, line) in checked.lines().enumerate() {
                if line.contains(engine) {
                    leaks.push(format!("{}:{}: {engine}", path.display(), line_number + 1));
                }
            }
        }
    }

    assert!(
        leaks.is_empty(),
        "an engine name reached a user-visible string:\n{}",
        leaks.join("\n")
    );
}

#[test]
fn no_builtin_plugin_names_an_engine() {
    for plugin in BuiltinPlugin::ALL {
        let name = plugin.name().to_ascii_lowercase();
        for engine in ENGINE_NAMES {
            assert!(!name.contains(engine), "{name} names {engine}");
        }
    }
}

#[test]
fn no_hook_id_names_an_engine() {
    for hook in PluginHook::ALL {
        let id = hook.as_str().to_ascii_lowercase();
        for engine in ENGINE_NAMES {
            assert!(!id.contains(engine), "{id} names {engine}");
        }
    }
}

#[test]
fn no_enum_id_in_the_public_vocabulary_names_an_engine() {
    let mut ids = Vec::new();
    ids.extend(HookOrder::ALL.map(HookOrder::as_str));
    ids.extend(uf_config::ApplyCondition::ALL.map(uf_config::ApplyCondition::as_str));
    ids.extend(PipelineMode::ALL.map(PipelineMode::as_str));
    ids.extend([
        HookDispatch::Broadcast.as_str(),
        HookDispatch::FirstWins.as_str(),
        HookDispatch::Chained.as_str(),
        PluginOrigin::Builtin.as_str(),
        PluginOrigin::Project.as_str(),
        PluginSource::Builtin.kind(),
    ]);

    for id in ids {
        let lowered = id.to_ascii_lowercase();
        for engine in ENGINE_NAMES {
            assert!(!lowered.contains(engine), "{id} names {engine}");
        }
    }
}

#[test]
fn the_inspect_report_never_names_an_engine() {
    let config = UniflowedConfig::default();
    let container = PluginContainer::from_descriptors(
        PipelineMode::Build,
        BuiltinSet::from_config(&config).descriptors(),
    )
    .expect("container");

    let json = serde_json::to_string(&container.report())
        .expect("serializes")
        .to_ascii_lowercase();

    for engine in ENGINE_NAMES {
        assert!(!json.contains(engine), "the report names {engine}: {json}");
    }
}

#[test]
fn a_resolved_project_pipeline_never_names_an_engine() {
    let mut config = UniflowedConfig::default();
    config.plugins = vec![
        uf_config::PluginEntry::Name("@uniflowed/plugin-mdx".into()),
        uf_config::PluginEntry::Spec(
            uf_config::PluginSpec::new("./plugins/metrics.js")
                .with_order(HookOrder::Post)
                .with_apply(uf_config::ApplyCondition::Build),
        ),
    ];
    let root = Utf8PathBuf::from("/workspace/app");

    let container = crate::resolve_pipeline(&config, &root, PipelineMode::Build).expect("resolves");
    let json = serde_json::to_string(&container.report())
        .expect("serializes")
        .to_ascii_lowercase();

    for engine in ENGINE_NAMES {
        assert!(!json.contains(engine), "the report names {engine}");
    }
}

#[test]
fn the_grep_would_notice_a_leak() {
    // Without this the test above could be passing because it reads nothing.
    let leaky = "let backend = \"rolldown\"; // rollup\n";

    assert!(
        without_line_comments(leaky).contains("rolldown"),
        "the grep must see code"
    );
    assert!(
        !without_line_comments(leaky).contains("rollup"),
        "the grep must not see line comments"
    );
}
