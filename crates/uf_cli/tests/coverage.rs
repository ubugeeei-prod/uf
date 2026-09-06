//! `uf test --coverage` end to end: a real transform, a real host, real counts.
//!
//! The fixture is chosen so that a report built on the *output* cannot pass.
//! `src/badge.js` declares an enum, and the enum lowering prepends seventeen
//! lines of runtime to the module: the author's `pick` is on line 10 and the
//! `function pick` that runs is on line 19 of the JavaScript. Anything that
//! counted the JavaScript would report line numbers past the end of a file that
//! has nineteen lines in it, and would report the runtime's helpers as
//! functions the author left untested.
//!
//! `crates/uf_test/src/tests/coverage.rs` covers the arithmetic from data.
//! What is only observable here is whether the transform really produces the
//! map that arithmetic assumes.

mod support;

use std::collections::BTreeMap;
use std::path::Path;

use support::{Project, host_ready, uf};

/// Nineteen lines, and every interesting one numbered in a comment.
const BADGE: &str = r#"// @flow

export type Label = string;

export enum Size {
  Small,
  Large,
}

export function pick(size: Size): Label {
  if (size === Size.Large) {
    return "large";
  }
  return "small";
}

export function never(): Label {
  return "unreached";
}
"#;

/// How many lines `BADGE` has. Nothing in a coverage report about it may name
/// a line past this one.
const BADGE_LINES: u32 = 20;

const BADGE_TEST: &str = r#"// @flow
import { expect, it } from "@uniflowed/test";

import { Size, pick } from "./badge.js";

it("picks small", () => {
  expect(pick(Size.Small)).toBe("small");
});
"#;

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
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The `DA:` records of one file in an LCOV document, as line to hits.
fn line_hits(lcov: &str, file: &str) -> BTreeMap<u32, u64> {
    record(lcov, file)
        .filter_map(|line| line.strip_prefix("DA:"))
        .filter_map(|entry| {
            let (line, hits) = entry.split_once(',')?;
            Some((line.parse().ok()?, hits.parse().ok()?))
        })
        .collect()
}

/// The `FN:`/`FNDA:` records of one file, as name to (line, hits).
fn functions(lcov: &str, file: &str) -> BTreeMap<String, (u32, u64)> {
    let mut declared: BTreeMap<String, u32> = BTreeMap::new();
    let mut hits: BTreeMap<String, u64> = BTreeMap::new();
    for line in record(lcov, file) {
        if let Some(entry) = line.strip_prefix("FN:")
            && let Some((at, name)) = entry.split_once(',')
            && let Ok(at) = at.parse()
        {
            declared.insert(name.to_owned(), at);
        }
        if let Some(entry) = line.strip_prefix("FNDA:")
            && let Some((count, name)) = entry.split_once(',')
            && let Ok(count) = count.parse()
        {
            hits.insert(name.to_owned(), count);
        }
    }
    declared
        .into_iter()
        .map(|(name, at)| {
            let count = hits.get(&name).copied().unwrap_or(0);
            (name, (at, count))
        })
        .collect()
}

/// The lines of one file's record.
fn record<'a>(lcov: &'a str, file: &str) -> impl Iterator<Item = &'a str> {
    let start = lcov
        .find(&format!("SF:{file}\n"))
        .unwrap_or_else(|| panic!("no record for {file} in:\n{lcov}"));
    lcov[start..]
        .lines()
        .take_while(|line| *line != "end_of_record")
}

/// A project holding the badge fixture and one test that exercises half of it.
fn badge_project() -> Project {
    Project::new(&[("src/badge.js", BADGE), ("src/badge.test.js", BADGE_TEST)])
}

