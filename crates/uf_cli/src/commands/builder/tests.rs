use camino::Utf8Path;
use uf_config::UniflowedConfig;

use super::{resolve, uniflowed_package};

/// A project root with a builder installed under `node_modules/<name>`.
fn project_with(name: &str, manifest: &str, files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let package = root.join("node_modules").join(name);
    std::fs::create_dir_all(&package).unwrap();
    std::fs::write(package.join("package.json"), manifest).unwrap();
    for file in files {
        let path = package.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "// a driver\n").unwrap();
    }
    dir
}

fn naming(module: &str) -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.builder.module = module.into();
    config
}

#[test]
fn the_default_builder_is_vite() {
    assert_eq!(UniflowedConfig::default().builder.module, "@uniflowed/vite");
}

#[test]
fn a_declared_driver_is_the_one_that_runs() {
    let dir = project_with(
        "some-builder",
        r#"{"name":"some-builder","version":"2.1.0","uf":{"builder":{"driver":"./bin/uf.js"}}}"#,
        &["bin/uf.js"],
    );
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let builder = resolve(root, &naming("some-builder")).unwrap();

    assert_eq!(builder.module, "some-builder");
    assert_eq!(builder.version.as_deref(), Some("2.1.0"));
    assert!(builder.driver.ends_with("bin/uf.js"), "{}", builder.driver);
    // The name and the version, which is both halves of what
    // ubugeeei-prod/uf#549 asks `uf explain build` to print.
    assert_eq!(builder.label(), "some-builder 2.1.0");
}

#[test]
fn a_builder_that_declares_nothing_keeps_the_name_it_always_had() {
    // The compatibility path, and the reason it exists: uf's packages are
    // version-pinned to each other, but a project may be holding an older
    // `@uniflowed/vite` than the `uf` driving it, and refusing to start over a
    // manifest key added after it was published would break a working project
    // to enforce a declaration uf can infer.
    let dir = project_with(
        "@uniflowed/vite",
        r#"{"name":"@uniflowed/vite","version":"0.0.0-alpha.1"}"#,
        &["driver.js", "bun-preload.js"],
    );
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let builder = resolve(root, &UniflowedConfig::default()).unwrap();

    assert!(builder.driver.ends_with("driver.js"), "{}", builder.driver);
    assert!(builder.bun_preload.is_some());
}

#[test]
fn a_builder_with_no_bun_preload_is_started_without_one() {
    // Bun's `--preload` is how a builder that transforms Flow installs its
    // hooks. A builder that transforms nothing needs none, and uf passing a
    // path that does not exist would fail every Bun run of a perfectly good
    // builder.
    let dir = project_with(
        "plain-builder",
        r#"{"name":"plain-builder","version":"1.0.0","uf":{"builder":{}}}"#,
        &["driver.js"],
    );
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let builder = resolve(root, &naming("plain-builder")).unwrap();
    assert_eq!(builder.bun_preload, None);
}

#[test]
fn a_declared_preload_that_is_missing_is_an_error() {
    let dir = project_with(
        "broken-builder",
        r#"{"name":"broken-builder","uf":{"builder":{"preload":{"bun":"./hooks.js"}}}}"#,
        &["driver.js"],
    );
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let error = resolve(root, &naming("broken-builder"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("preload.bun"), "{error}");
}

#[test]
fn a_preload_that_is_a_package_is_handed_to_bun_untouched() {
    // `@uniflowed/vite` preloads `@uniflowed/host/bun-preload`: the Flow hooks
    // belong to the host package, not to the bundler. Resolving it against the
    // builder's own directory is what uf used to do — it passed
    // `<builder>/bun-preload.js`, a file `@uniflowed/vite` has never shipped —
    // so every Bun run of the driver preloaded something that was not there.
    let dir = project_with(
        "scoped-builder",
        r#"{"name":"scoped-builder","uf":{"builder":{"preload":{"bun":"@uniflowed/host/bun-preload"}}}}"#,
        &["driver.js"],
    );
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let builder = resolve(root, &naming("scoped-builder")).unwrap();
    assert_eq!(
        builder.bun_preload.as_deref(),
        Some("@uniflowed/host/bun-preload")
    );
}

#[test]
fn a_package_with_no_driver_is_not_a_builder() {
    let dir = project_with(
        "not-a-builder",
        r#"{"name":"not-a-builder"}"#,
        &["index.js"],
    );
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let error = resolve(root, &naming("not-a-builder"))
        .unwrap_err()
        .to_string();
    // "not a builder", not "not installed": the two have different fixes, and
    // sending a reader to `uf install` for a package that is right there is
    // the message being confidently wrong.
    assert!(error.contains("is not a builder"), "{error}");
    assert!(error.contains("uf.builder.driver"), "{error}");
}

#[test]
fn a_builder_that_is_not_installed_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let error = resolve(root, &UniflowedConfig::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("@uniflowed/vite"), "{error}");
    assert!(error.contains("uf install"), "{error}");
}

#[test]
fn a_builder_is_found_up_the_tree() {
    let dir = project_with(
        "@uniflowed/vite",
        r#"{"name":"@uniflowed/vite","version":"0.0.0"}"#,
        &["driver.js"],
    );
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let nested = root.join("apps/docs");
    std::fs::create_dir_all(&nested).unwrap();
    let builder = resolve(&nested, &UniflowedConfig::default()).unwrap();
    assert_eq!(builder.directory, root.join("node_modules/@uniflowed/vite"));
}

#[test]
fn a_builder_inside_the_project_is_named_by_path() {
    // How somebody tries a builder before publishing it, which is the case
    // that makes the seam usable rather than merely present.
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let local = root.join("tools/my-builder");
    std::fs::create_dir_all(&local).unwrap();
    std::fs::write(local.join("driver.js"), "// a driver\n").unwrap();

    let builder = resolve(root, &naming("./tools/my-builder")).unwrap();
    assert_eq!(builder.directory, local);
    // No manifest, so no version — and `label()` has to survive that rather
    // than print "undefined".
    assert_eq!(builder.label(), "./tools/my-builder");
}

#[test]
fn a_path_that_climbs_out_of_the_project_is_refused() {
    // `uf.config.js` is untrusted input — a cloned repository's, most of the
    // time — and "run this file as the toolchain" is the most dangerous
    // sentence it can contain.
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let error = resolve(root, &naming("../elsewhere"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("outside"), "{error}");
}

#[test]
fn an_empty_module_is_refused_rather_than_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    let error = resolve(root, &naming("")).unwrap_err().to_string();
    assert!(error.contains("empty"), "{error}");
}

#[test]
fn a_uniflowed_package_still_needs_its_marker_file() {
    // `uniflowed_package` is the other walk, and the marker is what
    // distinguishes an installed package from a workspace link that has not
    // been built.
    let dir = project_with(
        "@uniflowed/host",
        r#"{"name":"@uniflowed/host"}"#,
        &["other.js"],
    );
    let root = Utf8Path::from_path(dir.path()).unwrap();
    assert!(uniflowed_package(root, "host", "register.js").is_err());
}
