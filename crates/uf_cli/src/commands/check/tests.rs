//! The `uf check` payload, and how the two halves of it are combined.

use uf_lint::Diagnostic;

use super::*;

fn lint_report(errors: usize, warnings: usize) -> LintReport {
    let mut diagnostics = Vec::new();
    for index in 0..errors {
        diagnostics.push(diagnostic(index, Severity::Error));
    }
    for index in 0..warnings {
        diagnostics.push(diagnostic(index, Severity::Warn));
    }
    LintReport {
        diagnostics,
        files_checked: 2,
        unavailable: Vec::new(),
    }
}

fn diagnostic(index: usize, severity: Severity) -> Diagnostic {
    Diagnostic {
        rule: "flow/unclear-type",
        severity,
        path: Some("a.js".to_string()),
        line: index + 1,
        column: 1,
        message: "unclear type".to_string(),
    }
}

#[test]
fn the_payload_keeps_the_shape_uf_lint_emits() {
    let value = payload(&lint_report(1, 1), &TypeCheck::Unavailable, None);

    assert_eq!(value["command"], json!("uf check"));
    assert_eq!(value["filesChecked"], json!(2));
    assert_eq!(value["errors"], json!(1));
    assert_eq!(value["warnings"], json!(1));
    assert!(value["diagnostics"].is_array());
    assert!(value["unavailableRules"].is_array());
}

#[test]
fn the_payload_names_the_checker_backend_and_its_status() {
    let value = payload(&lint_report(0, 0), &TypeCheck::Unavailable, None);

    assert_eq!(value["typeCheck"]["status"], json!("unavailable"));
    #[cfg(feature = "upstream-typecheck")]
    assert_eq!(value["typeCheck"]["backend"], json!(type_backend_name()));
    #[cfg(not(feature = "upstream-typecheck"))]
    assert_eq!(value["typeCheck"]["backend"], json!("unavailable"));
    assert_eq!(value["typeCheck"]["diagnostics"], json!([]));
}

#[cfg(feature = "upstream-typecheck")]
#[test]
fn a_checker_failure_is_reported_without_losing_the_lint_counts() {
    let types = TypeCheck::Failed(CheckError::SourceTooLarge {
        path: "big.js".into(),
        size: 8,
        limit: 4,
    });

    let value = payload(&lint_report(2, 0), &types, None);

    assert_eq!(value["errors"], json!(2));
    assert_eq!(value["typeCheck"]["status"], json!("failed"));
    assert!(
        value["typeCheck"]["error"]
            .as_str()
            .expect("an error string")
            .contains("big.js")
    );
}

#[test]
fn an_unavailable_checker_contributes_no_counts() {
    let types = TypeCheck::Unavailable;

    assert_eq!(types.count(TypeSeverity::Error), 0);
    assert_eq!(types.count(TypeSeverity::Warning), 0);
    #[cfg(feature = "upstream-typecheck")]
    assert!(types.diagnostics().is_empty());
    #[cfg(feature = "upstream-typecheck")]
    assert!(types.report().is_none());
}

#[test]
fn statuses_are_stable() {
    assert_eq!(TypeCheck::Unavailable.status(), "unavailable");
    #[cfg(feature = "upstream-typecheck")]
    assert_eq!(
        TypeCheck::Failed(CheckError::Unavailable).status(),
        "failed"
    );
}

#[test]
fn a_build_with_a_checker_reports_it_and_one_without_says_so() {
    let types = type_check(&[], &[], Utf8Path::new("."));

    #[cfg(feature = "upstream-typecheck")]
    if uf_check::is_available() {
        assert_eq!(types.status(), "checked");
        assert_eq!(
            value_of(&types)["filesChecked"],
            json!(0),
            "an empty project checks zero files"
        );
    } else {
        assert_eq!(types.status(), "unavailable");
    }
    #[cfg(not(feature = "upstream-typecheck"))]
    assert_eq!(types.status(), "unavailable");
}

#[cfg(feature = "upstream-typecheck")]
fn value_of(types: &TypeCheck) -> Value {
    payload(&lint_report(0, 0), types, None)["typeCheck"].clone()
}

