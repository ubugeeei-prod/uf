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

    assert_eq!(report.files.len(), 9);
    assert!(root.join("app.js").exists());
    assert!(root.join("uf.config.js").exists());
    assert!(root.join("app/_uf.page.js").exists());
    assert!(root.join("app/Counter.js").exists());

    let package = fs::read_to_string(root.join("package.json")).unwrap();
    assert!(!package.contains(r#""scripts""#));

    // Every dependency a scaffolded project declares has to be a package that
    // is actually implemented, or the project cannot start. The declaration
    // packages that throw when called must not appear here.
    for stub in [
        "@uniflowed/core",
        "@uniflowed/effect",
        "@uniflowed/fetch",
        "@uniflowed/loader",
        "@uniflowed/query",
        "@uniflowed/react-native",
        "@uniflowed/react-testing",
        "@uniflowed/relay",
        "@uniflowed/server",
        "@uniflowed/stylex",
        "@uniflowed/ui",
    ] {
        assert!(!package.contains(stub), "{stub} is not implemented yet");
        for file in &report.files {
            let contents = fs::read_to_string(file).unwrap();
            assert!(
                !contents.contains(stub),
                "{file} imports {stub}, which is not implemented yet"
            );
        }
    }

    let page = fs::read_to_string(root.join("app/_uf.page.js")).unwrap();
    assert!(page.contains("component Page()"));
    assert!(page.contains("enum Mood"));
    assert!(page.contains("match (mood)"));

    let hook = fs::read_to_string(root.join("app/useCounter.js")).unwrap();
    assert!(hook.contains("hook useCounter"));
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

/// A scaffolded project does not commit what uf generates.
///
/// It had no `.gitignore` at all, so the first `uf build` put `dist/`,
/// `router.js` and `.uf/` into `git status` and the first commit of a new
/// project carried them.
///
/// The list is not written twice: anything uf refuses to *lint* because it
/// generated it has to be something uf refuses to *commit*, so the template
/// is checked against `ALWAYS_IGNORED` and the default `lint.ignore`. `target`
/// is the exception and is named as one — it is Cargo's, and a scaffolded
/// Flow project has none.
#[test]
fn both_templates_ignore_what_uf_generates() {
    for kind in [CreateKind::AppReact, CreateKind::Lib] {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        create_project(
            &root,
            &CreateOptions {
                name: "ignored".to_string(),
                kind,
                force: false,
            },
        )
        .unwrap();

        let ignored = fs::read_to_string(root.join(".gitignore")).unwrap();
        let named = |entry: &str| {
            ignored
                .lines()
                .any(|line| line.trim_end_matches('/') == entry.trim_end_matches('/'))
        };

        // `.git` is git's own and `target` is Cargo's: uf skips both when it
        // walks a project, and neither is something a Flow project commits.
        for entry in ALWAYS_IGNORED {
            if *entry == ".git" {
                continue;
            }
            assert!(
                named(entry),
                "{kind:?}: {entry} is not in .gitignore:\n{ignored}"
            );
        }
        for entry in &UniflowedConfig::default().lint.ignore {
            if entry == "target" {
                continue;
            }
            assert!(
                named(entry),
                "{kind:?}: {entry} is not in .gitignore:\n{ignored}"
            );
        }
        // Two more that no ignore list knows about. `router.js` is generated
        // Flow that looks hand-written — this repository ignores its own
        // `docs/router.js` for the same reason — and `.uniflowed/` is where
        // `uf env use` records the active environment.
        assert!(named("router.js"), "{kind:?}:\n{ignored}");
        assert!(named(".uniflowed"), "{kind:?}:\n{ignored}");
    }
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

    let files = scan_source_files(&root, &UniflowedConfig::default())
        .unwrap()
        .files;

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "app/index.js");
}

/// A file that is not UTF-8 is reported, and the rest of the project is
/// still discovered.
///
/// One stray byte used to abort the walk. `uf fmt`, `uf lint`, `uf check` and
/// `uf doc` all stopped at the first such file and left every other file in
/// the project untouched — a build artifact or a vendored blob with a `.js`
/// name was enough. See ubugeeei-prod/uf#164.
#[test]
fn a_source_that_is_not_utf8_is_reported_rather_than_fatal() {
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/ok.js"), "// @flow\n").unwrap();
    // Sorted after `ok.js`, so a walk that stops at the first failure would
    // still have found the readable one and this test would pass by accident.
    fs::write(root.join("src/aaa.js"), [0xff, 0xfe, 0xfa]).unwrap();

    let scan = scan_source_files(&root, &UniflowedConfig::default()).unwrap();

    assert_eq!(scan.files.len(), 1);
    assert_eq!(scan.files[0].relative_path, "src/ok.js");
    assert_eq!(scan.unreadable.len(), 1);
    assert_eq!(scan.unreadable[0].relative_path, "src/aaa.js");
    assert!(
        scan.unreadable[0].reason.contains("UTF-8"),
        "{}",
        scan.unreadable[0].reason
    );
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

    let files = scan_source_files(&root, &UniflowedConfig::default())
        .unwrap()
        .files;

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "docs/app/index.js");
}

