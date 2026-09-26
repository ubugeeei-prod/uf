use super::*;
use serde_json::json;

#[test]
fn previous_config_migrates_without_losing_comments_or_unrelated_formatting() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    let before = r#"// 日本語 and the old release's formatting stay.
export default {
  env: { /* tool pins */ toolchain: { node: '24.14.0', npm: '11.9.0' } },
  builder: { module: '@uniflowed/vite' },
  pm: { packageManager: 'npm' },
  test: { runner: { /* target override */ applicationTarget: 'react-native' } },
  tasks: { untouched: 'echo keep  two spaces' },
};
"#;
    fs::write(root.join("uf.config.js"), before).unwrap();
    let plan = codemod::plan(root, Some("0.0.0-alpha.32"), "0.0.0-alpha.41").unwrap();
    assert!(plan.unmapped.is_empty(), "{:?}", plan.unmapped);
    let after = plan.changes[0].after.as_ref().unwrap();
    for text in [
        "// 日本語",
        "/* tool pins */",
        "/* target override */",
        "tasks: { untouched: 'echo keep  two spaces' }",
    ] {
        assert!(after.contains(text), "{after}");
    }
    assert_eq!(
        source::get(after, &["runtime"]).unwrap(),
        Some(json!("node@24.14.0"))
    );
    assert_eq!(
        source::get(after, &["packageManager"]).unwrap(),
        Some(json!("npm@11.9.0"))
    );
    assert_eq!(
        source::get(after, &["build", "builder"]).unwrap(),
        Some(json!("vite"))
    );
    assert_eq!(
        source::get(after, &["test", "target"]).unwrap(),
        Some(json!("react-native"))
    );
    assert_eq!(
        source::get(after, &["test", "runner"]).unwrap(),
        Some(json!("uf"))
    );
    plan.apply(root).unwrap();
    let again = codemod::plan(root, Some("0.0.0-alpha.32"), "0.0.0-alpha.41").unwrap();
    assert!(again.changes.is_empty());
    assert!(root.join(".uf/migration-report.json").exists());
}

#[test]
fn releases_order_the_alpha_series_below_0_x_0() {
    let release = |version| codemod::Release::parse(version).unwrap();
    assert!(release("0.0.0-alpha.9") < release("0.0.0-alpha.10"));
    assert!(release("0.0.0-alpha.48") < release("0.1.0"));
    assert!(release("0.1.0-rc.1") < release("0.1.0"));
    assert!(release("0.1.0-alpha.3") < release("0.1.0-beta.1"));
    assert!(release("0.1.0") < release("0.1.1"));
    assert!(release("0.1.1") < release("0.2.0"));
    assert!(release("0.9.0") < release("0.10.0"));
    assert!(release("0.10.0") < release("1.0.0"));
    for text in [
        "",
        "0.1",
        "0.1.0.0",
        "v0.1.0",
        "0.01.0",
        "0.1.0-alpha",
        "0.1.0-nightly.1",
        "uf@0.1.0",
    ] {
        assert!(
            codemod::Release::parse(text).is_err(),
            "{text:?} was accepted"
        );
    }
    // This binary's own version is the default `--to`, so it has to be one
    // the catalog reads — `0.0.0-alpha.N` before 0.1.0, `0.x.0` after.
    codemod::Release::parse(env!("CARGO_PKG_VERSION")).unwrap();
}