/// A project small enough to read, holding one workspace package.
///
/// `app.js` names a type it can only get from `@uniflowed/ids`, and
/// `unrelated.js` is wrong on purpose: it is what says whether a check that
/// was asked about one file stayed there.
#[cfg(feature = "upstream-typecheck")]
fn workspace() -> Vec<SourceFile> {
    fn file(path: &str, source: &str) -> SourceFile {
        SourceFile {
            path: path.to_owned(),
            source: source.to_owned(),
        }
    }
    vec![
        file(
            "app.js",
            "// @flow\nimport type { Id } from \"@uniflowed/ids\";\nexport const id: Id = \"a\";\n",
        ),
        file(
            "packages/ids/package.json",
            "{ \"name\": \"@uniflowed/ids\", \"exports\": { \".\": \"./index.js\" } }",
        ),
        file(
            "packages/ids/index.js",
            "// @flow\nexport type Id = string;\n",
        ),
        file(
            "unrelated.js",
            "// @flow\nexport const n: number = \"not a number\";\n",
        ),
    ]
}

/// A root nothing else writes to: `type_check` keeps its cache under one.
#[cfg(feature = "upstream-typecheck")]
fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

#[cfg(feature = "upstream-typecheck")]
fn codes(types: &TypeCheck) -> Vec<&'static str> {
    types
        .diagnostics()
        .iter()
        .filter_map(|diagnostic| diagnostic.code)
        .collect()
}

/// The bug, stated as a test: a batch of one file resolves nothing, so the
/// type it imports is an `any`-typed value and the annotation goes unread.
/// ubugeeei-prod/uf#403.
#[cfg(feature = "upstream-typecheck")]
#[test]
fn a_file_checked_with_nothing_else_cannot_see_the_package_it_imports() {
    if !uf_check::is_available() {
        return;
    }
    let project = workspace();
    let root = scratch();

    let types = type_check(
        &project[..1],
        &[],
        Utf8Path::from_path(root.path()).expect("a UTF-8 path"),
    );

    assert_eq!(codes(&types), ["value-as-type"]);
}

#[cfg(feature = "upstream-typecheck")]
#[test]
fn a_narrowed_check_types_the_file_against_what_it_imports() {
    if !uf_check::is_available() {
        return;
    }
    let project = workspace();
    let root = scratch();

    let types = type_check(
        &project[..1],
        &project,
        Utf8Path::from_path(root.path()).expect("a UTF-8 path"),
    );

    assert_eq!(codes(&types), Vec::<&str>::new());
    let batch = types.batch().expect("a checked batch");
    assert_eq!(batch.requested, 1);
    // The package's own file, and the manifest that publishes its name.
    assert_eq!(batch.imported, 2);
}

/// A file the reader did not name is in the batch to be typed against, not to
/// be reported on. `unrelated.js` is wrong and is nobody's business here.
#[cfg(feature = "upstream-typecheck")]
#[test]
fn a_narrowed_check_reports_only_the_files_it_was_asked_about() {
    if !uf_check::is_available() {
        return;
    }
    let mut project = workspace();
    // Make the dependency itself wrong, so that the only thing keeping its
    // diagnostic off the screen is the filter.
    project[2]
        .source
        .push_str("export const wrong: number = \"no\";\n");
    let root = scratch();

    let types = type_check(
        &project[..1],
        &project,
        Utf8Path::from_path(root.path()).expect("a UTF-8 path"),
    );

    assert_eq!(
        types.diagnostics().len(),
        0,
        "a dependency's own errors are not the caller's: {:#?}",
        types
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.primary.path.as_str(), diagnostic.code))
            .collect::<Vec<_>>()
    );
}

/// The whole-project case: no narrowing, so nothing was left out and every
/// file the scan found is both asked about and checked.
#[cfg(feature = "upstream-typecheck")]
#[test]
fn an_unnarrowed_check_reports_every_file_it_was_given() {
    if !uf_check::is_available() {
        return;
    }
    let project = workspace();
    let root = scratch();

    let types = type_check(
        &project,
        &[],
        Utf8Path::from_path(root.path()).expect("a UTF-8 path"),
    );

    assert_eq!(codes(&types), ["incompatible-type"]);
    let batch = types.batch().expect("a checked batch");
    assert_eq!(batch.requested, project.len());
    assert_eq!(batch.imported, 0);
}

/// A file that reads a type out of a package that ships no Flow types.
///
/// Clean if — and only if — the library definitions that describe the package
/// were merged. Without them `EdgeProps` is an `any`-typed value and using it
/// as a type is an error, which is ubugeeei-prod/uf#480.
#[cfg(feature = "upstream-typecheck")]
const USES_A_LIBDEF: &str = "// @flow\nimport type { EdgeProps } from \"@xyflow/react\";\n\
                             export function label(props: EdgeProps): string {\n  \
                             return props.id;\n}\n";