#[test]
fn coverage_is_reported_against_the_flow_source_and_not_the_transform_output() {
    if !host_ready() {
        return;
    }
    let project = badge_project();
    let (passed, _, stderr) = run(
        project.path(),
        &["--coverage", "--coverage-reporter", "lcov"],
    );
    assert!(passed, "{stderr}");

    let lcov = std::fs::read_to_string(project.path().join("coverage/lcov.info")).unwrap();
    let hits = line_hits(&lcov, "src/badge.js");

    // The whole test, in one assertion: the enum lowering makes the JavaScript
    // twenty-seven lines long, so a report built on the output would name lines
    // this file does not have.
    let highest = hits.keys().copied().max().expect("some lines");
    assert!(
        highest <= BADGE_LINES,
        "line {highest} is past the end of a {BADGE_LINES}-line file:\n{lcov}"
    );

    // `pick` was called with `Size.Small`, so its `return \"small\"` on line 14
    // ran and its `return \"large\"` on line 12 did not.
    assert!(hits[&14] > 0, "line 14 ran:\n{lcov}");
    assert_eq!(hits[&12], 0, "line 12 is the branch nothing took:\n{lcov}");
    // `never` was never called.
    assert_eq!(
        hits[&18], 0,
        "line 18 is in a function nobody called:\n{lcov}"
    );

    // Lines that generate nothing are absent rather than missed: the pragma,
    // the blank lines, and the `type` alias the transform erases.
    for absent in [1, 2, 3, 9] {
        assert!(
            !hits.contains_key(&absent),
            "line {absent} generates nothing and must not be counted:\n{lcov}"
        );
    }
}

#[test]
fn the_enum_runtime_is_not_reported_as_the_authors_functions() {
    if !host_ready() {
        return;
    }
    let project = badge_project();
    let (passed, _, stderr) = run(
        project.path(),
        &["--coverage", "--coverage-reporter", "lcov"],
    );
    assert!(passed, "{stderr}");

    let lcov = std::fs::read_to_string(project.path().join("coverage/lcov.info")).unwrap();
    let functions = functions(&lcov, "src/badge.js");

    // Two functions, at the two lines that declare one. The enum lowering adds
    // `$$ufEnum`, `$$ufEnumMirrored` and four arrow functions inside them; none
    // of the six is the author's, so none of them is in the denominator —
    // reporting them would make this file 25% covered by functions when the
    // author wrote two and tested one.
    let names: Vec<&str> = functions.keys().map(String::as_str).collect();
    assert_eq!(names, vec!["never", "pick"], "in:\n{lcov}");
    assert_eq!(functions["pick"], (10, 1));
    assert_eq!(functions["never"].0, 17);
    assert_eq!(functions["never"].1, 0);
}

#[test]
fn a_module_two_workers_both_loaded_is_counted_once_and_summed() {
    if !host_ready() {
        return;
    }
    // Two test files, each in its own worker, both importing the same module.
    // A merge that double-counted would report it twice; one that lost a
    // worker would report half its counts. Both are visible in the hit count
    // of the line each test drives.
    let project = Project::new(&[
        ("src/badge.js", BADGE),
        ("src/small.test.js", BADGE_TEST),
        (
            "src/large.test.js",
            r#"// @flow
import { expect, it } from "@uniflowed/test";

import { Size, pick } from "./badge.js";

it("picks large", () => {
  expect(pick(Size.Large)).toBe("large");
});
"#,
        ),
    ]);
    let (passed, _, stderr) = run(
        project.path(),
        &["--coverage", "--coverage-reporter", "lcov", "-j", "2"],
    );
    assert!(passed, "{stderr}");

    let lcov = std::fs::read_to_string(project.path().join("coverage/lcov.info")).unwrap();
    assert_eq!(
        lcov.matches("SF:src/badge.js\n").count(),
        1,
        "one record per file, whatever the fan-out:\n{lcov}"
    );

    let hits = line_hits(&lcov, "src/badge.js");
    // One worker took each arm, so both are covered now — which is the merge
    // adding rather than either half winning.
    assert!(hits[&12] > 0, "the large arm ran in one worker:\n{lcov}");
    assert!(hits[&14] > 0, "the small arm ran in the other:\n{lcov}");
    // The `if` on line 11 was reached by both.
    assert!(
        hits[&11] >= 2,
        "line 11 ran once per worker, and both counts survived:\n{lcov}"
    );
}

#[test]
fn a_threshold_the_project_set_fails_the_run() {
    if !host_ready() {
        return;
    }
    let project = badge_project();
    project.write(
        "uf.config.js",
        r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  test: { coverage: { thresholds: { functions: 100 } } },
});
"#,
    );

    let (passed, _, stderr) = run(project.path(), &["--coverage"]);
    assert!(
        !passed,
        "a coverage threshold that is not reached must fail the run:\n{stderr}"
    );
    assert!(
        stderr.contains("functions coverage is 50.00% (1/2), below the required 100%"),
        "the failure has to say which number and how far below:\n{stderr}"
    );
}

