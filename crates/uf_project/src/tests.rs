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

    // The runner, not the loader it is started with. `@uniflowed/test`
    // depends on `@uniflowed/host`, so naming the loader here too would put a
    // package a scaffolded library never imports into its manifest.
    assert!(package.contains(r#""@uniflowed/test""#));
    assert!(!package.contains("@uniflowed/host"));
}

/// A scaffolded manifest names the uf that wrote it, exactly.
///
/// Both templates said `"latest"`, which is a dist-tag and not a version, and
/// on npm it did not point at the current release: with `uf@0.0.0-alpha.7` out,
/// `latest` was `0.0.0-alpha.1` on every name these templates write. So the
/// first thing a new project installed was five releases behind the binary that
/// scaffolded it. See ubugeeei-prod/uf#408.
///
/// Two halves to the assertion, and the second is the one that would go
/// unnoticed: every `@uniflowed/*` dependency is at this exact version, and no
/// dependency anywhere in the manifest is a dist-tag. `react` and `react-dom`
/// are not uf's to pin, and keep the ranges they had.
#[test]
fn both_templates_pin_the_version_of_the_uf_that_wrote_them() {
    let version = env!("CARGO_PKG_VERSION");

    for kind in [CreateKind::AppReact, CreateKind::Lib] {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        create_project(
            &root,
            &CreateOptions {
                name: "pinned".to_string(),
                kind,
                force: false,
            },
        )
        .unwrap();

        let package = fs::read_to_string(root.join("package.json")).unwrap();

        // A dist-tag is resolved at install time by whatever the registry
        // points it at that day, which is the whole bug: not "an old version"
        // but "a version nothing in this repository chose".
        assert!(
            !package.contains(r#""latest""#),
            "{kind:?}: the manifest still names a dist-tag:\n{package}"
        );

        let uniflowed: Vec<&str> = package
            .lines()
            .filter(|line| line.contains("\"@uniflowed/"))
            .collect();
        assert!(
            !uniflowed.is_empty(),
            "{kind:?}: the manifest names no @uniflowed/* package at all:\n{package}"
        );
        for line in uniflowed {
            assert!(
                line.contains(&format!("\"{version}\"")),
                "{kind:?}: {} is not pinned to {version}:\n{package}",
                line.trim()
            );
        }
    }

    // And the app template's React, which uf does not release and must not
    // pin: a project is free to move it.
    let dir = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    create_project(
        &root,
        &CreateOptions {
            name: "pinned-app".to_string(),
            kind: CreateKind::AppReact,
            force: false,
        },
    )
    .unwrap();
    let package = fs::read_to_string(root.join("package.json")).unwrap();
    assert!(package.contains(r#""react": "^19.2.0""#), "{package}");
    assert!(package.contains(r#""react-dom": "^19.2.0""#), "{package}");
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
        // The two `.env` files that are a developer's own. The tracked ones
        // beside them are the project's defaults and are deliberately absent
        // from this list; a credential belongs in a `.local` file, which is
        // why that is the one uf refuses to commit for you.
        for entry in [".env.local", ".env.*.local"] {
            assert!(
                ignored.lines().any(|line| line.trim() == entry),
                "{kind:?}: {entry} is not in .gitignore:\n{ignored}"
            );
        }

        // Three more that no ignore list knows about. `router.js` and
        // `server-actions.js` are generated Flow that looks hand-written —
        // this repository ignores its own `docs/router.js` for the same
        // reason — and `.uniflowed/` is where `uf env use` records the active
        // environment.
        assert!(named("router.js"), "{kind:?}:\n{ignored}");
        assert!(named("server-actions.js"), "{kind:?}:\n{ignored}");
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

// ---------------------------------------------------------------------------
// Discovery at its edges: a large tree, ignored directories, bytes that are
// not UTF-8, symlinks, names an OS allows and a person would not write, and
// paths that stop existing while they are being read.
// ---------------------------------------------------------------------------

use std::os::unix::fs::{PermissionsExt, symlink};
use std::time::{Duration, Instant};

/// A temporary project root.
fn project_root() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a UTF-8 temp path");
    (dir, root)
}

/// Write `contents` at `relative`, creating the directories above it.
fn write(root: &Utf8Path, relative: &str, contents: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("the directories");
    fs::write(&path, contents).unwrap_or_else(|error| panic!("writing {path}: {error}"));
}

/// The discovered paths, in the order discovery returned them.
fn paths(scan: &SourceScan) -> Vec<&str> {
    scan.files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect()
}

/// A project the size of a real monorepo is walked once, and quickly.
///
/// The numbers, so that a regression is a number and not a feeling: twenty
/// packages of two hundred and fifty sources each is 5,000 files uf owns, and
/// beside them 20,000 files it must not open — a `node_modules` per package,
/// a build directory per package, and a vendored checkout with its own `.git`.
/// Discovery must return exactly the 5,000 and read none of the rest.
///
/// The bound is 5 seconds against a measured 0.14s for the walk (0.137s,
/// 0.139s, 0.143s over three runs of an unoptimised `cargo test`, arm64
/// macOS 26.5, 12 cores, APFS, warm page cache). Thirty-five times the
/// measurement is deliberately not a benchmark: it is the distance between
/// this walk and one that reads an ignored tree, stats a path twice per
/// entry, or has gone quadratic — all of which cost more than that. A tighter
/// bound would fail on a loaded CI box and teach everyone to ignore it.
#[test]
fn a_large_project_is_walked_once_and_within_a_bound() {
    const PACKAGES: usize = 20;
    const DIRECTORIES: usize = 10;
    const SOURCES: usize = 25;
    const IGNORED_PER_PACKAGE: usize = 1_000;

    let (_dir, root) = project_root();
    for package in 0..PACKAGES {
        for directory in 0..DIRECTORIES {
            for source in 0..SOURCES {
                write(
                    &root,
                    &format!("packages/p{package}/src/d{directory}/f{source}.js"),
                    "// @flow\nexport const a: number = 1;\n",
                );
            }
        }
        for file in 0..IGNORED_PER_PACKAGE / 2 {
            write(
                &root,
                &format!("packages/p{package}/node_modules/dep/lib/v{file}.js"),
                "// vendored\n",
            );
            write(
                &root,
                &format!("packages/p{package}/dist/assets/b{file}.js"),
                "// built\n",
            );
        }
    }
    write(&root, "vendor/upstream/.git/config", "[core]\n");
    write(&root, "vendor/upstream/lib.js", "// somebody else's\n");

    let started = Instant::now();
    let scan = scan_source_files(&root, &UniflowedConfig::default()).expect("a walk");
    let took = started.elapsed();

    assert_eq!(scan.files.len(), PACKAGES * DIRECTORIES * SOURCES);
    assert!(scan.unreadable.is_empty(), "{:?}", scan.unreadable);
    assert!(
        took < Duration::from_secs(5),
        "walking {} files past {} ignored ones took {took:?}",
        scan.files.len(),
        PACKAGES * IGNORED_PER_PACKAGE + 1
    );

    // Sorted, and sorted once: every caller renders these in order, and a
    // caller that has to sort them again is a caller that will forget to.
    let mut sorted = paths(&scan);
    sorted.sort_unstable();
    assert_eq!(paths(&scan), sorted);
}

/// An ignored directory is ignored at every depth, including inside a package
/// inside a workspace.
///
/// A bare name in `lint.ignore` names a kind of directory rather than a place;
/// this is the case that makes the difference visible, because a root-anchored
/// list would have walked into all four of these.
#[test]
fn an_ignored_directory_is_ignored_at_every_depth() {
    let (_dir, root) = project_root();
    write(&root, "packages/ui/src/button.js", "// @flow\n");
    write(
        &root,
        "packages/ui/node_modules/dep/index.js",
        "// vendored\n",
    );
    write(
        &root,
        "packages/ui/node_modules/dep/node_modules/deeper/index.js",
        "// vendored twice\n",
    );
    write(&root, "packages/ui/dist/button.js", "// built\n");
    write(&root, "packages/ui/target/debug/build.js", "// cargo's\n");
    write(
        &root,
        "packages/ui/.uf/cache/transform/a.js",
        "// uf's own\n",
    );
    write(&root, ".git/hooks/pre-commit.js", "// git's own\n");

    let scan = scan_source_files(&root, &UniflowedConfig::default()).expect("a walk");

    assert_eq!(paths(&scan), ["packages/ui/src/button.js"]);
}

/// `.uf` is uf's own working directory, and a project cannot opt back into it.
///
/// The other names are ordinary `lint.ignore` entries a project may remove;
/// this one is not, because the transform cache and the compiled config are
/// uf's own output and linting them says nothing about the project.
#[test]
fn a_project_cannot_opt_back_into_ufs_own_working_directory() {
    let (_dir, root) = project_root();
    write(&root, "src/app.js", "// @flow\n");
    write(&root, ".uf/cache/transform/a.js", "// uf's own\n");
    write(&root, "node_modules/dep/index.js", "// vendored\n");

    // Everything the default list holds, removed. `.uf` is not on that list.
    let mut config = UniflowedConfig::default();
    config.lint.ignore.clear();

    let scan = scan_source_files(&root, &config).expect("a walk");

    assert!(
        paths(&scan).contains(&"node_modules/dep/index.js"),
        "clearing lint.ignore must un-ignore node_modules: {:?}",
        paths(&scan)
    );
    assert!(
        !paths(&scan).iter().any(|path| path.starts_with(".uf/")),
        "`.uf` is uf's own and is not a project's to un-ignore: {:?}",
        paths(&scan)
    );
}

/// Which bytes make a source unreadable, and which do not.
///
/// The line is UTF-8 validity and nothing else, so all four of these have to
/// be checked together or the boundary is guesswork:
///
/// * a byte-order mark is valid UTF-8 and stays in the source — the formatter
///   prints it back, and a discovery that ate it would silently rewrite every
///   BOM-prefixed file in a project the first time `uf fmt` ran;
/// * a source that stops in the middle of a multi-byte character is not, and
///   is the shape a truncated download or a bad merge actually takes;
/// * UTF-16 is not, however well-formed it is as UTF-16;
/// * an empty file is trivially valid, and is a file rather than a failure.
#[test]
fn a_source_is_unreadable_when_its_bytes_are_not_utf8_and_not_otherwise() {
    let (_dir, root) = project_root();
    write(&root, "src/bom.js", b"\xef\xbb\xbf// @flow\n");
    write(&root, "src/empty.js", b"");
    write(&root, "src/truncated.js", b"// caf\xc3");
    let mut utf16 = vec![0xff, 0xfe];
    for unit in "// @flow\n".encode_utf16() {
        utf16.extend_from_slice(&unit.to_le_bytes());
    }
    write(&root, "src/utf16.js", &utf16);

    let scan = scan_source_files(&root, &UniflowedConfig::default()).expect("a walk");

    assert_eq!(paths(&scan), ["src/bom.js", "src/empty.js"]);
    assert_eq!(scan.files[0].source, "\u{feff}// @flow\n");
    assert_eq!(scan.files[1].source, "");
    assert_eq!(
        scan.unreadable
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<Vec<_>>(),
        ["src/truncated.js", "src/utf16.js"]
    );
    for failure in &scan.unreadable {
        assert!(
            failure.reason.contains("UTF-8"),
            "{}: {}",
            failure.relative_path,
            failure.reason
        );
    }
}

/// Symlinks are not followed, so a cycle is a walk that ends.
///
/// A link to the root from inside the root is the shape that hangs a walker
/// that follows links: there is no depth at which it stops, and the failure
/// is a command that never returns rather than one that reports an error.
///
/// The rest of the assertion is what not following costs, said out loud: a
/// symlinked *file* is not discovered either, so a project that links a source
/// into place is not linting it. That is the right trade — the alternative is
/// formatting one file twice under two names, or writing through a link into
/// a directory outside the project — but it is a trade, not an oversight.
#[test]
fn a_symlink_cycle_ends_the_walk_rather_than_hanging_it() {
    let (_dir, root) = project_root();
    write(&root, "src/real.js", "// @flow\n");
    write(&root, "shared/lib.js", "// @flow\n");
    symlink(root.join("src/real.js"), root.join("src/link.js")).expect("a file link");
    symlink(root.join("shared"), root.join("src/shared")).expect("a directory link");
    symlink(root.as_std_path(), root.join("src/up")).expect("a cycle");
    symlink(root.join("src/missing.js"), root.join("src/dangling.js")).expect("a dangling link");

    let started = Instant::now();
    let scan = scan_source_files(&root, &UniflowedConfig::default()).expect("a walk");
    let took = started.elapsed();

    assert!(took < Duration::from_secs(5), "the cycle took {took:?}");
    assert_eq!(paths(&scan), ["shared/lib.js", "src/real.js"]);
    assert!(scan.unreadable.is_empty(), "{:?}", scan.unreadable);
}

/// Names an operating system allows and nobody would type.
///
/// A newline in a filename is the one that matters, because every report uf
/// prints is line-oriented: discovery has to carry it through rather than
/// truncate it, or the path in the message is not the path on disk. The other
/// two are the ordinary international case and the length limit — a path one
/// byte under `PATH_MAX`, which `open(2)` accepts and a naive fixed buffer
/// does not.
#[test]
fn a_name_nobody_would_type_is_still_discovered_whole() {
    let (_dir, root) = project_root();
    write(&root, "src/two\nlines.js", "// @flow\n");
    write(&root, "src/café.js", "// @flow\n");
    write(&root, "src/コンポーネント.js", "// @flow\n");

    // As deep as the filesystem will take it, then a file in the deepest
    // directory that took. `PATH_MAX` is 1024 on macOS and 4096 on Linux, so
    // the loop finds the limit rather than assuming one.
    let mut deepest = root.join("src");
    loop {
        let next = deepest.join("d".repeat(60));
        if next.as_str().len() > 4_000 || fs::create_dir(&next).is_err() {
            break;
        }
        deepest = next;
    }
    let long = (1..=60)
        .rev()
        .map(|length| deepest.join(format!("{}.js", "n".repeat(length))))
        .find(|path| fs::write(path, "// @flow\n").is_ok())
        .expect("a file at the length limit");
    let long = long
        .strip_prefix(&root)
        .expect("under the root")
        .as_str()
        .to_owned();
    assert!(
        long.len() > 200,
        "the deep path is only {} bytes",
        long.len()
    );

    let scan = scan_source_files(&root, &UniflowedConfig::default()).expect("a walk");

    assert!(scan.unreadable.is_empty(), "{:?}", scan.unreadable);
    let mut found = paths(&scan);
    found.sort_unstable();
    let mut expected = vec![
        "src/two\nlines.js",
        "src/café.js",
        "src/コンポーネント.js",
        long.as_str(),
    ];
    expected.sort_unstable();
    assert_eq!(found, expected);
}

/// Whether a mode of `0o000` actually stops this process reading a path.
///
/// It does not for a process with `CAP_DAC_OVERRIDE` — root in a container,
/// which is how some CI images run. The permission tests would then fail while
/// the scanner is right, which is the worst kind of red: a correct change
/// looks broken. So they ask first and assert what they can.
fn permissions_are_enforced(dir: &Utf8Path) -> bool {
    let probe = dir.join(".permission-probe");
    if fs::write(probe.as_std_path(), "probe").is_err() {
        return false;
    }
    let locked = fs::set_permissions(probe.as_std_path(), fs::Permissions::from_mode(0o000))
        .is_ok()
        && fs::read_to_string(probe.as_std_path()).is_err();
    let _ = fs::set_permissions(probe.as_std_path(), fs::Permissions::from_mode(0o644));
    let _ = fs::remove_file(probe.as_std_path());
    locked
}

/// A file uf can see and cannot open is reported, and the walk goes on.
///
/// The permission case is the one that can be built on purpose; a file deleted
/// between the walk and the read reaches the same arm with a different errno,
/// which is what the test below covers.
#[test]
fn a_file_that_cannot_be_opened_is_reported_and_the_walk_goes_on() {
    let (_dir, root) = project_root();
    if !permissions_are_enforced(&root) {
        return;
    }
    write(&root, "src/aaa.js", "// @flow\n");
    write(&root, "src/locked.js", "// @flow\n");
    write(&root, "src/zzz.js", "// @flow\n");
    fs::set_permissions(
        root.join("src/locked.js").as_std_path(),
        fs::Permissions::from_mode(0o000),
    )
    .expect("to lock the file");

    let scan = scan_source_files(&root, &UniflowedConfig::default());
    let _ = fs::set_permissions(
        root.join("src/locked.js").as_std_path(),
        fs::Permissions::from_mode(0o644),
    );
    let scan = scan.expect("a walk");

    assert_eq!(paths(&scan), ["src/aaa.js", "src/zzz.js"]);
    assert_eq!(scan.unreadable.len(), 1, "{:?}", scan.unreadable);
    assert_eq!(scan.unreadable[0].relative_path, "src/locked.js");
    assert!(
        scan.unreadable[0].reason.contains("Permission denied"),
        "{}",
        scan.unreadable[0].reason
    );
}

/// A directory uf can see and cannot open is reported, not fatal.
///
/// It used to end the walk: `walkdir` hands an unopenable directory back as an
/// error item, discovery returned it, and `uf fmt`, `uf lint`, `uf check` and
/// `uf test` then did nothing at all for the rest of the project. That is the
/// same failure a single non-UTF-8 file used to cause — ubugeeei-prod/uf#164 —
/// one level up, and it has the same answer: name the path that could not be
/// read, do the rest of the work, and fail at the end.
#[test]
fn a_directory_that_cannot_be_opened_is_reported_rather_than_ending_the_walk() {
    let (_dir, root) = project_root();
    if !permissions_are_enforced(&root) {
        return;
    }
    write(&root, "src/app.js", "// @flow\n");
    write(&root, "secret/hidden.js", "// @flow\n");
    // Sorted after `secret`, so a walk that stopped at the error would still
    // have found `src/app.js` and the assertion below would pass by accident.
    write(&root, "zzz/last.js", "// @flow\n");
    fs::set_permissions(
        root.join("secret").as_std_path(),
        fs::Permissions::from_mode(0o000),
    )
    .expect("to lock the directory");

    let scan = scan_source_files(&root, &UniflowedConfig::default());
    let _ = fs::set_permissions(
        root.join("secret").as_std_path(),
        fs::Permissions::from_mode(0o755),
    );
    let scan = scan.expect("an unopenable directory is not fatal");

    assert_eq!(paths(&scan), ["src/app.js", "zzz/last.js"]);
    assert_eq!(scan.unreadable.len(), 1, "{:?}", scan.unreadable);
    assert_eq!(scan.unreadable[0].relative_path, "secret");
    assert!(
        scan.unreadable[0].reason.contains("Permission denied"),
        "{}",
        scan.unreadable[0].reason
    );
}

/// A root that cannot be read is still an error.
///
/// The line the test above draws has to stop somewhere: reporting a missing
/// root as "0 files, one of which was unreadable" would let `uf lint --cwd
/// typo` succeed at doing nothing.
#[test]
fn a_root_that_does_not_exist_is_an_error_rather_than_an_empty_project() {
    let (_dir, root) = project_root();
    let missing = root.join("no-such-directory");

    let error = scan_source_files(&missing, &UniflowedConfig::default())
        .expect_err("a missing root is fatal");

    assert!(matches!(error, ProjectError::Walk { .. }), "{error:?}");
}

/// A file that disappears between the walk and the read never fails the scan.
///
/// `walkdir` reads a directory's entries in blocks, so a file removed after
/// its block was read is still handed back as a regular file and the read then
/// fails with `ENOENT`. It happens for real whenever a watch rebuild, a
/// package manager or an editor is writing while uf is walking, and the only
/// acceptable answer is the one an unreadable file already gets: report it,
/// keep the rest.
///
/// Deleting half of a five-thousand-file tree while it is being walked is what
/// makes the window certain to be hit. The assertion holds whether or not any
/// individual file lost the race, so the test cannot fail for being unlucky —
/// only for the scan giving up.
#[test]
fn a_file_that_vanishes_between_the_walk_and_the_read_never_fails_the_scan() {
    const FILES: usize = 5_000;

    let (_dir, root) = project_root();
    for file in 0..FILES {
        write(
            &root,
            &format!("src/d{}/f{file}.js", file % 50),
            "// @flow\nexport const a: number = 1;\n",
        );
    }
    write(&root, "keep.js", "// @flow\n");

    let deleting = root.clone();
    let deleter = std::thread::spawn(move || {
        for file in 0..FILES {
            if file % 2 == 0 {
                let _ = fs::remove_file(deleting.join(format!("src/d{}/f{file}.js", file % 50)));
            }
        }
    });
    let scan = scan_source_files(&root, &UniflowedConfig::default())
        .expect("a file that vanished mid-walk is not fatal");
    deleter.join().expect("the deleter finished");

    // `keep.js` is never deleted, so a scan that gave up early is visible even
    // in the run where nothing lost the race.
    assert!(paths(&scan).contains(&"keep.js"), "{:?}", paths(&scan));
    assert!(scan.files.len() + scan.unreadable.len() <= FILES + 1);
    for failure in &scan.unreadable {
        assert!(
            failure.reason.contains("No such file"),
            "{}: {}",
            failure.relative_path,
            failure.reason
        );
    }
}