#[test]
fn a_project_on_the_alpha_series_migrates_to_this_binarys_version() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::write(
        root.join("uf.config.js"),
        "export default { env: { toolchain: { node: '24.14.0' } } };\n",
    )
    .unwrap();
    let plan = codemod::plan(root, Some("0.0.0-alpha.32"), env!("CARGO_PKG_VERSION")).unwrap();
    assert!(
        plan.migrations
            .iter()
            .any(|m| m == codemod::TOOL_DECLARATIONS)
    );
    let after = plan.changes[0].after.as_ref().unwrap();
    assert_eq!(
        source::get(after, &["runtime"]).unwrap(),
        Some(json!("node@24.14.0"))
    );
    // Past the release that introduced it, that migration does not run again
    // and the config is left alone. Later releases may still plan migrations
    // of their own (0.3.0 plans #1453's two), so this asserts only about this one.
    let current = codemod::plan(root, Some("0.0.0-alpha.41"), env!("CARGO_PKG_VERSION")).unwrap();
    assert!(
        !current
            .migrations
            .iter()
            .any(|m| m == codemod::TOOL_DECLARATIONS),
        "{:?}",
        current.migrations
    );
    assert!(
        current
            .changes
            .iter()
            .all(|change| change.path != "uf.config.js"),
        "the config was rewritten again"
    );
}

#[test]
fn conflicts_and_ambiguous_roles_keep_original_values() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    let before = "export default { env: { toolchain: { node: '24', bun: '1' } }, builder: { module: 'custom' }, build: { builder: 'vite' } };";
    fs::write(root.join("uf.config.js"), before).unwrap();
    let plan = codemod::plan(root, Some("0.0.0-alpha.40"), "0.0.0-alpha.41").unwrap();
    assert!(plan.changes.is_empty());
    assert_eq!(plan.unmapped.len(), 2);
}

#[test]
fn parser_refuses_spreads_duplicates_and_incomplete_sources() {
    for before in [
        "export default { ...other, a: 1 };",
        "export default { a: 1, a: 2 };",
        "export default {",
    ] {
        let mut candidate = before.to_owned();
        assert!(source::put(&mut candidate, &["a"], &json!(3)).is_err());
        assert_eq!(candidate, before);
    }
}

#[test]
fn adoption_preserves_unknown_settings_and_source_imports() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{"name":"app","scripts":{"test":"jest","lint":"eslint .","custom":"echo retained"} }"#,
    )
    .unwrap();
    fs::write(
        root.join(".prettierrc"),
        r#"{"singleQuote":true,"tabWidth":4}"#,
    )
    .unwrap();
    fs::write(
        root.join(".eslintrc.json"),
        r#"{"rules":{"custom/unknown":"error"}}"#,
    )
    .unwrap();
    fs::write(root.join("vite.config.js"), "import react from '@vitejs/plugin-react'; export default { base: '/app/', plugins: [react()] };").unwrap();
    fs::write(root.join("value.test.js"), "// @flow\nimport { value } from './value.js';\ntest('value', () => expect(value).toBe(42));\n").unwrap();
    let plan = adopt::plan(root).unwrap();
    assert!(
        plan.unmapped
            .iter()
            .any(|v| v.contains("rules.custom/unknown"))
    );
    assert!(
        plan.unmapped
            .iter()
            .any(|v| v.contains("vite.config.js#plugins"))
    );
    assert!(root.join(".prettierrc").exists()); // Planning is read-only.
    plan.apply(root).unwrap();
    assert!(!root.join(".prettierrc").exists());
    assert!(root.join(".eslintrc.json").exists());
    assert!(root.join("vite.config.js").exists());
    let config = fs::read_to_string(root.join("uf.config.js")).unwrap();
    assert_eq!(
        source::get(&config, &["tasks", "test"]).unwrap(),
        Some(json!("uf test"))
    );
    assert_eq!(
        source::get(&config, &["fmt", "quotes"]).unwrap(),
        Some(json!("single"))
    );
    assert_eq!(
        source::get(&config, &["vite", "base"]).unwrap(),
        Some(json!("/app/"))
    );
    let test = fs::read_to_string(root.join("value.test.js")).unwrap();
    assert!(test.starts_with("// @flow\nimport { test, expect } from \"@uniflowed/test\";\n"));
    assert!(test.contains("import { value } from './value.js';"));
}

