use camino::Utf8PathBuf;
use uf_config::{LibraryConfig, LibraryFormat, LibraryPlan, UniflowedConfig};

use super::{arguments, declared_dependencies, unresolved_exports};

/// A project directory holding `package.json` with `manifest` in it.
fn project(manifest: &str) -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    std::fs::write(root.join("package.json"), manifest).unwrap();
    (dir, root)
}

/// A library plan with `entries` and `formats` declared.
///
/// Built by assignment rather than by a struct expression: `LibraryConfig` is
/// `#[non_exhaustive]`, so only `uf_config` may write one out in full.
fn plan(entries: &[&str], formats: &[LibraryFormat]) -> LibraryPlan {
    let mut uf = UniflowedConfig::default();
    uf.app.router.enabled = false;
    uf.build.lib = Some(LibraryConfig::default());
    let lib = uf.build.lib.as_mut().expect("just set");
    lib.entries = entries.iter().map(|entry| (*entry).into()).collect();
    lib.formats = formats.to_vec();
    LibraryPlan::resolve(&uf).expect("a library")
}

#[test]
fn the_driver_is_told_the_entries_formats_and_externals() {
    let plan = plan(
        &["index.js", "internal/parse.js"],
        &[LibraryFormat::Es, LibraryFormat::Cjs],
    );
    let args = arguments("dist", &plan, &[String::from("react")]);
    assert_eq!(
        args,
        [
            "--out-dir",
            "dist",
            "--entry",
            "index.js",
            "--entry",
            "internal/parse.js",
            "--format",
            "es",
            "--format",
            "cjs",
            "--external",
            "react",
        ]
    );
}

/// Dependencies are external by default, which is the opposite of the
/// application build.
///
/// An application inlines what it can because it is the end of the line. A
/// library is not: a bundled copy of React inside it is a second React in
/// every application that installs it.
#[test]
fn every_declared_dependency_is_external_and_dev_dependencies_are_not() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "dependencies": { "zod": "^3" },
  "peerDependencies": { "react": ">=19" },
  "optionalDependencies": { "fsevents": "^2" },
  "devDependencies": { "@uniflowed/test": "0.0.0" }
}"#,
    );
    // Sorted, so two builds of one tree produce one command line.
    assert_eq!(declared_dependencies(&root), ["fsevents", "react", "zod"]);
}

/// The scaffold: a manifest with no dependency fields at all.
///
/// It externalises nothing beyond the host's built-ins, which is a correct
/// build of a project that imports only its own modules.
#[test]
fn a_manifest_with_no_dependencies_externalises_nothing() {
    let (_dir, root) = project(r#"{ "name": "lib", "type": "module" }"#);
    assert!(declared_dependencies(&root).is_empty());
}

#[test]
fn an_unreadable_manifest_is_not_a_failed_build() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    assert!(declared_dependencies(&root).is_empty());
    std::fs::write(root.join("package.json"), "{ not json").unwrap();
    assert!(declared_dependencies(&root).is_empty());
}

/// The shape a uf library publishes, with the build that backs it.
#[test]
fn an_exports_map_the_build_satisfies_reports_nothing() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": { ".": { "flow": "./index.js", "default": "./dist/index.js" } }
}"#,
    );
    std::fs::write(root.join("index.js"), "// @flow\n").unwrap();
    std::fs::create_dir_all(root.join("dist")).unwrap();
    std::fs::write(root.join("dist/index.js"), "export {};\n").unwrap();

    let found = unresolved_exports(&root, &root.join("dist"));
    assert!(found.is_empty(), "{found:?}");
}

/// A manifest naming a built file the build did not write.
///
/// The package installs, the build succeeds, and importing it fails — and
/// every check that existed before this one passes it.
#[test]
fn an_exports_target_under_the_output_directory_that_was_not_written_is_reported() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": {
    ".": { "flow": "./index.js", "default": "./dist/index.js" },
    "./parse": "./dist/internal/parse.js"
  }
}"#,
    );
    std::fs::create_dir_all(root.join("dist")).unwrap();
    std::fs::write(root.join("dist/index.js"), "export {};\n").unwrap();

    let found = unresolved_exports(&root, &root.join("dist"));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("./dist/internal/parse.js"), "{found:?}");
    // And not the source, which is in the checkout rather than in the build.
    assert!(!found[0].contains("./index.js"), "{found:?}");
}

/// The Flow source is never a finding, whether or not it exists.
///
/// It is outside the output directory, so it is not something this build
/// writes or is responsible for; reporting it would fire on every correctly
/// configured library.
#[test]
fn a_source_target_outside_the_output_directory_is_never_reported() {
    let (_dir, root) = project(
        r#"{ "name": "lib", "exports": { ".": { "flow": "./src/index.js", "default": "./dist/index.js" } } }"#,
    );
    std::fs::create_dir_all(root.join("dist")).unwrap();
    std::fs::write(root.join("dist/index.js"), "export {};\n").unwrap();

    assert!(unresolved_exports(&root, &root.join("dist")).is_empty());
}

#[test]
fn a_manifest_with_no_exports_reports_nothing() {
    let (_dir, root) = project(r#"{ "name": "lib", "main": "./dist/index.js" }"#);
    assert!(unresolved_exports(&root, &root.join("dist")).is_empty());
}
