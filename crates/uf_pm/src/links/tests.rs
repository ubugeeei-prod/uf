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
