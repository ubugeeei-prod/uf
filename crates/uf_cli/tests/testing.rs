//! `uf test`: filtering, scheduling, control flags, and the shape of `--json`.

mod support;

use std::fs;
use std::path::Path;

use support::{assert_plain, uf};

fn write(dir: &Path, name: &str, body: &str) {
    let path = dir.join("src").join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

/// A suite with one of every outcome, so one project exercises the whole
/// reporting surface.
fn mixed_project(dir: &Path) {
    write(
        dir,
        "shared.js",
        "// @flow\nexport const shared = (v: string): string => v;\n",
    );
    write(dir, "lonely.js", "// @flow\nexport const lonely = 1;\n");
    write(
        dir,
        "math.test.js",
        r#"// @flow
import { describe, expect, it } from "@uniflowed/test";
import { shared } from "./shared.js";

describe("math", () => {
  it("adds", () => {
    expect(1 + 1).toBe(2);
  });

  it("fails", () => {
    expect("flow").toBe("typescript");
  });

  it.skip("off", () => {});
  it.todo("later");
});
"#,
    );
    write(
        dir,
        "ui/button.test.js",
        "// @flow\nit(\"renders\", () => { expect(1).toBe(1); });\n",
    );
}

fn run(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let output = uf()
        .arg("--cwd")
        .arg(dir)
        .arg("test")
        .args(args)
        .output()
        .unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

fn json(dir: &Path, args: &[&str]) -> serde_json::Value {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let (_, stdout, _) = run(dir, &all);
    serde_json::from_str(&stdout).expect("--json output must parse")
}

/// The document with every measured duration removed, which is what "identical
/// modulo timings" means.
fn without_timings(document: &str) -> String {
    document
        .lines()
        .filter(|line| !line.contains("durationMicros"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn test_json_is_pure_json_even_with_color_forced_on() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["test", "--json", "--color", "always"])
        .env("FORCE_COLOR", "3")
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_plain(&stdout);
    assert!(
        stdout.starts_with('{'),
        "a banner leaked into --json output"
    );
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("--json must parse");
    assert_eq!(value["command"], serde_json::json!("uf test"));
}

#[test]
fn test_json_counts_every_outcome() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let value = json(dir.path(), &[]);
    assert_eq!(value["passed"], serde_json::json!(2));
    assert_eq!(value["failed"], serde_json::json!(1));
    assert_eq!(value["skipped"], serde_json::json!(1));
    assert_eq!(value["todo"], serde_json::json!(1));
    assert_eq!(value["success"], serde_json::json!(false));
}

#[test]
fn test_json_is_byte_identical_across_runs_modulo_timings() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    // Warm the recorded timings first: the schedule provenance counters
    // describe the cache, not the suite, and only settle once it exists.
    let _ = run(dir.path(), &["--json"]);
    let (_, first, _) = run(dir.path(), &["--json"]);
    let (_, second, _) = run(dir.path(), &["--json"]);

    assert_eq!(without_timings(&first), without_timings(&second));
}

#[test]
fn a_cold_and_a_warm_schedule_produce_the_same_results() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let cold = json(dir.path(), &[]);
    let warm = json(dir.path(), &[]);

    assert_eq!(cold["tests"], warm["tests"]);
    assert_ne!(warm["scheduledWarm"], serde_json::json!(0));
    assert_eq!(cold["scheduledWarm"], serde_json::json!(0));
}

#[test]
fn a_serial_run_and_a_parallel_run_produce_the_same_results() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let serial = json(dir.path(), &["-j", "1"]);
    let parallel = json(dir.path(), &["-j", "8"]);

    assert_eq!(serial["tests"], parallel["tests"]);
    assert_eq!(serial["passed"], parallel["passed"]);
    assert_eq!(serial["failed"], parallel["failed"]);
}

