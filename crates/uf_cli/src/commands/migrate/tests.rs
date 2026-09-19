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