#[test]
fn adoption_changes_only_jest_import_sources() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::write(root.join("package.json"), r#"{"name":"app"}"#).unwrap();
    fs::write(root.join("import.test.js"), "// 日本語 @jest/globals\nimport { test, expect } from '@jest/globals';\ntest('data', () => expect('@jest/globals').toBe('@jest/globals'));\n").unwrap();
    adopt::plan(root).unwrap().apply(root).unwrap();
    let after = fs::read_to_string(root.join("import.test.js")).unwrap();
    assert!(after.contains("from \"@uniflowed/test\""));
    assert!(after.contains("// 日本語 @jest/globals"));
    assert!(after.contains("expect('@jest/globals').toBe('@jest/globals')"));
}

#[test]
fn adoption_rejects_malformed_manifest_fields_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    for field in ["scripts", "dependencies", "devDependencies"] {
        for value in [
            json!(null),
            json!(false),
            json!(42),
            json!("invalid"),
            json!([]),
        ] {
            let before = serde_json::to_string(&json!({ "name": "app", field: value })).unwrap();
            fs::write(root.join("package.json"), &before).unwrap();
            let error = adopt::plan(root).expect_err("invalid field must be rejected");
            assert!(error.to_string().contains(&format!("package.json#{field}")));
            assert_eq!(
                fs::read_to_string(root.join("package.json")).unwrap(),
                before
            );
            assert!(!root.join("uf.config.js").exists());
            assert!(!root.join(".uf").exists());
        }
    }
}

#[test]
fn stale_plan_writes_nothing_and_symlinks_are_not_followed() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{"name":"app","scripts":{"test":"jest"}}"#,
    )
    .unwrap();
    let plan = adopt::plan(root).unwrap();
    fs::write(root.join("package.json"), "{\"name\":\"newer-edit\"}").unwrap();
    assert!(plan.apply(root).is_err());
    assert!(!root.join("uf.config.js").exists());
    #[cfg(unix)]
    {
        let target = root.join("original.json");
        fs::rename(root.join("package.json"), &target).unwrap();
        std::os::unix::fs::symlink(&target, root.join("package.json")).unwrap();
        assert!(adopt::plan(root).unwrap().apply(root).is_err());
    }
}

#[test]
fn catalogued_previous_release_fixture_has_no_deprecated_tool_keys_after_migration() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::write(
        root.join("uf.config.js"),
        include_str!("../../../tests/fixtures/migrations/alpha32/uf.config.js"),
    )
    .unwrap();
    let plan = codemod::plan(root, Some("0.0.0-alpha.32"), "0.0.0-alpha.41").unwrap();
    assert!(plan.unmapped.is_empty());
    plan.apply(root).unwrap();
    let config = uf_config::load_config_file(&root.join("uf.config.js")).unwrap();
    assert!(config.tool_deprecations().is_empty());
}

/// The catalogued fixture for `ui-namespaces-1453`: a page written against the
/// prefixed part names migrates to the namespaces, byte for byte what
/// `after.js` says, and a second run over the result changes nothing.
///
/// Planned as the 0.3.0 binary would plan it, because the migration is
/// registered for the release that removed the names and `main`'s binary is
/// older than that until the release is cut.
#[test]
fn catalogued_ui_fixture_moves_every_prefixed_part_onto_its_namespace() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    fs::write(root.join("uf.config.js"), "export default {};\n").unwrap();
    fs::create_dir_all(root.join("app")).unwrap();
    fs::write(
        root.join("app/settings.js"),
        include_str!("../../../tests/fixtures/migrations/ui-namespaces/before.js"),
    )
    .unwrap();

    let plan = codemod::plan_for(root, Some("0.2.0"), "0.3.0", "0.3.0").unwrap();
    assert!(plan.unmapped.is_empty(), "{:?}", plan.unmapped);
    assert!(plan.migrations.iter().any(|id| id == "ui-namespaces-1453"));
    plan.apply(root).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("app/settings.js")).unwrap(),
        include_str!("../../../tests/fixtures/migrations/ui-namespaces/after.js")
    );

    let again = codemod::plan_for(root, Some("0.2.0"), "0.3.0", "0.3.0").unwrap();
    assert!(again.changes.is_empty());

    // A project already past the release is not rewritten again, and a binary
    // older than the target still refuses to plan it.
    let later = codemod::plan_for(root, Some("0.3.0"), "0.3.0", "0.3.0").unwrap();
    assert!(!later.migrations.iter().any(|id| id == "ui-namespaces-1453"));
    assert!(codemod::plan_for(root, Some("0.2.0"), "0.3.0", "0.2.0").is_err());
}