#[test]
fn a_run_records_its_timings_for_the_next_one() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let _ = run(dir.path(), &[]);
    let recorded = dir.path().join(".uf").join("test-timings.json");
    assert!(recorded.exists(), "the run must record its own timings");

    let document = fs::read_to_string(&recorded).unwrap();
    assert!(document.contains("src/math.test.js"));
    assert!(document.contains("\"version\": 1"));
}

#[test]
fn a_corrupt_timings_file_schedules_cold_instead_of_failing() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());
    fs::create_dir_all(dir.path().join(".uf")).unwrap();
    fs::write(
        dir.path().join(".uf").join("test-timings.json"),
        "{ not json at all",
    )
    .unwrap();

    let (_, stdout, _) = run(dir.path(), &[]);
    assert!(stdout.contains("scheduling cold"), "{stdout}");
    assert!(stdout.contains("2 passed"), "{stdout}");
}

#[test]
fn a_hostile_timings_file_is_ignored_entry_by_entry() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());
    fs::create_dir_all(dir.path().join(".uf")).unwrap();
    fs::write(
        dir.path().join(".uf").join("test-timings.json"),
        r#"{"version": 1, "files": {"../../../../etc/passwd": 1, "/etc/shadow": -3}}"#,
    )
    .unwrap();

    let (_, stdout, _) = run(dir.path(), &[]);
    assert!(stdout.contains("unusable entr"), "{stdout}");
    assert!(stdout.contains("2 passed"), "{stdout}");
}

#[test]
fn a_name_filter_keeps_only_matching_tests() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let value = json(dir.path(), &["-t", "adds"]);
    assert_eq!(value["passed"], serde_json::json!(1));
    assert_eq!(value["failed"], serde_json::json!(0));

    let statuses: Vec<&str> = value["tests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|test| test["status"] == serde_json::json!("passed"))
        .map(|test| test["name"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, vec!["math > adds"]);
}

#[test]
fn a_path_filter_keeps_only_matching_files() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let value = json(dir.path(), &["src/ui"]);
    assert_eq!(value["files"], serde_json::json!(1));
    assert_eq!(value["passed"], serde_json::json!(1));
    assert_eq!(value["success"], serde_json::json!(true));
}

#[test]
fn bail_stops_the_run_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "a.test.js",
        "it('a', () => { expect(1).toBe(2); });\n",
    );
    write(
        dir.path(),
        "b.test.js",
        "it('b', () => { expect(1).toBe(2); });\n",
    );
    write(
        dir.path(),
        "c.test.js",
        "it('c', () => { expect(1).toBe(2); });\n",
    );

    let value = json(dir.path(), &["--bail", "-j", "1"]);
    assert_eq!(value["bailed"], serde_json::json!(true));
    let not_run = value["fileReports"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|file| file["status"] == serde_json::json!("not-run"))
        .count();
    assert!(not_run > 0, "bail must leave files unscheduled: {value}");
}

#[test]
fn bail_takes_an_explicit_threshold() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a", "b", "c", "d"] {
        write(
            dir.path(),
            &format!("{name}.test.js"),
            &format!("it('{name}', () => {{ expect(1).toBe(2); }});\n"),
        );
    }

    let value = json(dir.path(), &["--bail=2", "-j", "1"]);
    assert_eq!(value["bailed"], serde_json::json!(true));
    assert!(value["failed"].as_u64().unwrap() >= 2);
}

#[test]
fn retry_re_runs_a_failing_test_the_requested_number_of_times() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "a.test.js",
        "it('a', () => { expect(1).toBe(2); });\n",
    );

    let value = json(dir.path(), &["--retry", "2"]);
    let attempts = value["tests"][0]["attempts"].as_u64().unwrap();
    assert_eq!(attempts, 3, "one run plus two retries");
}

#[test]
fn retry_does_not_re_run_a_passing_test() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "a.test.js",
        "it('a', () => { expect(1).toBe(1); });\n",
    );

    let value = json(dir.path(), &["--retry", "5"]);
    assert_eq!(value["tests"][0]["attempts"], serde_json::json!(1));
}

