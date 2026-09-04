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