/// `unread-config-keys-1387`: every key ubugeeei-prod/uf#1387 removed comes
/// out, a section it empties goes with it, and what is still read stays with
/// its comments.
///
/// Planned as the 0.10.0 binary would plan it, for the reason the UI fixture
/// above is.
#[test]
fn keys_nothing_read_are_removed_and_the_sections_they_empty_with_them() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    let before = r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    componentDefault: "server",
    orm: { enabled: true },
    builtins: {
      relay: false,
      tui: { beatReactInk: true },
      markdown: {
        module: "@uniflowed/markdown",
        mdx: { jsxImportSource: "@uniflowed/jsx-runtime" },
      },
    },
    rendering: { cache: { actions: false, route: true } },
  },
  // Kept: `uf clean` reads it.
  docs: { enabled: true, outDir: "dist/docs" },
  std: { modules: ["fs"] },
  taskRunner: { engine: "vite-task" },
  test: { runner: { applicationTarget: "web", jsHosts: ["node"] } },
});
"#;
    fs::write(root.join("uf.config.js"), before).unwrap();

    let plan = codemod::plan_for(root, Some("0.9.0"), "0.10.0", "0.10.0").unwrap();
    assert!(plan.unmapped.is_empty(), "{:?}", plan.unmapped);
    assert!(plan.migrations.iter().any(|id| id == codemod::UNREAD_KEYS));
    let after = plan.changes[0].after.clone().unwrap();
    let get = |path: &[&str]| source::get(&after, path).unwrap();
    assert_eq!(
        get(&["app"]),
        Some(json!({
            "builtins": { "relay": false },
            "rendering": { "cache": { "route": true } },
        }))
    );
    assert_eq!(get(&["docs"]), Some(json!({ "outDir": "dist/docs" })));
    assert_eq!(get(&["std"]), None);
    assert_eq!(get(&["taskRunner"]), None);
    assert_eq!(
        get(&["test"]),
        Some(json!({ "runner": { "applicationTarget": "web" } }))
    );
    assert!(after.contains("// Kept: `uf clean` reads it."), "{after}");

    plan.apply(root).unwrap();
    uf_config::load_config_file(&root.join("uf.config.js")).unwrap();
    let again = codemod::plan_for(root, Some("0.9.0"), "0.10.0", "0.10.0").unwrap();
    assert!(again.changes.is_empty());
    let later = codemod::plan_for(root, Some("0.10.0"), "0.10.0", "0.10.0").unwrap();
    assert!(!later.migrations.iter().any(|id| id == codemod::UNREAD_KEYS));
}

/// The two keys whose value decides: a JSX runtime MDX can import is kept,
/// and `allowPackageScripts: false`, enforced from this release, is kept and
/// reported rather than silently changing what `uf run` does.
#[test]
fn keys_that_are_read_now_are_kept_and_the_one_that_changes_behaviour_is_reported() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(temp.path()).unwrap();
    let before = r#"export default {
  app: { builtins: { markdown: { mdx: { jsxImportSource: "preact" } } } },
  taskRunner: { allowPackageScripts: false },
};
"#;
    fs::write(root.join("uf.config.js"), before).unwrap();

    let plan = codemod::plan_for(root, Some("0.9.0"), "0.10.0", "0.10.0").unwrap();
    assert!(plan.changes.is_empty());
    assert_eq!(plan.unmapped.len(), 1, "{:?}", plan.unmapped);
    assert!(plan.unmapped[0].contains("allowPackageScripts"));
}