#[test]
fn only_restricts_the_file_it_appears_in() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "a.test.js",
        "it('kept out', () => {});\nit.only('focused', () => { expect(1).toBe(1); });\n",
    );
    write(
        dir.path(),
        "b.test.js",
        "it('other file', () => { expect(1).toBe(1); });\n",
    );

    let value = json(dir.path(), &[]);
    assert_eq!(value["passed"], serde_json::json!(2));
    let not_only = value["tests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|test| test["status"] == serde_json::json!("not-only"))
        .count();
    assert_eq!(not_only, 1);
}

#[test]
fn an_unexpandable_declaration_is_reported_by_name_and_fails_the_run() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "a.test.js",
        "it.each([1, 2])('generated %i', () => {});\n",
    );

    let (success, stdout, stderr) = run(dir.path(), &[]);
    assert!(!success, "an unexpandable declaration must fail the run");
    assert!(stdout.contains("it.each"), "{stdout}");
    assert!(stdout.contains("unsupported declarations"), "{stdout}");
    assert!(stderr.contains("unsupported test declaration"), "{stderr}");
}

#[test]
fn an_unsupported_matcher_is_named_with_a_code_frame() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "a.test.js",
        "it('contains', () => {\n  expect([1, 2]).toContain(2);\n});\n",
    );

    let (success, stdout, _) = run(dir.path(), &[]);
    assert!(!success);
    assert!(stdout.contains("toContain"), "{stdout}");
    assert!(stdout.contains("src/a.test.js:2:3"), "{stdout}");
    assert!(stdout.contains("expect([1, 2]).toContain(2);"), "{stdout}");
}

#[test]
fn a_failure_is_shown_as_a_code_frame_at_the_assertion() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "a.test.js",
        "it('fails', () => {\n  expect(\"flow\").toBe(\"typescript\");\n});\n",
    );

    let (success, stdout, _) = run(dir.path(), &[]);
    assert!(!success);
    assert!(stdout.contains("src/a.test.js:2:3"), "{stdout}");
    assert!(stdout.contains("toBe assertion failed"), "{stdout}");
    assert!(stdout.contains('^'), "a caret row must be drawn:\n{stdout}");
}

#[test]
fn the_summary_names_the_slowest_files() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let (_, stdout, _) = run(dir.path(), &[]);
    assert!(stdout.contains("slowest files"), "{stdout}");
    assert!(stdout.contains("src/math.test.js"), "{stdout}");
}

#[test]
fn the_summary_reports_where_the_schedule_came_from() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let (_, stdout, _) = run(dir.path(), &[]);
    assert!(stdout.contains("schedule"), "{stdout}");
    assert!(stdout.contains("by size"), "{stdout}");
    assert!(stdout.contains(".uf/test-timings.json"), "{stdout}");
}

#[test]
fn list_reports_what_each_declaration_would_do() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let (success, stdout, _) = run(dir.path(), &["--list"]);
    assert!(success);
    assert!(stdout.contains("selection"), "{stdout}");
    assert!(stdout.contains("math > off"), "{stdout}");
    assert!(stdout.contains("todo"), "{stdout}");
    assert_plain(&stdout);
}

#[test]
fn list_honours_the_name_filter() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let (_, stdout, _) = run(dir.path(), &["--list", "-t", "adds"]);
    assert!(stdout.contains("math > adds"), "{stdout}");
    assert!(!stdout.contains("math > fails"), "{stdout}");
}

#[test]
fn watch_and_json_cannot_be_combined() {
    let dir = tempfile::tempdir().unwrap();
    mixed_project(dir.path());

    let (success, _, stderr) = run(dir.path(), &["--watch", "--json"]);
    assert!(!success);
    assert!(stderr.contains("--watch and --json"), "{stderr}");
}

#[test]
fn a_project_with_no_tests_is_a_green_run() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "index.js", "// @flow\nexport const a = 1;\n");

    let (success, stdout, _) = run(dir.path(), &[]);
    assert!(success, "{stdout}");
    assert!(stdout.contains("0 passed, 0 failed"), "{stdout}");
}
