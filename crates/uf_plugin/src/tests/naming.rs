//! uf borrows the plugin *semantics* of the existing ecosystem so those plugins
//! keep working. It does not borrow the bundler names.
//!
//! A developer using uf writes `uf.config.js` and `.js` files with `// @flow`.
//! They never chose an underlying bundler, so no id, no diagnostic, no report
//! field, and no generated file may hand them one to reason about. Vite Task is
//! different: it is the public task-runner engine in `uf.config.js`, not a hidden
//! bundler implementation detail.

use std::fs;
use std::path::{Path, PathBuf};

use camino::Utf8PathBuf;
use uf_config::{HookOrder, PipelineMode, UniflowedConfig};

use crate::builtin::{BuiltinPlugin, BuiltinSet};
use crate::container::PluginContainer;
use crate::descriptor::{PluginOrigin, PluginSource};
use crate::hook::{HookDispatch, PluginHook};

/// The bundler engines uf drives internally and never names to a user.
const HIDDEN_BUNDLER_ENGINE_NAMES: [&str; 2] = ["rolldown", "rollup"];

/// Source trees whose user-visible strings are checked.
///
/// Everything that decides what a user reads: this crate, because every id it
/// invents ends up in `uf inspect --json`; the CLI, because it writes both the
/// terminal output and the manifests that land in someone's `dist/`; the config
/// crate, because its enum spellings are the vocabulary of `uf.config.js`; and
/// the project templates, because they become the files in someone's repository.
const CHECKED_TREES: [&str; 5] = [
    "src",
    "benches",
    "../uf_cli/src",
    "../uf_config/src",
    "../uf_project/src",
];

/// Shipped JavaScript trees whose user-visible strings are checked.
///
/// `@uniflowed/*` modules are published, so a comment in one is something a
/// user reads in their own `node_modules`.
const CHECKED_JS_TREES: [&str; 1] = ["../../packages"];

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Collect the `.js` files under `dir` that are published to users.
fn js_sources(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            js_sources(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "js") {
            into.push(path);
        }
    }
}

/// Collect the `.rs` files under `dir` that end up in a shipped binary.
///
/// `#[cfg(test)]` code is skipped, as a `tests` directory or a `tests.rs`
/// sibling: it is the one place the engines have to be named, because a test
/// for "this name never reaches a user" has to say the name to look for it, and
/// a fixture may legitimately contain a *user's* task command such as
/// `vite --host 0.0.0.0`. None of it is ever compiled into `uf`.
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
        } else if path.file_name().is_some_and(|name| name == "tests.rs") {
            continue;
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
        sources.len() > 25,
        "the grep found almost nothing, so it is not actually checking anything: {sources:?}"
    );

    let mut leaks = Vec::new();
    for path in sources {
        let source = fs::read_to_string(&path).expect("readable source");
        let checked = without_line_comments(&source).to_ascii_lowercase();
        for engine in HIDDEN_BUNDLER_ENGINE_NAMES {
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

/// The shipped `@uniflowed/*` modules are read in someone's `node_modules`, so
/// their comments count as user-visible too — unlike this repository's own Rust
/// comments, which are for people editing `uf`.
#[test]
fn no_shipped_javascript_names_the_underlying_engines() {
    let root = crate_dir();
    let mut sources = Vec::new();
    for tree in CHECKED_JS_TREES {
        js_sources(&root.join(tree), &mut sources);
    }
    assert!(
        sources.len() > 20,
        "the grep found almost nothing, so it is not actually checking anything: {sources:?}"
    );

    let mut leaks = Vec::new();
    for path in sources {
        // `@uniflowed/vite` is the seam between uf and the engine it runs on,
        // so naming that engine is the one thing it is for. It is plumbing
        // `uf dev` and `uf build` wire up, not a module an application
        // imports — a user still writes only `uf.config.js`, which is what
        // this rule protects.
        if path.components().any(|part| part.as_os_str() == "vite") {
            continue;
        }
        let source = fs::read_to_string(&path).expect("readable source");
        for engine in HIDDEN_BUNDLER_ENGINE_NAMES {
            for (line_number, line) in source.to_ascii_lowercase().lines().enumerate() {
                if line.contains(engine) {
                    leaks.push(format!("{}:{}: {engine}", path.display(), line_number + 1));
                }
            }
        }
    }

    assert!(
        leaks.is_empty(),
        "an engine name reached a shipped module:\n{}",
        leaks.join("\n")
    );
}

#[test]
fn no_builtin_plugin_names_an_engine() {
    for plugin in BuiltinPlugin::ALL {
        let name = plugin.name().to_ascii_lowercase();
        for engine in HIDDEN_BUNDLER_ENGINE_NAMES {
            assert!(!name.contains(engine), "{name} names {engine}");
        }
    }
}

#[test]
fn no_hook_id_names_an_engine() {
    for hook in PluginHook::ALL {
        let id = hook.as_str().to_ascii_lowercase();
        for engine in HIDDEN_BUNDLER_ENGINE_NAMES {
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
        for engine in HIDDEN_BUNDLER_ENGINE_NAMES {
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

    for engine in HIDDEN_BUNDLER_ENGINE_NAMES {
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

    for engine in HIDDEN_BUNDLER_ENGINE_NAMES {
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
