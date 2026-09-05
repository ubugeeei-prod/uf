use super::*;

const CLEAN: &str = "// @flow\nconst n: number = 1;\n";

/// A type error that inference reports whenever it runs over the file.
const TYPE_ERROR: &str = "// @flow\nconst n: number = \"not a number\";\n";

/// The same type error in a file that opts out of Flow.
const OPTED_OUT: &str = "// @noflow\nconst n: number = \"not a number\";\n";

/// Skip the body when this build has no checker compiled in.
macro_rules! require_checker {
    () => {
        if !is_available() {
            return;
        }
    };
}

#[test]
fn backend_names_are_stable() {
    assert_eq!(
        backend_name(CheckerBackend::UpstreamRustPort),
        "upstream-flow-rust-port"
    );
    assert_eq!(backend_name(CheckerBackend::Unavailable), "unavailable");
}

#[test]
fn availability_matches_the_compiled_in_backend() {
    if is_available() {
        assert_eq!(active_backend(), CheckerBackend::UpstreamRustPort);
    } else {
        assert_eq!(active_backend(), CheckerBackend::Unavailable);
    }
}

#[test]
fn a_build_without_a_checker_says_so_instead_of_failing_opaquely() {
    if is_available() {
        return;
    }

    let error = check_source(Source::new("app.js", CLEAN), &CheckLimits::default())
        .expect_err("no checker is compiled in");

    assert!(error.is_unavailable());
    assert_eq!(error, CheckError::Unavailable);
}

#[test]
fn only_the_unavailable_error_reports_itself_as_unavailable() {
    assert!(CheckError::Unavailable.is_unavailable());
    assert!(
        !CheckError::SourceTooLarge {
            path: "app.js".into(),
            size: 8,
            limit: 4,
        }
        .is_unavailable()
    );
}

#[test]
fn errors_name_the_file_and_the_limit_they_broke() {
    let error = CheckError::SourceTooLarge {
        path: "src/app.js".into(),
        size: 5_000_000,
        limit: 4_194_304,
    };

    let rendered = error.to_string();

    assert!(rendered.contains("src/app.js"), "{rendered}");
    assert!(rendered.contains("5000000"), "{rendered}");
    assert!(rendered.contains("4194304"), "{rendered}");
}

#[test]
fn a_source_keeps_the_path_diagnostics_are_reported_under() {
    let source = Source::new("src/app.js", CLEAN);

    assert_eq!(source.path, "src/app.js");
    assert_eq!(source.source, CLEAN);
}

#[test]
fn an_empty_batch_is_not_an_error() {
    match check_sources(&[], &CheckLimits::default()) {
        Ok(report) => {
            assert_eq!(report.files_checked, 0);
            assert_eq!(report.files_skipped, 0);
            assert!(report.diagnostics.is_empty());
            assert!(!report.has_errors());
        }
        Err(error) => assert!(error.is_unavailable()),
    }
}

#[test]
fn a_report_counts_by_severity() {
    let report = CheckReport {
        diagnostics: Vec::new(),
        files_checked: 3,
        files_skipped: 0,
        untyped_modules: Vec::new(),
        builtins: BuiltinsTiming {
            elapsed: std::time::Duration::ZERO,
            cold_elapsed: std::time::Duration::ZERO,
            cold: false,
        },
        elapsed: std::time::Duration::from_millis(10),
    };

    assert_eq!(report.count(Severity::Error), 0);
    assert_eq!(report.count(Severity::Warning), 0);
    assert!(!report.has_errors());
    assert_eq!(report.files_per_second(), Some(300.0));
}

#[test]
fn throughput_is_unknown_when_no_time_passed() {
    let report = CheckReport {
        diagnostics: Vec::new(),
        files_checked: 1,
        files_skipped: 0,
        untyped_modules: Vec::new(),
        builtins: BuiltinsTiming {
            elapsed: std::time::Duration::ZERO,
            cold_elapsed: std::time::Duration::ZERO,
            cold: true,
        },
        elapsed: std::time::Duration::ZERO,
    };

    assert_eq!(report.files_per_second(), None);
}

