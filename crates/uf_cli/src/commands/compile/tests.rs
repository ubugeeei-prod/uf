//! What `--compile` decides before it builds anything.
//!
//! Every refusal in [`super`] happens before the bundler runs, which is what
//! makes it testable without a project, a runtime or a network: a triple, a
//! version, a permission set and a backend are all the input there is. The
//! end-to-end half — that the binary those decisions produce serves the
//! application — is `crates/uf_cli/tests/vite.rs`.

use camino::Utf8Path;
use serde_json::json;
use uf_config::{CapabilityJsHost, Permissions, UniflowedConfig};

use super::{
    Backend, NODE_SEA_FLOOR, TARGETS, Version, artefact_permissions, binary_name, finish,
    parse_target, refuse_target,
};

/// A permission set as `uf.config.js` would carry it.
///
/// Through serde rather than a struct literal, because `Permissions` is
/// `#[non_exhaustive]`: the only way to build one outside `uf_runtime` is the
/// way a project's config file builds one, which is the shape a test should
/// have been using anyway.
fn declared(value: serde_json::Value) -> Permissions {
    serde_json::from_value(value).expect("a permission set uf.config.js could carry")
}

fn config_on(host: CapabilityJsHost) -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.app.runtime.capability_js_host.default = host;
    config
}

#[test]
fn a_triple_uf_names_resolves_to_bun_s_name_for_it() {
    let target = parse_target("x86_64-unknown-linux-gnu").unwrap();
    assert_eq!(target.bun, "bun-linux-x64");
    assert!(!target.windows);

    let windows = parse_target("x86_64-pc-windows-msvc").unwrap();
    assert!(
        windows.windows,
        "a Windows target's executable is named `.exe`, and Bun appends one whether uf does or not"
    );
}

#[test]
fn an_unknown_triple_is_refused_with_the_list() {
    let error = parse_target("sparc-sun-solaris").unwrap_err().to_string();
    assert!(error.contains("sparc-sun-solaris"), "{error}");
    for target in TARGETS {
        assert!(
            error.contains(target.triple),
            "the refusal has to be the list, and {} is missing:\n{error}",
            target.triple
        );
    }
}

#[test]
fn bun_s_own_name_for_a_machine_is_answered_with_the_triple() {
    // A reader who typed `bun-linux-x64` named a real platform correctly, in
    // the other vocabulary. Handing them the list to search would make them
    // find a line they could already have guessed; naming the triple is one
    // sentence and no searching.
    let error = parse_target("bun-linux-x64").unwrap_err().to_string();
    assert!(error.contains("x86_64-unknown-linux-gnu"), "{error}");
    assert!(error.contains("Bun's name"), "{error}");
}

#[test]
fn every_target_uf_names_is_distinct_in_all_three_vocabularies() {
    // Two entries that shared a triple would make `--target` ambiguous, two
    // that shared Bun's name would silently build the same binary twice, and
    // two that shared a cache name would each think the other's runtime was
    // theirs. All three are the kind of table typo nothing else would catch.
    for (at, target) in TARGETS.iter().enumerate() {
        for other in &TARGETS[at + 1..] {
            assert_ne!(target.triple, other.triple);
            assert_ne!(target.bun, other.bun);
            assert_ne!(target.runtime, other.runtime);
        }
    }
}

#[test]
fn an_arm64_target_is_cached_under_the_name_bun_writes() {
    // The bug this pins: Bun takes `arm64` on the command line and writes
    // `aarch64` in the cache filename, so a lookup keyed on the argument never
    // matched. Every build for such a target announced a download it might not
    // need, and none of them reported the one it did — which is exactly the
    // pair of claims ubugeeei-prod/uf#310 asks `--target` to get right.
    for target in TARGETS {
        let expected = target.bun.replace("arm64", "aarch64");
        assert_eq!(
            target.runtime, expected,
            "{} is cached under a name Bun does not write",
            target.triple
        );
    }
}

#[test]
fn versions_compare_by_number_and_ignore_a_suffix() {
    assert_eq!(Version::parse("v25.8.1").unwrap().to_string(), "25.8.1");
    assert_eq!(Version::parse("1.1.27").unwrap().to_string(), "1.1.27");
    assert!(Version::parse("v26.0.0-nightly20260901").unwrap() >= NODE_SEA_FLOOR);
    assert!(Version::parse("v24.19.0").unwrap() < NODE_SEA_FLOOR);
    assert!(Version::parse("not a version").is_none());
}

