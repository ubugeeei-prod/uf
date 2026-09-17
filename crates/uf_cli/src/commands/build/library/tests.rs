use camino::Utf8PathBuf;
use uf_config::{LibraryConfig, LibraryFormat, LibraryPlan, UniflowedConfig};

use uf_declare::{Construct, Gap};

use super::{
    Declarations, arguments, declared_dependencies, gap_summary, missing_types_condition,
    unpublished_exports, unresolved_exports,
};

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

#[test]
fn exports_targets_covered_by_files_report_nothing() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": { ".": { "flow": "./index.js", "default": "./dist/index.js" } },
  "files": ["index.js", "dist", "!*.test.js"]
}"#,
    );

    assert!(unpublished_exports(&root).is_empty());
}

#[test]
fn an_exports_target_not_covered_by_files_is_reported() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": { ".": { "flow": "./index.js", "default": "./dist/index.js" } },
  "files": ["index.js", "!*.test.js"]
}"#,
    );

    let found = unpublished_exports(&root);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("./dist/index.js"), "{found:?}");
    assert!(!found[0].contains("./index.js"), "{found:?}");
}

#[test]
fn files_are_applied_in_order_so_negations_can_subtract_exports() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": {
    ".": "./index.js",
    "./fixture": "./internal/fixture.test.js"
  },
  "files": ["index.js", "internal", "!*.test.js"]
}"#,
    );

    let found = unpublished_exports(&root);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("./internal/fixture.test.js"), "{found:?}");
    assert!(!found[0].contains("./index.js"), "{found:?}");
}

#[test]
fn files_globs_cover_manifest_exports() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": {
    ".": "./index.js",
    "./native": "./internal/native-runtime.js"
  },
  "files": ["*.js", "internal/*.js", "!*.test.js"]
}"#,
    );

    assert!(unpublished_exports(&root).is_empty());
}

#[test]
fn files_globs_are_root_relative_and_directory_matches_include_children() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": {
    ".": "./index.js",
    "./private": "./internal/private.js",
    "./deep": "./dist/sub/index.js"
  },
  "files": ["*.js", "dist/*", "!*.test.js"]
}"#,
    );

    let found = unpublished_exports(&root);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("./internal/private.js"), "{found:?}");
    assert!(!found[0].contains("./index.js"), "{found:?}");
    assert!(!found[0].contains("./dist/sub/index.js"), "{found:?}");
}

#[test]
fn package_json_exports_are_always_publishable() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": {
    ".": "./index.js",
    "./package.json": "./package.json"
  },
  "files": ["index.js", "!*.test.js"]
}"#,
    );

    assert!(unpublished_exports(&root).is_empty());
}

#[test]
fn npm_always_published_manifest_targets_are_not_reported() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "main": "./dist/index.js",
  "bin": {
    "lib": "./dist/cli.js"
  },
  "exports": {
    ".": "./dist/index.js",
    "./cli": "./dist/cli.js"
  },
  "files": ["index.js", "!*.test.js"]
}"#,
    );

    assert!(unpublished_exports(&root).is_empty());
}

#[test]
fn root_metadata_files_are_always_publishable_but_nested_ones_are_not() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": {
    ".": "./index.js",
    "./readme": "./README.md",
    "./license": "./LICENSE.txt",
    "./nested-license": "./docs/LICENSE.txt"
  },
  "files": ["index.js", "!*.test.js"]
}"#,
    );

    let found = unpublished_exports(&root);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("./docs/LICENSE.txt"), "{found:?}");
    assert!(!found[0].contains("./README.md"), "{found:?}");
    assert!(!found[0].contains("./LICENSE.txt"), "{found:?}");
}

#[test]
fn a_manifest_with_no_files_has_no_files_allowlist_warning() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": { ".": { "flow": "./index.js", "default": "./dist/index.js" } }
}"#,
    );

    assert!(unpublished_exports(&root).is_empty());
}

/// Declarations written into a package whose `exports` never names them.
///
/// The quietest way to fail ubugeeei-prod/uf#969: the types are translated,
/// written and published, the build succeeds, the package installs — and every
/// TypeScript consumer is still on `any`, because TypeScript looks for a
/// `"types"` condition and there is none. Nothing else in the build has a
/// reason to notice, which is the whole argument for this check.
#[test]
fn exports_without_a_types_condition_is_reported() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": { ".": { "flow": "./index.js", "default": "./dist/index.js" } }
}"#,
    );
    let declarations = Declarations {
        enabled: true,
        files: vec![root.join("dist/index.d.ts")],
        gaps: Vec::new(),
    };

    let found = missing_types_condition(&root, &declarations);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("types"), "{found:?}");
}

/// The shape the scaffold writes, which is the one that works.
#[test]
fn exports_that_names_the_declarations_reports_nothing() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": {
    ".": {
      "flow": "./index.js",
      "types": "./dist/index.d.ts",
      "default": "./dist/index.js"
    }
  }
}"#,
    );
    let declarations = Declarations {
        enabled: true,
        files: vec![root.join("dist/index.d.ts")],
        gaps: Vec::new(),
    };

    assert!(missing_types_condition(&root, &declarations).is_empty());
}

/// A build that wrote no declarations has nothing to say about finding them.
#[test]
fn a_build_without_declarations_says_nothing_about_a_types_condition() {
    let (_dir, root) = project(
        r#"{
  "name": "lib",
  "exports": { ".": { "flow": "./index.js", "default": "./dist/index.js" } }
}"#,
    );

    assert!(missing_types_condition(&root, &Declarations::default()).is_empty());
}

/// The headline over the gap list counts two different kinds of news.
///
/// An exact object type still publishes every member it has; a `$Diff<A, B>`
/// publishes `unknown`. Reporting one number for both would tell an author
/// that twelve things went wrong without telling them that eleven of those
/// twelve still type-check their consumers.
#[test]
fn the_gap_summary_separates_a_widening_from_a_refusal() {
    let declarations = Declarations {
        enabled: true,
        files: Vec::new(),
        gaps: vec![
            (
                String::from("index.js"),
                Gap {
                    declaration: "Options".into(),
                    construct: Construct::ExactObject,
                    reason: "widened".into(),
                    line: 3,
                },
            ),
            (
                String::from("index.js"),
                Gap {
                    declaration: "Diffed".into(),
                    construct: Construct::FlowUtility,
                    reason: "refused".into(),
                    line: 9,
                },
            ),
        ],
    };

    let summary = gap_summary(&declarations);
    assert_eq!(summary.len(), 1, "{summary:?}");
    assert!(summary[0].contains("2 declaration"), "{summary:?}");
    assert!(summary[0].contains("1 of them"), "{summary:?}");
}

#[test]
fn a_build_with_no_gaps_prints_no_headline() {
    assert!(gap_summary(&Declarations::default()).is_empty());
}
