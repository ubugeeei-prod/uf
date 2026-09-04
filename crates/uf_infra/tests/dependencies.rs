//! What uf's own crates are allowed to depend on.
//!
//! uf schedules its own work. `uf_infra::parallel` is the one data-parallel
//! primitive the toolchain needs, `uf_test` runs its own worker pool, and
//! nothing in uf is asynchronous — so neither a parallel iterator framework nor
//! an async runtime has a place in the dependency graph, and the way to keep
//! one out is to say so somewhere that fails.
//!
//! This is about uf's crates only. `upstream/flow` is Meta's source, vendored
//! as a submodule, and it uses rayon itself; that is its business and not ours
//! to rewrite.

use std::fs;
use std::path::{Path, PathBuf};

/// Crates uf may not depend on, and what to reach for instead.
const BANNED: &[(&str, &str)] = &[
    ("rayon", "uf_infra::parallel::map"),
    ("tokio", "std::thread, as uf_test does"),
    ("async-std", "std::thread, as uf_test does"),
    ("smol", "std::thread, as uf_test does"),
    ("futures", "std::thread, as uf_test does"),
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the crate lives inside the workspace")
}

/// Every `crates/*/Cargo.toml`, which is every crate uf owns.
fn uf_manifests() -> Vec<PathBuf> {
    let mut manifests = fs::read_dir(workspace_root().join("crates"))
        .expect("the workspace has a crates directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("Cargo.toml"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    manifests.sort();
    manifests
}

/// The dependency names one manifest declares, in any of its three tables.
fn declared_dependencies(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && !line.starts_with('['))
        .filter_map(|line| line.split_once(['=', '.']))
        .map(|(name, _)| name.trim().trim_matches('"').to_owned())
        .collect()
}

#[test]
fn no_uf_crate_depends_on_a_parallel_or_async_runtime() {
    let mut offenders = Vec::new();

    for manifest in uf_manifests() {
        let source = fs::read_to_string(&manifest).expect("a readable manifest");
        let declared = declared_dependencies(&source);
        for (banned, instead) in BANNED {
            if declared.iter().any(|name| name == banned) {
                let crate_name = manifest
                    .parent()
                    .and_then(Path::file_name)
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                offenders.push(format!("{crate_name} depends on {banned}; use {instead}"));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "{}\n\nuf schedules its own work; see crates/uf_infra/src/parallel.rs",
        offenders.join("\n")
    );
}

#[test]
fn the_workspace_does_not_offer_one_either() {
    let root = fs::read_to_string(workspace_root().join("Cargo.toml")).expect("a root manifest");
    let declared = declared_dependencies(&root);

    for (banned, instead) in BANNED {
        assert!(
            !declared.iter().any(|name| name == banned),
            "the workspace still offers {banned} to any crate that asks; use {instead}"
        );
    }
}

/// The parser reads what a manifest actually declares, in every table shape.
#[test]
fn dependencies_are_read_from_every_table_shape() {
    let manifest = r#"
[package]
name = "example"

[dependencies]
plain = "1"
detailed = { version = "1", features = ["x"] }
workspace-style.workspace = true

[dev-dependencies]
only-for-tests = "1"

[build-dependencies]
only-for-build = "1"
"#;

    let declared = declared_dependencies(manifest);

    for expected in [
        "plain",
        "detailed",
        "workspace-style",
        "only-for-tests",
        "only-for-build",
    ] {
        assert!(
            declared.iter().any(|name| name == expected),
            "{expected} was not read out of the manifest, got {declared:?}"
        );
    }
}