#[test]
fn a_file_that_opts_out_of_flow_is_not_checked() {
    require_checker!();

    let checked = check_source(Source::new("app.js", TYPE_ERROR), &CheckLimits::default())
        .expect("the checker runs");
    assert!(
        checked.iter().any(TypeDiagnostic::is_error),
        "the fixture must be a type error when it is checked, or this test proves nothing"
    );

    let opted_out = check_source(Source::new("app.js", OPTED_OUT), &CheckLimits::default())
        .expect("the checker runs");

    assert!(
        opted_out.is_empty(),
        "`@noflow` must opt a file out of inference, but it reported {opted_out:?}"
    );
}

#[test]
fn opting_out_is_counted_as_skipped_rather_than_checked() {
    require_checker!();

    let report = check_sources(
        &[
            Source::new("checked.js", CLEAN),
            Source::new("plain.js", OPTED_OUT),
        ],
        &CheckLimits::default(),
    )
    .expect("the checker runs");

    assert_eq!(report.files_checked, 1, "only one file opted in");
    assert_eq!(report.files_skipped, 1, "the other opted out");
}

#[test]
fn opting_out_does_not_hide_a_file_that_cannot_be_parsed() {
    require_checker!();

    let diagnostics = check_source(
        Source::new("broken.js", "// @noflow\nfunction ( {\n"),
        &CheckLimits::default(),
    )
    .expect("the checker runs");

    assert!(
        diagnostics.iter().any(TypeDiagnostic::is_error),
        "a file uf must still transform has to parse, whatever its docblock says"
    );
}

#[test]
fn a_file_with_no_docblock_is_still_checked() {
    require_checker!();

    let diagnostics = check_source(
        Source::new("app.js", "const n: number = \"not a number\";\n"),
        &CheckLimits::default(),
    )
    .expect("the checker runs");

    assert!(
        diagnostics.iter().any(TypeDiagnostic::is_error),
        "uf is Flow-first: a file without a pragma is checked, and `@noflow` is \
         the way to say otherwise"
    );
}

