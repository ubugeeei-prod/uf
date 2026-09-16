use std::fs;

use camino::Utf8PathBuf;

use super::*;

/// A project at `<tmp>/app` with an empty `node_modules`, and the temporary
/// directory it is in — canonical, because where a link leads is compared with
/// the path the filesystem itself reports, and on macOS the temporary directory
/// is reached through a link of its own.
fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().unwrap()).unwrap();
    fs::create_dir_all(root.join("app/node_modules")).unwrap();
    (dir, root)
}

/// A symbolic link at `from`, saying `to`.
#[cfg(unix)]
fn symlink(from: &Utf8Path, to: &str) {
    fs::create_dir_all(from.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(to, from).unwrap();
}

#[test]
fn only_a_name_that_stays_inside_node_modules_is_looked_at() {
    let (_dir, root) = project();
    let app = root.join("app");
    for name in [
        "",
        ".",
        "..",
        "../app",
        "a/b",
        "@acme",
        "@/ui",
        "@acme/",
        "@acme/..",
        "@acme/ui/x",
        "a\\b",
        "C:ui",
        "a\0b",
    ] {
        assert!(!is_package_name(name), "{name:?}");
        assert_eq!(link_state(&app, name), None, "{name:?}");
    }
    for name in ["left-pad", "@acme/ui", "lodash.merge", "_private"] {
        assert!(is_package_name(name), "{name:?}");
    }
}

#[test]
fn nothing_there_is_absent_and_a_directory_is_an_install() {
    let (_dir, root) = project();
    let app = root.join("app");
    assert_eq!(link_state(&app, "left-pad"), Some(LinkState::Absent));

    fs::create_dir_all(app.join("node_modules/left-pad")).unwrap();
    assert_eq!(link_state(&app, "left-pad"), Some(LinkState::Installed));
}

/// npm and bun: a link straight to the checkout, here under a scope.
#[cfg(unix)]
#[test]
fn a_link_out_of_node_modules_is_linked_and_says_where_it_leads() {
    let (_dir, root) = project();
    let app = root.join("app");
    fs::create_dir_all(root.join("ui")).unwrap();
    symlink(&app.join("node_modules/@acme/ui"), "../../../ui");

    assert_eq!(
        link_state(&app, "@acme/ui"),
        Some(LinkState::Linked(root.join("ui")))
    );
}

/// Yarn 1: a link to its own registry entry, which is a link to the checkout.
#[cfg(unix)]
#[test]
fn a_link_through_a_registry_is_followed_to_the_checkout() {
    let (_dir, root) = project();
    let app = root.join("app");
    fs::create_dir_all(root.join("ui")).unwrap();
    symlink(&root.join("registry/link/ui"), "../../ui");
    symlink(&app.join("node_modules/ui"), "../../registry/link/ui");

    assert_eq!(
        link_state(&app, "ui"),
        Some(LinkState::Linked(root.join("ui")))
    );
}

/// pnpm installs every package as a link into `.pnpm`, and that is not a link
/// anybody asked for.
#[cfg(unix)]
#[test]
fn a_link_into_the_projects_own_node_modules_is_an_install() {
    let (_dir, root) = project();
    let app = root.join("app");
    fs::create_dir_all(app.join("node_modules/.pnpm/tiny@1.0.0/node_modules/tiny")).unwrap();
    symlink(
        &app.join("node_modules/tiny"),
        ".pnpm/tiny@1.0.0/node_modules/tiny",
    );

    assert_eq!(link_state(&app, "tiny"), Some(LinkState::Installed));
}

#[cfg(unix)]
#[test]
fn a_link_that_leads_nowhere_is_broken_and_says_what_it_says() {
    let (_dir, root) = project();
    let app = root.join("app");
    symlink(&app.join("node_modules/ui"), "../../gone");

    assert_eq!(
        link_state(&app, "ui"),
        Some(LinkState::Broken(Utf8PathBuf::from("../../gone")))
    );
}

#[test]
fn a_package_name_comes_from_its_manifest_and_only_a_package_name_does() {
    let (_dir, root) = project();
    let ui = root.join("ui");
    fs::create_dir_all(&ui).unwrap();
    assert_eq!(package_name(&ui), None, "no manifest");

    for (manifest, expected) in [
        (
            r#"{ "name": "@acme/ui", "version": "1.0.0" }"#,
            Some("@acme/ui"),
        ),
        (r#"{ "version": "1.0.0" }"#, None),
        (r#"{ "name": 7 }"#, None),
        (r#"{ "name": "../../etc" }"#, None),
        ("not json", None),
    ] {
        fs::write(ui.join("package.json"), manifest).unwrap();
        assert_eq!(package_name(&ui).as_deref(), expected, "{manifest}");
    }
}

#[cfg(unix)]
#[test]
fn a_manifest_that_is_itself_a_link_is_not_followed() {
    let (_dir, root) = project();
    fs::create_dir_all(root.join("ui")).unwrap();
    fs::write(root.join("elsewhere.json"), r#"{ "name": "ui" }"#).unwrap();
    symlink(&root.join("ui/package.json"), "../elsewhere.json");

    assert_eq!(package_name(&root.join("ui")), None);
}

#[test]
fn a_yarn_link_is_read_from_resolutions_and_nothing_else_is() {
    let (_dir, root) = project();
    let app = root.join("app");
    fs::write(
        app.join("package.json"),
        r#"{
          "dependencies": { "ui": "link:../ui", "tiny": "link:../tiny" },
          "resolutions": {
            "ui": "portal:/work/ui",
            "lib": "link:../lib",
            "react": "18.3.1",
            "__proto__": "portal:/work/proto"
          }
        }"#,
    )
    .unwrap();

    assert_eq!(
        linked_resolution(&app, "ui").as_deref(),
        Some("portal:/work/ui")
    );
    assert_eq!(
        linked_resolution(&app, "lib").as_deref(),
        Some("link:../lib")
    );
    assert_eq!(
        linked_resolution(&app, "react"),
        None,
        "a version is no link"
    );
    assert_eq!(
        linked_resolution(&app, "tiny"),
        None,
        "a dependency is no resolution"
    );
    assert_eq!(linked_resolution(&app, "__proto__"), None);
    assert_eq!(linked_resolution(&app, "missing"), None);
}

/// A lookup that answers from a fixed list of variables, and nothing else.
fn variables(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    }
}

#[test]
fn each_managers_global_directory_follows_its_own_variables() {
    let dirs = GlobalDirs::from_env(&variables(&[
        ("HOME", "/home/me"),
        ("PNPM_HOME", "/opt/pnpm"),
        ("XDG_DATA_HOME", "/data"),
        ("BUN_INSTALL", "/opt/bun"),
    ]));
    assert_eq!(dirs.npm, None, "npm is asked, not guessed");
    assert_eq!(dirs.pnpm.as_deref(), Some(Utf8Path::new("/opt/pnpm")));
    assert_eq!(
        dirs.bun.as_deref(),
        Some(Utf8Path::new("/opt/bun/install/global"))
    );
    if !cfg!(windows) {
        assert_eq!(dirs.yarn.as_deref(), Some(Utf8Path::new("/data/yarn/link")));
    }

    let dirs = GlobalDirs::from_env(&variables(&[
        ("HOME", "/home/me"),
        ("BUN_INSTALL", "/opt/bun"),
        ("BUN_INSTALL_GLOBAL_DIR", "/opt/bun-global"),
    ]));
    assert_eq!(dirs.bun.as_deref(), Some(Utf8Path::new("/opt/bun-global")));
    if cfg!(target_os = "macos") {
        assert_eq!(
            dirs.pnpm.as_deref(),
            Some(Utf8Path::new("/home/me/Library/pnpm"))
        );
    } else if !cfg!(windows) {
        assert_eq!(
            dirs.pnpm.as_deref(),
            Some(Utf8Path::new("/home/me/.local/share/pnpm"))
        );
    }
    if !cfg!(windows) {
        assert_eq!(
            dirs.yarn.as_deref(),
            Some(Utf8Path::new("/home/me/.config/yarn/link"))
        );
    }

    assert_eq!(GlobalDirs::from_env(&|_| None), GlobalDirs::default());
}

#[test]
fn a_registry_entry_is_where_each_manager_keeps_a_registered_package() {
    let (_dir, root) = project();
    fs::create_dir_all(root.join("pnpm/global/5/node_modules")).unwrap();
    fs::create_dir_all(root.join("pnpm/global/v11")).unwrap();
    let dirs = GlobalDirs {
        npm: Some(root.join("npm/lib/node_modules")),
        pnpm: Some(root.join("pnpm")),
        yarn: Some(root.join("yarn/link")),
        bun: Some(root.join("bun/install/global")),
    };

    assert_eq!(
        registry_entries(PackageManager::Npm, "@acme/ui", &dirs),
        [root.join("npm/lib/node_modules/@acme/ui")]
    );
    assert_eq!(
        registry_entries(PackageManager::Pnpm, "@acme/ui", &dirs),
        [
            root.join("pnpm/global/5/node_modules/@acme/ui"),
            root.join("pnpm/global/v11/node_modules/@acme/ui"),
        ]
    );
    assert_eq!(
        registry_entries(
            PackageManager::Yarn(YarnEdition::Classic),
            "@acme/ui",
            &dirs
        ),
        [root.join("yarn/link/@acme/ui")]
    );
    assert_eq!(
        registry_entries(PackageManager::Bun, "@acme/ui", &dirs),
        [root.join("bun/install/global/node_modules/@acme/ui")]
    );
    assert!(
        registry_entries(PackageManager::Yarn(YarnEdition::Berry), "@acme/ui", &dirs).is_empty(),
        "Yarn 2+ keeps no registry"
    );
    assert!(registry_entries(PackageManager::Npm, "../../etc", &dirs).is_empty());
}

#[cfg(unix)]
#[test]
fn a_registration_is_a_link_to_this_package_and_anything_else_says_what_is_there() {
    let (_dir, root) = project();
    let ui = root.join("ui");
    let other = root.join("other");
    fs::create_dir_all(&ui).unwrap();
    fs::create_dir_all(&other).unwrap();
    let entry = root.join("registry/ui");
    let only = |entry: &Utf8PathBuf| registration(std::slice::from_ref(entry), &ui);

    assert_eq!(only(&entry), Registration::Absent);

    symlink(&entry, "../other");
    assert_eq!(
        only(&entry),
        Registration::Elsewhere {
            entry: entry.clone(),
            to: other,
        }
    );

    fs::remove_file(&entry).unwrap();
    symlink(&entry, "../ui");
    assert_eq!(only(&entry), Registration::Linked(entry.clone()));

    // A link to this package in any layout is the registration, whatever an
    // older layout holds under the same name.
    let installed = root.join("old-registry/ui");
    fs::create_dir_all(&installed).unwrap();
    assert_eq!(
        registration(&[installed.clone(), entry.clone()], &ui),
        Registration::Linked(entry.clone())
    );
    assert_eq!(only(&installed), Registration::Installed(installed.clone()));

    fs::remove_file(&entry).unwrap();
    symlink(&entry, "../gone");
    assert_eq!(
        only(&entry),
        Registration::Elsewhere {
            entry: entry.clone(),
            to: Utf8PathBuf::from("../gone"),
        }
    );
}

#[cfg(unix)]
#[test]
fn only_a_link_is_removed_and_never_what_it_leads_to() {
    let (_dir, root) = project();
    let app = root.join("app");
    fs::create_dir_all(root.join("ui")).unwrap();
    fs::write(root.join("ui/package.json"), "{}").unwrap();
    symlink(&app.join("node_modules/@acme/ui"), "../../../ui");

    remove_link(&app, "@acme/ui").unwrap();
    assert!(fs::symlink_metadata(app.join("node_modules/@acme/ui")).is_err());
    assert!(
        root.join("ui/package.json").is_file(),
        "what the link led to is untouched"
    );

    fs::create_dir_all(app.join("node_modules/left-pad")).unwrap();
    assert!(
        remove_link(&app, "left-pad").is_err(),
        "a directory is no link"
    );
    assert!(app.join("node_modules/left-pad").is_dir());
    assert!(remove_link(&app, "../ui").is_err());
    assert!(root.join("ui").is_dir());
}

#[test]
fn a_link_override_is_taken_out_only_where_pnpm_wrote_it_the_way_pnpm_writes() {
    let (_dir, root) = project();
    let app = root.join("app");
    let file = app.join("pnpm-workspace.yaml");

    assert_eq!(remove_link_override(&app, "scratch-lib").unwrap(), None);

    fs::write(&file, "overrides:\n  scratch-lib: link:../lib\n").unwrap();
    assert_eq!(
        remove_link_override(&app, "scratch-lib")
            .unwrap()
            .as_deref(),
        Some("link:../lib")
    );
    assert!(
        !file.exists(),
        "pnpm link created the file, and nothing else was in it"
    );

    fs::write(
        &file,
        "packages:\n  - app\noverrides:\n  '@acme/ui': link:../ui\n  react: 18.3.1\n",
    )
    .unwrap();
    assert_eq!(
        remove_link_override(&app, "@acme/ui").unwrap().as_deref(),
        Some("link:../ui")
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "packages:\n  - app\noverrides:\n  react: 18.3.1\n"
    );

    fs::write(
        &file,
        "overrides:\n  scratch-lib: link:../lib\ncatalog:\n  react: ^18.3.1\n",
    )
    .unwrap();
    assert!(remove_link_override(&app, "scratch-lib").unwrap().is_some());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "catalog:\n  react: ^18.3.1\n"
    );

    for (untouched, name) in [
        ("overrides:\n  react: 18.3.1\n", "react"),
        ("overrides: { scratch-lib: 'link:../lib' }\n", "scratch-lib"),
        (
            "overrides:\n  scratch-lib: link:../lib # mine\n",
            "scratch-lib",
        ),
        (
            "overrides:\n  scratch-lib: link:../lib\n  scratch-lib: link:../other\n",
            "scratch-lib",
        ),
        (
            "overrides:\n  scratch-lib:\n    nested: link:../lib\n",
            "scratch-lib",
        ),
    ] {
        fs::write(&file, untouched).unwrap();
        assert_eq!(
            remove_link_override(&app, name).unwrap(),
            None,
            "{untouched}"
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), untouched);
    }
}

/// npm masks a UUID wherever it prints one, `npm root --global` included, so
/// the directory it names is looked up before uf reads it.
#[test]
fn a_masked_answer_from_npm_is_put_back_from_the_filesystem() {
    let (_dir, root) = project();
    let registry = root.join("8c1d5f02-4a6b-4c3e-9f70-2b8e1a4d6c59/npm-global/lib/node_modules");
    fs::create_dir_all(&registry).unwrap();

    assert_eq!(
        unmasked(&root.join("***/npm-global/lib/node_modules")),
        Some(registry.clone()),
        "the masked segment names the one directory the rest of the path fits"
    );
    assert_eq!(
        unmasked(&registry),
        Some(registry),
        "an answer with nothing masked in it is the answer"
    );
    let never = root.join("npm-global/lib/node_modules");
    assert_eq!(
        unmasked(&never),
        Some(never),
        "a global directory npm has never had to create is still the answer"
    );
}

/// A mask uf cannot put back is no answer at all: `uf unlink` removes what it
/// finds in this directory, so a guess is worse than saying it cannot find it.
#[test]
fn a_mask_that_two_directories_fit_is_no_answer_and_neither_is_one_nothing_fits() {
    let (_dir, root) = project();
    for uuid in [
        "8c1d5f02-4a6b-4c3e-9f70-2b8e1a4d6c59",
        "b71e0a93-2d4c-4e8f-8a15-6c9d3f0b2e47",
    ] {
        fs::create_dir_all(root.join(uuid).join("npm-global/lib/node_modules")).unwrap();
    }

    assert_eq!(
        unmasked(&root.join("***/npm-global/lib/node_modules")),
        None
    );
    assert_eq!(unmasked(&root.join("***/nowhere/node_modules")), None);
}
