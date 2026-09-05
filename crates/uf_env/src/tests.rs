use camino::Utf8PathBuf;

use super::*;
use crate::project;

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
    assert!(
        Tool::Bun.is_runtime(),
        "bun runs code as well as installing it"
    );
    assert!(!Tool::Pnpm.is_runtime());
}

/// A repository's links are rebuilt to exactly what it declares, so a tool
/// that is dropped stops being on the project's path.
#[test]
fn linking_replaces_what_the_repository_had() {
    let (_guard, root) = temp();
    let store = Store::new(root.join("store"));
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();

    let install = |pin: &Pin, executables: &[&str]| {
        let staged = store.staging(pin).unwrap();
        std::fs::create_dir_all(staged.join("bin")).unwrap();
        for name in executables {
            std::fs::write(staged.join("bin").join(name), "#!/bin/sh\n").unwrap();
        }
        store.adopt(pin, &staged).unwrap();
    };

    let old = node("22.9.0");
    let new = node("24.14.0");
    install(&old, &["node", "npx"]);
    install(&new, &["node", "npx"]);

    project::link(&project, &store, std::slice::from_ref(&old)).unwrap();
    let bin = project::bin_dir(&project);
    assert!(bin.join("node").exists());

    project::link(&project, &store, std::slice::from_ref(&new)).unwrap();
    assert_eq!(
        std::fs::read_link(bin.join("node").as_std_path())
            .unwrap()
            .to_string_lossy(),
        store.path(&new).join("bin/node").as_str(),
        "the link points at the version the project now declares"
    );
}

/// Linking a pin that is not installed says so rather than leaving a link to
/// nothing on the project's path.
#[test]
fn linking_something_that_is_not_installed_refuses() {
    let (_guard, root) = temp();
    let store = Store::new(root.join("store"));
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();

    let error = project::link(&project, &store, &[node("24.14.0")]).unwrap_err();
    assert!(matches!(error, EnvError::NotInstalled { .. }), "{error:?}");
}

/// A range is not a pin. An environment that can resolve differently
/// tomorrow does not answer "what is this built with".
#[test]
fn a_version_that_is_not_exact_is_refused() {
    use uf_config::UniflowedConfig;

    let platform = Platform {
        os: Os::Darwin,
        arch: Arch::Arm64,
    };
    let mut config = UniflowedConfig::default();
    config
        .env
        .toolchain
        .insert("node".into(), "^24.14.0".into());
    let error = project::declared(&config, platform).unwrap_err();
    assert!(
        matches!(error, EnvError::NotAnExactVersion { .. }),
        "{error:?}"
    );

    config.env.toolchain.clear();
    config.env.toolchain.insert("cargo".into(), "1.0.0".into());
    let error = project::declared(&config, platform).unwrap_err();
    assert!(matches!(error, EnvError::UnknownTool { .. }), "{error:?}");

    config.env.toolchain.clear();
    config.env.toolchain.insert("node".into(), "24.14.0".into());
    config.env.toolchain.insert("pnpm".into(), "9.15.0".into());
    let pins = project::declared(&config, platform).unwrap();
    assert_eq!(pins.len(), 2);
    assert_eq!(pins[0].tool, Tool::Node, "runtimes are listed first");
}

/// Each publisher's URL is built the way that publisher names its files.
#[test]
fn a_source_is_where_its_publisher_puts_it() {
    use crate::source::{Checksum, Format, Source};

    let pin = node("24.14.0");
    let source = Source::for_pin(&pin).unwrap();
    assert_eq!(
        source.archive,
        "https://nodejs.org/dist/v24.14.0/node-v24.14.0-darwin-arm64.tar.gz"
    );
    assert!(matches!(source.format, Format::TarGz));
    assert_eq!(
        source.strip, 1,
        "the tarball wraps everything in one directory"
    );
    match source.checksum {
        Checksum::Sha256File { url, file } => {
            assert_eq!(url, "https://nodejs.org/dist/v24.14.0/SHASUMS256.txt");
            assert_eq!(file, "node-v24.14.0-darwin-arm64.tar.gz");
        }
        other => panic!("node publishes a SHASUMS file: {other:?}"),
    }

    let pnpm = Pin {
        tool: Tool::Pnpm,
        version: "9.15.0".to_owned(),
        platform: pin.platform,
    };
    let source = Source::for_pin(&pnpm).unwrap();
    assert_eq!(
        source.archive, "https://registry.npmjs.org/pnpm/-/pnpm-9.15.0.tgz",
        "a package manager is an npm package"
    );
}
