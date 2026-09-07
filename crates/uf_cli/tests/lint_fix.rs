//! `uf lint --fix` and `uf check --fix`: what they write, what they refuse to
//! write, and what they exit with.
//!
//! The unit tests in `crates/uf_cli/src/fix` cover the catalogue and the edits.
//! These cover the promise the command makes to a person who runs it over a
//! directory: the file on disk afterwards, the count on screen, and the status
//! a script branches on.

mod support;

use std::fs;

use support::{Project, uf};

/// No errors remain.
const SUCCESS: i32 = 0;
/// Errors remain.
const FOUND_A_PROBLEM: i32 = 1;

/// Two safe fixes on one line, and one finding with only an unsafe answer.
const FIXABLE: &str =
    "// @flow\n\nexport type Props = { a: bool, b: bool };\n\nexport let count: number = 0;\n";

/// One unsafe finding, in a file `uf fmt --check` accepts as it stands.
///
/// Deliberately without a `bool` in it: uf's printer spells the deprecated
/// alias `boolean` when it reprints, so a file containing one is a file
/// `uf fmt` already wants to change, and the tests about staying formatted
/// need a file it does not.
const FORMATTED_AND_UNSAFE: &str = "// @flow\n\nexport let count: number = 0;\n";

/// A fixable error beside a warning nothing can fix.
///
/// `flow/export-renamed-default` is a warning and deliberately has no fix, so
/// a run over this file fixes the error, leaves the warning, and has to exit
/// `0` anyway.
const ONE_ERROR_AND_ONE_WARNING: &str = "// @flow\n\nexport type Flag = bool;\nexport const advice: string = \"x\";\nexport { advice as default };\n";

/// Run `uf` inside `project`, returning its exit code and its output.
fn run(project: &Project, args: &[&str]) -> (i32, String) {
    let (code, stdout, stderr) = split_run(project, args);
    (code, stdout + &stderr)
}

/// The same, with the two streams kept apart, for the tests that parse stdout.
fn split_run(project: &Project, args: &[&str]) -> (i32, String, String) {
    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["--color", "never"])
        .args(args)
        .output()
        .expect("uf started");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn read(project: &Project, name: &str) -> String {
    fs::read_to_string(project.path().join(name)).expect("the file is still there")
}

/// The default is the `--check`-like one: it reports and writes nothing.
#[test]
fn lint_without_fix_leaves_every_file_exactly_as_it_found_it() {
    let project = Project::new(&[("app.js", FIXABLE)]);

    let (code, output) = run(&project, &["lint"]);

    assert_eq!(code, FOUND_A_PROBLEM, "{output}");
    assert_eq!(read(&project, "app.js"), FIXABLE);
    assert!(!output.contains("applied"), "{output}");
}

/// Two fixes on one line, applied in one pass, and counted out loud.
///
/// The overlapping case proper is a unit test, because the catalogue is too
/// narrow to produce one from a real file; this is the batch case that a real
/// file does produce, and it is the one that used to corrupt files in other
/// tools — two edits to the same line, applied against offsets computed before
/// the first of them moved everything after it.
#[test]
fn lint_fix_applies_every_safe_fix_and_says_how_many() {
    let project = Project::new(&[("app.js", FIXABLE)]);

    let (code, output) = run(&project, &["lint", "--fix"]);

    assert!(
        read(&project, "app.js").contains("{ a: boolean, b: boolean }"),
        "{}",
        read(&project, "app.js")
    );
    assert!(output.contains("applied 2 fixes in 1 file"), "{output}");
    assert!(output.contains("app.js"), "{output}");
    assert_eq!(
        code, FOUND_A_PROBLEM,
        "the unsafe finding is still an error\n{output}"
    );
}

/// The status describes what is left, not what was done.
#[test]
fn a_run_that_fixed_the_only_error_exits_zero_even_with_a_warning_left() {
    let project = Project::new(&[("app.js", ONE_ERROR_AND_ONE_WARNING)]);

    let (before, _) = run(&project, &["lint"]);
    assert_eq!(before, FOUND_A_PROBLEM, "the fixture starts with an error");

    let (code, output) = run(&project, &["lint", "--fix"]);

    assert_eq!(code, SUCCESS, "{output}");
    assert!(output.contains("flow/export-renamed-default"), "{output}");
    assert!(
        read(&project, "app.js").contains("Flag = boolean"),
        "{output}"
    );
}

/// Twice is once. The second run finds the fixpoint the first one left.
#[test]
fn lint_fix_is_idempotent() {
    let project = Project::new(&[("app.js", FIXABLE)]);

    run(&project, &["lint", "--fix-unsafe"]);
    let once = read(&project, "app.js");
    let (code, output) = run(&project, &["lint", "--fix-unsafe"]);

    assert_eq!(read(&project, "app.js"), once, "the second run rewrote it");
    assert_eq!(code, SUCCESS, "{output}");
    assert!(
        output.contains("no finding here had a fix to apply"),
        "{output}"
    );
}