#[cfg(feature = "upstream-typecheck")]
const LIBDEF: &str = "declare module \"@xyflow/react\" {\n  \
                      declare export type EdgeProps = { readonly id: string };\n}\n";

/// Write `files` into a fresh project root.
#[cfg(feature = "upstream-typecheck")]
fn project_root(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = scratch();
    for (path, source) in files {
        let path = root.path().join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory is made");
        std::fs::write(path, source).expect("the file is written");
    }
    root
}

#[cfg(feature = "upstream-typecheck")]
fn checked_in(root: &tempfile::TempDir, sources: &[SourceFile]) -> TypeCheck {
    type_check(
        sources,
        &[],
        Utf8Path::from_path(root.path()).expect("a UTF-8 path"),
    )
}

#[cfg(feature = "upstream-typecheck")]
fn app() -> Vec<SourceFile> {
    vec![SourceFile {
        path: "src/Edge.js".to_owned(),
        source: USES_A_LIBDEF.to_owned(),
    }]
}

/// The bug, stated as a test. ubugeeei-prod/uf#480.
#[cfg(feature = "upstream-typecheck")]
#[test]
fn a_project_whose_libdefs_are_not_read_reports_every_type_they_declare() {
    if !uf_check::is_available() {
        return;
    }
    // The declarations are on disk, and no configuration names the directory
    // they are in.
    let root = project_root(&[("libdefs/packages.js", LIBDEF)]);

    let types = checked_in(&root, &app());

    assert_eq!(codes(&types), ["value-as-type"]);
    assert_eq!(types.batch().expect("a checked batch").libdefs, 0);
}

#[cfg(feature = "upstream-typecheck")]
#[test]
fn a_project_is_checked_against_the_library_definitions_its_flowconfig_names() {
    if !uf_check::is_available() {
        return;
    }
    let root = project_root(&[
        (".flowconfig", "[libs]\nlibdefs\n"),
        ("libdefs/packages.js", LIBDEF),
    ]);

    let types = checked_in(&root, &app());

    assert_eq!(codes(&types), Vec::<&str>::new());
    assert_eq!(types.batch().expect("a checked batch").libdefs, 1);
    let report = types.report().expect("a report");
    assert!(
        report.untyped_modules.is_empty(),
        "a module the project declared is not a hole: {:?}",
        report.untyped_modules
    );
}

/// `flow-typed` is Flow's own implicit lib path, so a project that has one and
/// no `.flowconfig` still gets it.
#[cfg(feature = "upstream-typecheck")]
#[test]
fn the_conventional_directory_is_read_without_being_configured() {
    if !uf_check::is_available() {
        return;
    }
    let root = project_root(&[("flow-typed/packages.js", LIBDEF)]);

    let types = checked_in(&root, &app());

    assert_eq!(codes(&types), Vec::<&str>::new());
    assert_eq!(types.batch().expect("a checked batch").libdefs, 1);
}

/// A `.flowconfig` a reader wrote and got wrong is reported, not skipped: the
/// alternative is one error per declared type and no explanation.
#[cfg(feature = "upstream-typecheck")]
#[test]
fn a_flowconfig_that_does_not_parse_fails_the_check_rather_than_being_ignored() {
    if !uf_check::is_available() {
        return;
    }
    let root = project_root(&[
        (".flowconfig", "[libs]\nlibdefs\n\n[libs]\nmore\n"),
        ("libdefs/packages.js", LIBDEF),
    ]);

    let types = checked_in(&root, &app());

    assert_eq!(types.status(), "failed");
}

/// The scan collects `flow-typed/` like any other directory, so without a rule
/// the same file would be both the environment and a file checked against it —
/// and `declare module` in a file checked as a source is a syntax error. It is
/// the environment.
#[cfg(feature = "upstream-typecheck")]
#[test]
fn a_library_definition_is_the_environment_and_not_a_file_to_check() {
    if !uf_check::is_available() {
        return;
    }
    let root = project_root(&[("flow-typed/packages.js", LIBDEF)]);
    let mut sources = app();
    sources.push(SourceFile {
        path: "flow-typed/packages.js".to_owned(),
        source: LIBDEF.to_owned(),
    });

    let types = checked_in(&root, &sources);

    assert_eq!(codes(&types), Vec::<&str>::new());
    // And is not counted among the files the reader asked about.
    assert_eq!(types.batch().expect("a checked batch").requested, 1);
}
