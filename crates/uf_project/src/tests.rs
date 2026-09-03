use super::*;

#[test]
fn creates_zero_config_react_flow_app() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

    let report = create_project(
        &root,
        &CreateOptions {
            name: "hello-uniflowed".to_string(),
            kind: CreateKind::AppReact,
            force: false,
        },
    )
    .unwrap();

    assert_eq!(report.files.len(), 12);
    assert!(root.join("app.js").exists());
    assert!(root.join("uf.config.js").exists());
    assert!(root.join("app/_uf.page.js").exists());
    assert!(root.join("app/_uf.page.native.js").exists());
    assert!(root.join("server/actions.js").exists());

    let package = fs::read_to_string(root.join("package.json")).unwrap();
    assert!(!package.contains(r#""scripts""#));

    let page = fs::read_to_string(root.join("app/_uf.page.js")).unwrap();
    assert!(page.contains("component Page()"));
    assert!(page.contains("@uniflowed/query"));
    assert!(page.contains("@uniflowed/effect"));
    assert!(page.contains("@uniflowed/state"));
    assert!(page.contains("@uniflowed/fetch"));
    assert!(page.contains("@uniflowed/loader"));
    assert!(page.contains("@uniflowed/validator"));
    assert!(page.contains("@uniflowed/stylex"));
    assert!(page.contains("@uniflowed/relay"));
    assert!(page.contains("Form.Root"));
    assert!(page.contains("Dialog.Body"));
    assert!(root.join("app/client/useCounter.js").exists());
}

#[test]
fn creates_flow_library_template() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

    create_project(
        &root,
        &CreateOptions {
            name: "flow-lib".to_string(),
            kind: CreateKind::Lib,
            force: false,
        },
    )
    .unwrap();

    let index = fs::read_to_string(root.join("index.js")).unwrap();
    let package = fs::read_to_string(root.join("package.json")).unwrap();

    assert!(index.contains("opaque type UniflowedId"));
    assert!(!package.contains(r#""scripts""#));
}

#[test]
fn refuses_to_overwrite_without_force() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::write(root.join("package.json"), "{}").unwrap();

    let error = create_project(
        &root,
        &CreateOptions {
            name: "exists".to_string(),
            kind: CreateKind::Lib,
            force: false,
        },
    )
    .unwrap_err();

    assert!(matches!(error, ProjectError::Exists(_)));
}

#[test]
fn collects_source_files_and_ignores_generated_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app")).unwrap();
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("app/index.js"), "// @flow\n").unwrap();
    fs::write(root.join("dist/index.js"), "// built\n").unwrap();

    let files = collect_source_files(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "app/index.js");
}

#[test]
fn a_build_directory_is_ignored_wherever_it_sits() {
    // uf's own documentation builds into `docs/dist`. Anchoring the ignore
    // list at the project root left those bundles to be linted and offered up
    // for reformatting.
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("docs/app")).unwrap();
    fs::create_dir_all(root.join("docs/dist/assets")).unwrap();
    fs::create_dir_all(root.join("packages/ui/node_modules")).unwrap();
    fs::write(root.join("docs/app/index.js"), "// @flow\n").unwrap();
    fs::write(root.join("docs/dist/assets/app.js"), "// built\n").unwrap();
    fs::write(root.join("packages/ui/node_modules/dep.js"), "// vendor\n").unwrap();

    let files = collect_source_files(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "docs/app/index.js");
}

#[test]
fn an_ignore_entry_with_a_separator_still_means_one_place() {
    // `dist` names a kind of directory; `app/dist` names one directory. A
    // project that ignores the latter has not asked for the former.
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app/generated")).unwrap();
    fs::create_dir_all(root.join("lib/generated")).unwrap();
    fs::write(root.join("app/generated/routes.js"), "// @flow\n").unwrap();
    fs::write(root.join("lib/generated/keep.js"), "// @flow\n").unwrap();

    let mut config = UniflowedConfig::default();
    config.lint.ignore.push("app/generated".into());
    config.lint.files.push("lib".into());

    let files = collect_source_files(&root, &config).unwrap();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "lib/generated/keep.js");
}