#[test]
fn node_is_not_a_backend_for_a_machine_that_is_not_this_one() {
    // The failure this prevents is not a crash. Without it, a Node project
    // asking for a Linux binary would get a working macOS one and find out on
    // the server.
    let target = parse_target("x86_64-unknown-linux-gnu").unwrap();
    let refusal = refuse_target(Backend::NodeSea, Some(target)).expect("node cannot cross-compile");
    assert!(refusal.contains("x86_64-unknown-linux-gnu"), "{refusal}");
    assert!(refusal.contains("no cross-compilation"), "{refusal}");
    assert!(
        refusal.contains("Bun"),
        "the reason has to name the backend that can:\n{refusal}"
    );

    // And nothing at all is refused when no target was asked for, which is
    // every ordinary `--compile` on a Node project.
    assert!(refuse_target(Backend::NodeSea, None).is_none());
    assert!(refuse_target(Backend::Bun, Some(target)).is_none());
}

#[test]
fn a_target_that_is_this_machine_needs_no_download() {
    let host = TARGETS
        .iter()
        .find(|target| target.is_host())
        .copied()
        .expect("the machine running the tests is one of the platforms uf names");
    let resolved = finish(
        Utf8Path::new("/tmp/project"),
        &config_on(CapabilityJsHost::Bun),
        Backend::Bun,
        "/usr/bin/bun".into(),
        "1.1.27".to_owned(),
        Some(host),
    )
    .unwrap();
    assert!(
        !resolved.fetches_a_runtime(),
        "Bun appends to the copy of itself that uf started; nothing is fetched for the host"
    );
}

#[test]
fn a_foreign_target_says_it_will_fetch_before_the_build_starts() {
    let foreign = TARGETS
        .iter()
        .find(|target| !target.is_host())
        .copied()
        .expect("eight platforms, and this machine is not all of them");
    let root = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(root.path()).unwrap();
    let resolved = finish(
        root,
        &config_on(CapabilityJsHost::Bun),
        Backend::Bun,
        "/usr/bin/bun".into(),
        "1.1.27".to_owned(),
        Some(foreign),
    )
    .unwrap();
    assert!(
        resolved.fetches_a_runtime(),
        "an empty project has no cached runtime for {}, so the build has to say it will download \
         one",
        foreign.triple
    );
}

#[test]
fn a_project_that_declares_no_permissions_bakes_no_flags_in() {
    assert!(
        artefact_permissions(Backend::NodeSea, None)
            .unwrap()
            .is_empty()
    );
    assert!(artefact_permissions(Backend::Bun, None).unwrap().is_empty());
}

#[test]
fn node_bakes_the_declared_permission_set_into_the_binary() {
    let permissions = declared(json!({ "read": ["/srv/data"] }));
    let argv = artefact_permissions(Backend::NodeSea, Some(&permissions)).unwrap();
    assert_eq!(
        argv,
        vec![
            "--permission".to_owned(),
            "--allow-fs-read=/srv/data".to_owned()
        ],
        "the artefact's set is the declared set and nothing else: a compiled binary loads no \
         project, so uf needs no grant of its own"
    );
}

#[test]
fn node_refuses_a_category_it_cannot_enforce_in_a_binary_either() {
    // The same rule ubugeeei-prod/uf#617 applies to a run. Node's permission
    // model has no network dimension, and a binary that shipped with four of
    // five flags would look sandboxed and not be.
    let permissions = declared(json!({ "net": ["example.com"] }));
    let error = artefact_permissions(Backend::NodeSea, Some(&permissions))
        .unwrap_err()
        .to_string();
    assert!(error.contains("net"), "{error}");
}

#[test]
fn bun_refuses_a_permission_set_rather_than_shipping_an_unenforced_one() {
    let permissions = declared(json!({ "read": ["/srv/data"] }));
    let error = artefact_permissions(Backend::Bun, Some(&permissions))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Bun has no permission model"), "{error}");
    assert!(
        error.contains("Compile on Node"),
        "the refusal has to name the backend that can enforce it:\n{error}"
    );
}

#[test]
fn a_windows_target_names_the_binary_the_way_windows_does() {
    let root = Utf8Path::new("/tmp/shop");
    assert_eq!(
        binary_name(root, Some(parse_target("x86_64-pc-windows-msvc").unwrap())),
        "shop.exe"
    );
    assert_eq!(
        binary_name(
            root,
            Some(parse_target("x86_64-unknown-linux-gnu").unwrap())
        ),
        "shop"
    );
}