/// The unsafe tier is not applied, and the report says the flag that would.
#[test]
fn an_unsafe_fix_is_left_alone_until_it_is_asked_for_by_name() {
    let project = Project::new(&[("app.js", FIXABLE)]);

    let (_, output) = run(&project, &["lint", "--fix"]);

    assert!(
        read(&project, "app.js").contains("export let count"),
        "`--fix` wrote an unsafe fix"
    );
    assert!(
        output.contains("1 finding would be fixed by `--fix-unsafe`"),
        "{output}"
    );

    let (code, output) = run(&project, &["lint", "--fix-unsafe"]);
    assert_eq!(code, SUCCESS, "{output}");
    assert!(
        read(&project, "app.js").contains("export const count"),
        "`--fix-unsafe` did not write the unsafe fix"
    );
}

/// The help has to say what the word means, because the flag is the only place
/// a reader meets it.
#[test]
fn the_unsafe_flag_says_what_unsafe_means() {
    let output = uf().args(["lint", "--help"]).output().expect("uf started");
    let help = String::from_utf8_lossy(&output.stdout).into_owned();

    assert!(help.contains("--fix-unsafe"), "{help}");
    assert!(help.contains("can change what the program does"), "{help}");
    assert!(
        help.contains("export const"),
        "the help should show the fix it means\n{help}"
    );
}

/// What is written parses, and `uf fmt` is content with it.
#[test]
fn a_fixed_file_still_parses_and_still_passes_the_format_check() {
    let project = Project::new(&[("app.js", FORMATTED_AND_UNSAFE)]);

    let (before, output) = run(&project, &["fmt", "--check"]);
    assert_eq!(before, SUCCESS, "the fixture starts formatted\n{output}");

    let (code, output) = run(&project, &["lint", "--fix-unsafe"]);
    assert_eq!(code, SUCCESS, "{output}");
    assert!(read(&project, "app.js").contains("export const count"));

    let (code, output) = run(&project, &["fmt", "--check"]);
    assert_eq!(
        code, SUCCESS,
        "the fix left a file uf fmt rejects\n{output}"
    );
}

/// A fix that pushes a line past the print width is still applied, and the
/// file it leaves behind is the formatter's own output rather than a line the
/// formatter would have broken.
#[test]
fn a_fix_that_disturbs_the_layout_comes_back_formatted() {
    // Ninety-nine columns, which the printer leaves on one line; `const` is
    // two wider than `let`, which takes it past the hundred it allows.
    const WIDE: &str = "// @flow\n\nexport let settings = { alpha: 1, bravo: 2, charlie: 3, delta: 4, echo: 5, foxtrot: 6, golfoo: 7 };\n";
    let project = Project::new(&[("wide.js", WIDE)]);

    let (before, output) = run(&project, &["fmt", "--check"]);
    assert_eq!(before, SUCCESS, "the fixture starts formatted\n{output}");

    let (code, output) = run(&project, &["lint", "--fix-unsafe"]);
    assert_eq!(code, SUCCESS, "{output}");

    let fixed = read(&project, "wide.js");
    assert!(fixed.contains("export const settings = {\n"), "{fixed}");
    let (code, output) = run(&project, &["fmt", "--check"]);
    assert_eq!(
        code, SUCCESS,
        "the fix left a line the formatter would have broken\n{output}"
    );
}

/// A path argument narrows what is written, not just what is reported.
#[test]
fn a_path_argument_narrows_what_gets_written() {
    let project = Project::new(&[("kept.js", FIXABLE), ("touched.js", FIXABLE)]);

    run(&project, &["lint", "--fix", "touched.js"]);

    assert_eq!(
        read(&project, "kept.js"),
        FIXABLE,
        "an unnamed file changed"
    );
    assert!(read(&project, "touched.js").contains("boolean"));
}

/// The machine-readable report carries the pass as well as the findings, and
/// the findings in it are the ones that survived.
#[test]
fn the_json_report_describes_the_pass_and_what_it_left() {
    let project = Project::new(&[("app.js", FIXABLE)]);

    let (_, stdout, _) = split_run(&project, &["lint", "--json", "--fix"]);
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("JSON on stdout");

    assert_eq!(report["fixed"]["applied"], 2);
    assert_eq!(report["fixed"]["needsUnsafeFix"], 1);
    assert_eq!(report["fixed"]["files"][0], "app.js");
    assert_eq!(
        report["errors"], 1,
        "the counts are from after the fixes: {report}"
    );
}

/// `uf lint` without `--fix` says nothing about a pass it did not make.
#[test]
fn the_json_report_has_no_fix_object_when_nothing_was_fixed() {
    let project = Project::new(&[("app.js", FIXABLE)]);

    let (_, stdout, _) = split_run(&project, &["lint", "--json"]);
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("JSON on stdout");

    assert!(report.get("fixed").is_none(), "{report}");
}

/// The two flags are the same flags on the other command.
#[test]
fn check_fixes_the_same_findings_lint_does() {
    let project = Project::new(&[("app.js", FIXABLE)]);

    let (_, output) = run(&project, &["check", "--fix"]);

    assert!(output.contains("applied 2 fixes in 1 file"), "{output}");
    assert!(read(&project, "app.js").contains("boolean"));
}

/// Asking for both tiers at once is a contradiction the parser refuses, rather
/// than a precedence rule nobody would remember.
#[test]
fn the_two_fix_flags_cannot_be_combined() {
    let output = uf()
        .args(["lint", "--fix", "--fix-unsafe"])
        .output()
        .expect("uf started");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot be used with"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
