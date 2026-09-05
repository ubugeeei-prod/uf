use camino::Utf8PathBuf;

use super::*;

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

fn node(version: &str) -> Pin {
    Pin {
        tool: Tool::Node,
        version: version.to_owned(),
        platform: Platform {
            os: Os::Darwin,
            arch: Arch::Arm64,
        },
    }
}

/// An entry is written under a temporary name and renamed into place, so a
/// reader never sees a directory that is there but not finished.
#[test]
fn an_entry_is_adopted_whole_or_not_at_all() {
    let (_guard, root) = temp();
    let store = Store::new(root.join("store"));
    let pin = node("24.14.0");

    assert!(!store.has(&pin));

    let staged = store.staging(&pin).unwrap();
    std::fs::create_dir_all(staged.join("bin")).unwrap();
    std::fs::write(staged.join("bin/node"), "#!/bin/sh\n").unwrap();
    let installed = store.adopt(&pin, &staged).unwrap();

    assert!(store.has(&pin));
    assert_eq!(installed, store.path(&pin));
    assert!(installed.join("bin/node").is_file());
    assert!(!staged.exists(), "the staging directory is gone");
    assert_eq!(store.entries().unwrap(), vec!["node-24.14.0-darwin-arm64"]);
}

/// Installing the same pin twice is the same as installing it once, because
/// `uf install` runs it every time.
#[test]
fn adopting_an_entry_that_exists_keeps_the_one_that_is_there() {
    let (_guard, root) = temp();
    let store = Store::new(root.join("store"));
    let pin = node("24.14.0");

    let first = store.staging(&pin).unwrap();
    std::fs::write(first.join("marker"), "first").unwrap();
    store.adopt(&pin, &first).unwrap();

    let second = store.staging(&pin).unwrap();
    std::fs::write(second.join("marker"), "second").unwrap();
    store.adopt(&pin, &second).unwrap();

    assert_eq!(
        std::fs::read_to_string(store.path(&pin).join("marker")).unwrap(),
        "first",
        "the entry that was there is the one that stays"
    );
    assert!(!second.exists(), "the loser cleans up after itself");
}

/// A root names the repository and what it uses, and replaces what that
/// repository said before.
#[test]
fn registering_a_root_replaces_what_the_repository_used_to_hold() {
    let (_guard, root) = temp();
    let roots = Roots::new(root.join("roots"));
    let repository = root.join("project");
    std::fs::create_dir_all(&repository).unwrap();

    roots
        .register(&repository, &["node-24.14.0-darwin-arm64".to_owned()])
        .unwrap();
    roots
        .register(&repository, &["node-26.0.0-darwin-arm64".to_owned()])
        .unwrap();

    let all = roots.all().unwrap();
    assert_eq!(all.len(), 1, "one repository, one root");
    assert_eq!(all[0].1.entries, ["node-26.0.0-darwin-arm64"]);
    assert_eq!(all[0].1.repository, repository);
}

/// What no live repository names is garbage, whatever happened to it.
#[test]
fn collection_removes_what_no_repository_reaches() {
    let (_guard, root) = temp();
    let store = Store::new(root.join("store"));
    let roots = Roots::new(root.join("roots"));

    let used = node("24.14.0");
    let unused = node("22.9.0");
    for pin in [&used, &unused] {
        let staged = store.staging(pin).unwrap();
        std::fs::write(staged.join("marker"), "x").unwrap();
        store.adopt(pin, &staged).unwrap();
    }

    let repository = root.join("project");
    std::fs::create_dir_all(&repository).unwrap();
    roots.register(&repository, &[used.slug()]).unwrap();

    let plan = gc::plan(&store, &roots).unwrap();
    assert_eq!(plan.unreachable, [unused.slug()]);
    assert_eq!(plan.kept, 1);
    assert!(plan.dead_roots.is_empty());

    // The plan is what runs: a reader shown this is shown what happens.
    let (entries, dead) = gc::collect(&store, &roots, &plan).unwrap();
    assert_eq!((entries, dead), (1, 0));
    assert!(store.has(&used));
    assert!(!store.has(&unused));
}

/// A repository that is deleted stops holding its tools, in the same pass.
#[test]
fn a_repository_that_is_gone_releases_what_it_held() {
    let (_guard, root) = temp();
    let store = Store::new(root.join("store"));
    let roots = Roots::new(root.join("roots"));

    let pin = node("24.14.0");
    let staged = store.staging(&pin).unwrap();
    std::fs::write(staged.join("marker"), "x").unwrap();
    store.adopt(&pin, &staged).unwrap();

    let repository = root.join("deleted");
    std::fs::create_dir_all(&repository).unwrap();
    roots.register(&repository, &[pin.slug()]).unwrap();
    std::fs::remove_dir_all(&repository).unwrap();

    let plan = gc::plan(&store, &roots).unwrap();
    assert_eq!(plan.dead_roots.len(), 1, "the root is dead");
    assert_eq!(
        plan.unreachable,
        [pin.slug()],
        "and its entry is unreachable in the same pass"
    );

    gc::collect(&store, &roots, &plan).unwrap();
    assert!(!store.has(&pin));
    assert!(roots.all().unwrap().is_empty());
}

/// An interrupted install leaves a staging directory. Nothing will ever link
/// it, so collection takes it.
#[test]
fn an_interrupted_install_is_collected() {
    let (_guard, root) = temp();
    let store = Store::new(root.join("store"));
    let roots = Roots::new(root.join("roots"));

    let pin = node("24.14.0");
    let staged = store.staging(&pin).unwrap();
    std::fs::write(staged.join("half"), "x").unwrap();

    let plan = gc::plan(&store, &roots).unwrap();
    assert_eq!(plan.unreachable, [format!(".staging-{}", pin.slug())]);

    gc::collect(&store, &roots, &plan).unwrap();
    assert!(!staged.exists());
}

/// A root that cannot be parsed stops the plan. It is holding an unknown set
/// of entries, and guessing is how a tool in use is deleted.
#[test]
fn an_unreadable_root_refuses_to_guess() {
    let (_guard, root) = temp();
    let store = Store::new(root.join("store"));
    let roots = Roots::new(root.join("roots"));
    std::fs::create_dir_all(roots.path()).unwrap();
    std::fs::write(roots.path().join("broken.json"), "{ not json").unwrap();

    let error = gc::plan(&store, &roots).unwrap_err();
    assert!(matches!(error, EnvError::Decode { .. }), "{error:?}");
}

#[test]
fn a_pin_names_itself_the_way_a_reader_would() {
    let pin = node("24.14.0");
    assert_eq!(pin.slug(), "node-24.14.0-darwin-arm64");
    assert_eq!(pin.to_string(), "node@24.14.0");
    assert_eq!(Tool::parse("pnpm"), Some(Tool::Pnpm));
    assert_eq!(Tool::parse("cargo"), None);
    assert!(Tool::Bun.is_runtime(), "bun runs code as well as installing it");
    assert!(!Tool::Pnpm.is_runtime());
}