/// A toolchain for React applications has to know what a `document` is.
///
/// Flow's `lib/` holds `core.js` and `react.js` and nothing else — every
/// browser and Node global lives in `evals/flow-typed/environment`, which Flow
/// loads through a `.flowconfig`'s `[libs]`. uf has no `.flowconfig`, so
/// without merging them explicitly `uf check` reported 193
/// `cannot-resolve-name` errors against uf's own packages, 41 of them for
/// `Response`.
#[test]
fn the_platform_globals_resolve() {
    require_checker!();

    // One name from each environment that a uf project actually reaches for.
    for (source, name) in [
        ("const el: HTMLElement = document.body;", "dom/html"),
        ("const r: Response = new Response();", "bom/fetch"),
        (
            "const u: URL = new URL(\"https://uniflowed.dev\");",
            "bom/url",
        ),
        ("const w: number = window.innerWidth;", "bom/window"),
        ("const p: string = process.platform;", "node"),
        ("const s: Storage = localStorage;", "bom/storage"),
        ("const t: EventTarget = new EventTarget();", "dom/events"),
    ] {
        let diagnostics = check_source(
            Source::new("app.js", &format!("// @flow\n{source}\n")),
            &CheckLimits::default(),
        )
        .expect("the checker runs");

        let unresolved = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Some("cannot-resolve-name"))
            .collect::<Vec<_>>();
        assert!(
            unresolved.is_empty(),
            "{name}: {source}\n  unresolved: {unresolved:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Checking across modules.
//
// The unit of these tests is a *batch*: one call to `check_sources` standing in
// for the set of files `uf check` collected. What a specifier resolves to is a
// function of that batch and nothing else, so each test states the whole
// project it is about.
// ---------------------------------------------------------------------------

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn batch(sources: &[Source<'_>]) -> CheckReport {
    check_sources(sources, &CheckLimits::default().without_timeout()).expect("the checker runs")
}

fn codes(report: &CheckReport) -> Vec<&str> {
    report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code.unwrap_or("<none>"))
        .collect()
}

fn assert_clean(report: &CheckReport) {
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got {:#?}",
        report
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.primary.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_type_imported_from_another_file_is_a_type() {
    require_checker!();

    // Issue #199's own reproduction: three `value-as-type` errors for one
    // correct import, because the imported module was never read.
    let report = batch(&[
        Source::new(
            "a.js",
            "// @flow\nexport type Mode = \"onSubmit\" | \"onChange\";\n",
        ),
        Source::new(
            "b.js",
            "// @flow\nimport type { Mode } from \"./a.js\";\n\
             export function pick(m: ?Mode): Mode {\n\
             \x20 const mode: Mode = m ?? \"onSubmit\";\n\
             \x20 return mode;\n\
             }\n",
        ),
    ]);

    assert_clean(&report);
    assert_eq!(report.files_checked, 2);
    assert!(
        report.untyped_modules.is_empty(),
        "`./a.js` is in the batch, so nothing about it is untyped: {:?}",
        report.untyped_modules
    );
}

#[test]
fn an_imported_type_still_rejects_a_value_it_does_not_admit() {
    require_checker!();

    // The other half of the same claim: resolving the import must give the
    // checker the *real* type, not a quieter `any`.
    let report = batch(&[
        Source::new(
            "a.js",
            "// @flow\nexport type Mode = \"onSubmit\" | \"onChange\";\n",
        ),
        Source::new(
            "b.js",
            "// @flow\nimport type { Mode } from \"./a.js\";\nconst mode: Mode = \"onNever\";\n",
        ),
    ]);

    assert_eq!(codes(&report), ["incompatible-type"]);
    assert_eq!(report.diagnostics[0].primary.path, "b.js");
}

#[test]
fn a_value_imported_from_another_file_has_the_exported_type() {
    require_checker!();

    let report = batch(&[
        Source::new(
            "a.js",
            "// @flow\nexport function width(n: number): string {\n  return String(n);\n}\n",
        ),
        Source::new(
            "b.js",
            "// @flow\nimport { width } from \"./a.js\";\nconst bad: number = width(1);\n",
        ),
    ]);

    assert_eq!(codes(&report), ["incompatible-type"]);
}

#[test]
fn an_error_about_an_imported_value_can_point_into_the_file_that_declared_it() {
    require_checker!();

    // A dependency is merged from its signature, where every location is an
    // index into that file's own table rather than a line and a column.
    // Rendering one is the difference between a usable note and a panic.
    let report = batch(&[
        Source::new("a.js", "// @flow\nexport const label: string = \"hi\";\n"),
        Source::new(
            "b.js",
            "// @flow\nimport { label } from \"./a.js\";\nconst n: number = label;\n",
        ),
    ]);

    let diagnostic = &report.diagnostics[0];
    assert_eq!(diagnostic.code, Some("incompatible-type"));
    let declaration = diagnostic
        .related
        .iter()
        .find(|related| related.span.path == "a.js")
        .unwrap_or_else(|| panic!("no note points at a.js: {:?}", diagnostic.related));
    assert_eq!(declaration.span.start.line, 2);
}

#[test]
fn a_re_exported_type_resolves_through_the_file_that_re_exported_it() {
    require_checker!();

    let report = batch(&[
        Source::new("a.js", "// @flow\nexport type Mode = \"on\" | \"off\";\n"),
        Source::new(
            "index.js",
            "// @flow\nexport type { Mode } from \"./a.js\";\n",
        ),
        Source::new(
            "b.js",
            "// @flow\nimport type { Mode } from \"./index.js\";\nconst mode: Mode = \"on\";\n",
        ),
    ]);

    assert_clean(&report);
}

#[test]
fn a_star_re_export_carries_values_across_two_files() {
    require_checker!();

    let report = batch(&[
        Source::new("a.js", "// @flow\nexport const label: string = \"hi\";\n"),
        Source::new("index.js", "// @flow\nexport * from \"./a.js\";\n"),
        Source::new(
            "b.js",
            "// @flow\nimport { label } from \"./index.js\";\nconst n: number = label;\n",
        ),
    ]);

    assert_eq!(codes(&report), ["incompatible-type"]);
}

#[test]
fn two_files_that_import_each_other_check_instead_of_hanging() {
    require_checker!();

    // Each module names the other's type without being defined in terms of it,
    // which is the ordinary shape of a package's internal cycle. It has to
    // check, and it has to terminate.
    let report = batch(&[
        Source::new(
            "a.js",
            "// @flow\nimport type { B } from \"./b.js\";\n\
             export type A = { b: ?B, tag: \"a\" };\n\
             export function mkA(): A {\n  return { b: null, tag: \"a\" };\n}\n",
        ),
        Source::new(
            "b.js",
            "// @flow\nimport type { A } from \"./a.js\";\n\
             export type B = { a: ?A, tag: \"b\" };\n",
        ),
        Source::new(
            "use.js",
            "// @flow\nimport { mkA } from \"./a.js\";\nconst tag: string = mkA().tag;\n",
        ),
    ]);

    assert_clean(&report);
}

#[test]
fn a_file_that_imports_itself_terminates() {
    require_checker!();

    let report = batch(&[Source::new(
        "a.js",
        "// @flow\nimport type { T } from \"./a.js\";\nexport type T = number;\nconst n: T = 1;\n",
    )]);

    assert_eq!(report.files_checked, 1);
}

/// A known hole, written down so it is noticed when it closes.
///
/// `A` is defined *as* `B` and `B` as `A`, through an import. Within one file
/// Flow reports `recursive-definition` for exactly this shape. Across files the
/// two signature tvars unify with each other, nothing concrete flows in, and
/// the alias silently becomes `any` — so a wrong program is accepted. What this
/// test pins down is that it terminates: it neither hangs nor overflows the
/// check thread's stack, which is the part that would take the whole run with
/// it. If a later change makes Flow report the recursion here too, this test
/// fails and should be rewritten to assert the error.
#[test]
fn a_type_defined_as_itself_across_files_resolves_to_any_instead_of_erroring() {
    require_checker!();

    let report = batch(&[
        Source::new(
            "a.js",
            "// @flow\nimport type { B } from \"./b.js\";\n\
             export type A = B;\nconst a: A = 1;\nconst s: string = a;\n",
        ),
        Source::new(
            "b.js",
            "// @flow\nimport type { A } from \"./a.js\";\nexport type B = A;\n",
        ),
    ]);

    assert_eq!(report.files_checked, 2, "the batch has to finish");
    assert!(
        report.diagnostics.is_empty(),
        "today the alias is `any`, so `const s: string = a` passes; got {:?}",
        codes(&report)
    );
}

#[test]
fn a_specifier_written_without_its_extension_resolves() {
    require_checker!();

    let report = batch(&[
        Source::new("src/a.js", "// @flow\nexport type Mode = \"on\";\n"),
        Source::new(
            "src/b.js",
            "// @flow\nimport type { Mode } from \"./a\";\nconst mode: Mode = \"on\";\n",
        ),
    ]);

    assert_clean(&report);
    assert!(report.untyped_modules.is_empty());
}

#[test]
fn a_directory_specifier_resolves_to_its_index_file() {
    require_checker!();

    let report = batch(&[
        Source::new(
            "src/internal/index.js",
            "// @flow\nexport type Mode = \"on\";\n",
        ),
        Source::new(
            "src/b.js",
            "// @flow\nimport type { Mode } from \"./internal\";\nconst mode: Mode = \"on\";\n",
        ),
    ]);

    assert_clean(&report);
}

#[test]
fn a_bare_package_specifier_is_still_unchecked_and_still_recorded() {
    require_checker!();

    // The gap that remains, and the report that states it. A package name
    // resolves through `node_modules` or a workspace, neither of which a batch
    // of sources contains — and a file in the batch that happens to share the
    // name must not be mistaken for it.
    let report = batch(&[
        Source::new("some-package.js", "// @flow\nexport type Mode = \"on\";\n"),
        Source::new(
            "b.js",
            "// @flow\nimport { thing } from \"some-package\";\nconst n: number = thing;\n",
        ),
    ]);

    assert_clean(&report);
    assert_eq!(report.untyped_modules, ["some-package"]);
}

#[test]
fn a_builtin_module_still_resolves_to_its_declare_module_block() {
    require_checker!();

    let report = batch(&[Source::new(
        "app.js",
        "// @flow\nimport * as React from \"react\";\nconst n: number = React.useMemo;\n",
    )]);

    assert_eq!(
        codes(&report),
        ["incompatible-type"],
        "`react` is declared by Flow's own library definitions, so `useMemo` has a type"
    );
    assert!(
        report.untyped_modules.is_empty(),
        "a module Flow declares is not an untyped one: {:?}",
        report.untyped_modules
    );
}

#[test]
fn a_relative_specifier_that_names_nothing_in_the_batch_is_unchecked_and_recorded() {
    require_checker!();

    // Not an error: the batch is what `uf check` was handed, and a file outside
    // it — an asset, a generated module, a path the scan did not walk — exists
    // whether or not this run read it. Reporting it as missing would bury every
    // real error under one `cannot-resolve-module` per import.
    let report = batch(&[Source::new(
        "b.js",
        "// @flow\nimport { thing } from \"./missing.js\";\nconst n: number = thing;\n",
    )]);

    assert_clean(&report);
    assert_eq!(report.untyped_modules, ["./missing.js"]);
}

#[test]
fn a_specifier_that_climbs_out_of_the_project_is_unchecked_and_recorded() {
    require_checker!();

    let report = batch(&[Source::new(
        "b.js",
        "// @flow\nimport { thing } from \"../outside.js\";\nconst n: number = thing;\n",
    )]);

    assert_clean(&report);
    assert_eq!(report.untyped_modules, ["../outside.js"]);
}

#[test]
fn importing_a_file_that_opted_out_of_flow_is_unchecked_and_recorded() {
    require_checker!();

    // `@noflow` says the file is plain JavaScript. It has no types to take, so
    // the import is `any` — which is exactly what upstream's
    // `unchecked_module_t` does for a dependency with no typed parse.
    let report = batch(&[
        Source::new("plain.js", OPTED_OUT),
        Source::new(
            "b.js",
            "// @flow\nimport { n } from \"./plain.js\";\nconst m: number = n;\n",
        ),
    ]);

    assert_clean(&report);
    assert_eq!(report.untyped_modules, ["./plain.js"]);
    assert_eq!(report.files_skipped, 1);
}

#[test]
fn importing_a_file_that_does_not_parse_is_unchecked_and_reported_against_that_file() {
    require_checker!();

    let report = batch(&[
        Source::new("broken.js", "// @flow\nfunction ( {\n"),
        Source::new(
            "b.js",
            "// @flow\nimport { n } from \"./broken.js\";\nconst m: number = n;\n",
        ),
    ]);

    assert_eq!(report.untyped_modules, ["./broken.js"]);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.primary.path == "broken.js"),
        "the syntax error belongs to the file that has it: {:?}",
        report
            .diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.primary.path.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn one_hole_is_named_once_however_many_files_reach_for_it() {
    require_checker!();

    let report = batch(&[
        Source::new("a.js", "// @flow\nimport \"pkg\";\n"),
        Source::new("b.js", "// @flow\nimport \"pkg\";\n"),
        Source::new("c.js", "// @flow\nimport \"other\";\n"),
    ]);

    assert_eq!(report.untyped_modules, ["other", "pkg"]);
}

#[test]
fn a_module_imported_by_many_files_is_merged_once() {
    require_checker!();

    // The cache is what keeps a project whose files all import one module from
    // costing one merge per importer. Correctness first: every importer has to
    // agree about the type, whichever of them reached it first.
    let mut sources = vec![Source::new(
        "shared.js",
        "// @flow\nexport type Mode = \"on\";\nexport const label: string = \"hi\";\n",
    )];
    const IMPORTER: &str = "// @flow\nimport type { Mode } from \"./shared.js\";\n\
         import { label } from \"./shared.js\";\n\
         export const mode: Mode = \"on\";\nexport const text: string = label;\n";
    let paths: Vec<String> = (0..24).map(|n| format!("f{n}.js")).collect();
    sources.extend(paths.iter().map(|path| Source::new(path, IMPORTER)));

    let report = batch(&sources);

    assert_clean(&report);
    assert_eq!(report.files_checked, 25);
}

#[test]
fn checking_a_batch_twice_gives_the_same_answer() {
    require_checker!();

    let sources = [
        Source::new("a.js", "// @flow\nexport const label: string = \"hi\";\n"),
        Source::new(
            "b.js",
            "// @flow\nimport { label } from \"./a.js\";\nconst n: number = label;\n",
        ),
    ];

    let first = batch(&sources);
    let second = batch(&sources);

    assert_eq!(first.diagnostics, second.diagnostics);
    assert_eq!(first.untyped_modules, second.untyped_modules);
}