#[test]
fn a_threshold_the_project_reached_leaves_the_run_green() {
    if !host_ready() {
        return;
    }
    let project = badge_project();
    project.write(
        "uf.config.js",
        r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  test: { coverage: { enabled: true, thresholds: { lines: 50, functions: 50 } } },
});
"#,
    );

    // `enabled` rather than `--coverage`: the gate has to hold for a run that
    // never asked for coverage on the command line, or CI and a laptop are
    // checking different things.
    let (passed, stdout, stderr) = run(project.path(), &[]);
    assert!(passed, "{stderr}");
    assert!(
        stdout.contains("coverage"),
        "an enabled run reports its coverage:\n{stdout}"
    );
}

#[test]
fn the_json_document_carries_the_same_numbers_as_the_table() {
    if !host_ready() {
        return;
    }
    let project = badge_project();
    let (_, stdout, _) = run(project.path(), &["--json", "--coverage"]);
    let document: serde_json::Value =
        serde_json::from_str(&stdout).expect("--json output must parse");

    let coverage = &document["coverage"];
    assert_eq!(coverage["functions"]["covered"], 1);
    assert_eq!(coverage["functions"]["total"], 2);
    assert_eq!(coverage["functions"]["percent"], "50.00");
    assert_eq!(coverage["files"][0]["file"], "src/badge.js");
}

#[test]
fn a_run_that_did_not_measure_has_no_coverage_object_at_all() {
    if !host_ready() {
        return;
    }
    let project = badge_project();
    let (_, stdout, _) = run(project.path(), &["--json"]);
    let document: serde_json::Value =
        serde_json::from_str(&stdout).expect("--json output must parse");

    // Absent, not zeroed: a check reading this has to be able to tell "not
    // measured" from "measured, and nothing is covered".
    assert!(document.get("coverage").is_none(), "{stdout}");
}

#[test]
fn junit_names_every_case_and_the_file_it_came_from() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/mixed.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("passes", () => {
  expect(1).toBe(1);
});

it("fails", () => {
  expect(1).toBe(2);
});

it.skip("is off", () => {});
it.todo("is unwritten");
"#,
    )]);

    let (passed, _, _) = run(
        project.path(),
        &[
            "--reporter",
            "junit",
            "--reporter-outfile",
            "reports/junit.xml",
        ],
    );
    assert!(!passed, "the suite has a failing test");

    let xml = std::fs::read_to_string(project.path().join("reports/junit.xml")).unwrap();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites "));
    assert!(xml.contains("tests=\"4\" failures=\"1\" errors=\"0\" skipped=\"2\""));
    assert!(xml.contains("<testsuite name=\"src/mixed.test.js\""));
    assert!(xml.contains("<testcase name=\"passes\" classname=\"src/mixed.test.js\""));
    assert!(xml.contains("<failure message=\"expected 1 to be 2\" type=\"AssertionError\">"));
    assert!(xml.contains("<skipped message=\"todo\"/>"));
}

#[test]
fn junit_without_an_outfile_is_refused_rather_than_written_to_stdout() {
    let project = badge_project();
    let (passed, _, stderr) = run(project.path(), &["--reporter", "junit"]);
    assert!(!passed);
    assert!(
        stderr.contains("--reporter-outfile"),
        "the refusal names the missing flag:\n{stderr}"
    );
}

#[test]
fn coverage_and_watch_are_refused_together() {
    let project = badge_project();
    let (passed, _, stderr) = run(project.path(), &["--watch", "--coverage"]);
    assert!(!passed);
    assert!(
        stderr.contains("--watch and --coverage cannot be combined"),
        "{stderr}"
    );
}

#[test]
fn cobertura_carries_the_same_file_and_the_same_lines() {
    if !host_ready() {
        return;
    }
    let project = badge_project();
    let (passed, _, stderr) = run(
        project.path(),
        &[
            "--coverage",
            "--coverage-reporter",
            "cobertura",
            "--coverage-dir",
            "reports/cov",
        ],
    );
    assert!(passed, "{stderr}");

    let xml =
        std::fs::read_to_string(project.path().join("reports/cov/cobertura-coverage.xml")).unwrap();
    assert!(xml.contains("filename=\"src/badge.js\""));
    assert!(xml.contains("<line number=\"12\" hits=\"0\""));
    assert!(xml.contains("timestamp=\"0\""));
    // `--coverage-dir` moved it, so the default is not also written.
    assert!(!project.path().join("coverage").exists());
}