#[test]
fn a_nested_repository_is_not_this_project() {
    // `upstream/flow` is Meta's source, vendored as a submodule. `uf fmt`
    // reformatted it, and the next sync would have discarded the result —
    // a directory with a `.git` in it belongs to another history.
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    fs::create_dir_all(root.join("app")).unwrap();
    fs::create_dir_all(root.join("vendor/upstream/.git")).unwrap();
    fs::write(root.join("app/index.js"), "// @flow\n").unwrap();
    fs::write(root.join("vendor/upstream/lib.js"), "// @flow\n").unwrap();

    let files = scan_source_files(&root, &UniflowedConfig::default())
        .unwrap()
        .files;

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "app/index.js");
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

    let files = scan_source_files(&root, &config).unwrap().files;

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].relative_path, "lib/generated/keep.js");
}

/// The `test` task a scaffolded project gets must run its tests.
///
/// Both templates shipped `uf test --list`, which lists what would run and
/// runs none of it. A generated project therefore had a green `uf run test`
/// that executed nothing, which is worse than having no task at all: it is a
/// check that reports success without checking.
#[test]
fn a_scaffolded_project_gets_a_test_task_that_runs_tests() {
    for (kind, files) in [("app", app_react_files("demo")), ("lib", lib_files("demo"))] {
        let config = files
            .iter()
            .find(|(path, _)| *path == "uf.config.js")
            .map(|(_, contents)| contents.clone())
            .unwrap_or_else(|| panic!("the {kind} template writes a uf.config.js"));

        assert!(
            config.contains(r#"test: { command: "uf test" }"#),
            "the {kind} template's test task must run the tests:\n{config}"
        );
        assert!(
            !config.contains("--list"),
            "`uf test --list` runs nothing, so a task that uses it always passes:\n{config}"
        );
    }
}

/// Every task a template scaffolds has to name a command uf actually has.
#[test]
fn scaffolded_tasks_name_real_commands() {
    const COMMANDS: &[&str] = &[
        "build", "check", "create", "dev", "env", "exec", "explain", "fmt", "info", "inspect",
        "install", "lint", "lsp", "prepare", "publish", "release", "run", "test", "upgrade", "use",
    ];

    for (kind, files) in [("app", app_react_files("demo")), ("lib", lib_files("demo"))] {
        let config = files
            .iter()
            .find(|(path, _)| *path == "uf.config.js")
            .map(|(_, contents)| contents.clone())
            .expect("a config");

        for line in config.lines() {
            let Some(rest) = line.split_once(r#"command: "uf "#) else {
                continue;
            };
            let command = rest
                .1
                .split([' ', '"'])
                .next()
                .expect("a command follows `uf `");
            assert!(
                COMMANDS.contains(&command),
                "the {kind} template scaffolds `uf {command}`, which is not a uf command"
            );
        }
    }
}

/// Every `@uniflowed/*` a template scaffolds has to be a package that is
/// actually published.
///
/// `uf create` writes a `package.json` and the next thing anyone runs is
/// `uf install`. A dependency on a name npm does not serve makes that fail with
/// a 404 — not uf's error message, npm's — and the project is unusable before
/// it has been opened. This repository has shipped that failure before, with a
/// scaffold that imported ten packages which did not exist.
///
/// `tools/release/published-packages.txt` is the list of names a release
/// publishes, and it is deliberately shorter than `packages/`: most of those
/// are declarations whose functions throw. A package earns its way onto that
/// list by being implemented, and only then may a template depend on it.
#[test]
fn every_scaffolded_dependency_is_a_published_package() {
    let list = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/release/published-packages.txt"),
    )
    .expect("the publish list is readable");
    let published = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|name| format!("@uniflowed/{name}"))
        .collect::<std::collections::BTreeSet<_>>();

    let mut unpublished = Vec::new();
    for (kind, files) in [("app", app_react_files("demo")), ("lib", lib_files("demo"))] {
        let manifest = files
            .iter()
            .find(|(path, _)| *path == "package.json")
            .map(|(_, contents)| contents.clone())
            .expect("a manifest");

        for line in manifest.lines() {
            let Some(name) = line.split('"').nth(1) else {
                continue;
            };
            if !name.starts_with("@uniflowed/") || published.contains(name) {
                continue;
            }
            unpublished.push(format!("the {kind} template depends on {name}"));
        }
    }

    assert!(
        unpublished.is_empty(),
        "{}\n\nadd the package to tools/release/published-packages.txt once it is \
         implemented, and depend on it once a release has published it",
        unpublished.join("\n")
    );
}
