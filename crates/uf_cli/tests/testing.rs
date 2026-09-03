//! `uf test` end to end: real files, a real host, real assertions.
//!
//! Every test here runs `uf test` over a project written into this
//! repository's workspace, which imports `@uniflowed/test` and executes on
//! Node exactly as a user's project would. Nothing is stubbed, because the
//! runner's whole job is to drive a JavaScript host and a stubbed host would
//! only prove the stub works.
//!
//! They skip — loudly — where Node or the installed workspace is missing, so a
//! checkout that never ran `npm ci` still passes `cargo test`.

mod support;

use std::path::Path;

use support::{Project, assert_plain, host_ready, uf};

/// A suite with one of every outcome, so one project exercises the whole
/// reporting surface.
const MIXED: [(&str, &str); 3] = [
    (
        "src/shared.js",
        "// @flow\nexport const shared = (value: string): string => value;\n",
    ),
    (
        "src/math.test.js",
        r#"// @flow
import { describe, expect, it } from "@uniflowed/test";
import { shared } from "./shared.js";

describe("math", () => {
  it("adds", () => {
    expect(1 + 1).toBe(2);
    expect(shared("x")).toBe("x");
  });

  it("fails", () => {
    expect("flow").toBe("typescript");
  });

  it.skip("off", () => {});
  it.todo("later");
});
"#,
    ),
    (
        "src/ui/button.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"renders\", () => {\n  expect(1).toBe(1);\n});\n",
    ),
];

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
fn a_test_body_actually_runs() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/effects.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

let ran = false;

it("executes its body", () => {
  ran = true;
  expect(ran).toBe(true);
});

it("sees what the previous test did", () => {
  expect(ran).toBe(true);
});
"#,
    )]);

    let document = json(project.path(), &[]);

    assert_eq!(document["passed"], 2);
    assert_eq!(document["failed"], 0);
}

#[test]
fn json_counts_every_outcome() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let document = json(project.path(), &[]);

    assert_eq!(document["passed"], 2);
    assert_eq!(document["failed"], 1);
    assert_eq!(document["skipped"], 1);
    assert_eq!(document["todo"], 1);
    assert_eq!(document["files"], 2, "src/shared.js declares no tests");
    assert_eq!(document["success"], false);
}

#[test]
fn json_is_pure_json_even_with_color_forced_on() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let output = uf()
        .env("FORCE_COLOR", "3")
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    assert_plain(&stdout);
    serde_json::from_str::<serde_json::Value>(&stdout).expect("still parses");
}

#[test]
fn json_is_byte_identical_across_runs_modulo_timings() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    // The first run records the timings the next one schedules from, so it is
    // the one run that legitimately differs. Compare the two after that.
    run(project.path(), &["--json"]);
    let (_, first, _) = run(project.path(), &["--json"]);
    let (_, second, _) = run(project.path(), &["--json"]);

    similar_asserts::assert_eq!(without_timings(&first), without_timings(&second));
}

#[test]
fn a_serial_run_and_a_parallel_run_produce_the_same_results() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    run(project.path(), &["--json"]);
    let (_, serial, _) = run(project.path(), &["--json", "-j", "1"]);
    let (_, parallel, _) = run(project.path(), &["--json", "-j", "8"]);

    // Everything but how long each case took: a run on one worker and a run on
    // eight must agree about what happened, and cannot agree about timing.
    similar_asserts::assert_eq!(without_timings(&serial), without_timings(&parallel));
}

#[test]
fn a_failure_is_shown_as_a_code_frame_at_the_assertion() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let (success, stdout, _) = run(project.path(), &[]);

    assert!(!success);
    assert_plain(&stdout);
    assert!(
        stdout.contains("expected \"flow\" to be \"typescript\""),
        "{stdout}"
    );
    // The frame points at the assertion in the *Flow* source, which only works
    // because the transform's source map reaches the worker's stack traces.
    assert!(stdout.contains("src/math.test.js:12"), "{stdout}");
    assert!(
        stdout.contains("expect(\"flow\").toBe(\"typescript\")"),
        "{stdout}"
    );
    assert!(stdout.contains("expected"), "{stdout}");
    assert!(stdout.contains("received"), "{stdout}");
}

#[test]
fn hooks_run_in_the_documented_order() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/hooks.test.js",
        r#"// @flow
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it } from "@uniflowed/test";

const order: Array<string> = [];

beforeEach(() => { order.push("outer-before"); });
afterEach(() => { order.push("outer-after"); });

describe("inner", () => {
  beforeAll(() => { order.push("all"); });
  beforeEach(() => { order.push("inner-before"); });
  afterEach(() => { order.push("inner-after"); });
  afterAll(() => { order.push("after-all"); });

  it("first", () => {
    expect(order.join(",")).toBe("all,outer-before,inner-before");
  });

  it("second", () => {
    expect(order.join(",")).toBe(
      "all,outer-before,inner-before,inner-after,outer-after,outer-before,inner-before",
    );
  });
});
"#,
    )]);

    let document = json(project.path(), &[]);

    assert_eq!(document["passed"], 2, "{document}");
}

#[test]
fn a_hook_that_throws_fails_the_tests_it_was_setting_up() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/broken-hook.test.js",
        r#"// @flow
import { beforeAll, describe, expect, it } from "@uniflowed/test";

describe("suite", () => {
  beforeAll(() => {
    throw new Error("set-up failed");
  });

  it("never gets to run", () => {
    expect(1).toBe(1);
  });
});
"#,
    )]);

    let (success, stdout, _) = run(project.path(), &[]);

    assert!(!success);
    assert!(stdout.contains("set-up failed"), "{stdout}");
}

#[test]
fn a_module_that_throws_while_loading_is_reported_as_the_file_failing() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/broken.test.js",
        "// @flow\nimport { it } from \"@uniflowed/test\";\n\nthrow new Error(\"boom at import\");\n\nit(\"never registers\", () => {});\n",
    )]);

    let document = json(project.path(), &[]);

    assert_eq!(document["failedFiles"], 1, "{document}");
    assert_eq!(document["passed"], 0);
    let file = &document["fileReports"][0];
    assert_eq!(file["status"], "load-failed");
    assert!(
        file["reason"].as_str().unwrap().contains("boom at import"),
        "{file}"
    );
}

#[test]
fn a_syntax_error_names_the_file_rather_than_taking_down_the_run() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[
        (
            "src/bad.test.js",
            "// @flow\nimport { it } from \"@uniflowed/test\";\nit('a', () => { const = ; });\n",
        ),
        (
            "src/good.test.js",
            "// @flow\nimport { expect, it } from \"@uniflowed/test\";\nit('b', () => { expect(1).toBe(1); });\n",
        ),
    ]);

    let document = json(project.path(), &[]);

    assert_eq!(document["passed"], 1, "the good file still ran: {document}");
    assert_eq!(document["failedFiles"], 1);
}

#[test]
fn a_test_that_never_settles_is_failed_rather_than_hanging_the_run() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/hangs.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("waits forever", async () => {
  await new Promise(() => {});
}, { timeout: 250 });

it("still runs after it", () => {
  expect(1).toBe(1);
});
"#,
    )]);

    let document = json(project.path(), &[]);

    assert_eq!(document["failed"], 1, "{document}");
    assert_eq!(document["passed"], 1, "the next test still ran: {document}");
    let failing = document["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|test| test["status"] == "failed")
        .unwrap();
    assert!(
        failing["failures"][0]["message"]
            .as_str()
            .unwrap()
            .contains("timed out"),
        "{failing}"
    );
}

#[test]
fn a_name_filter_keeps_only_matching_tests() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let document = json(project.path(), &["-t", "adds"]);

    assert_eq!(document["passed"], 1);
    let filtered: Vec<&str> = document["tests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|test| test["status"] == "filtered")
        .map(|test| test["name"].as_str().unwrap())
        .collect();
    assert!(filtered.contains(&"math > fails"), "{filtered:?}");
}

#[test]
fn a_filter_does_not_override_an_explicit_skip_or_a_todo() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let document = json(project.path(), &["-t", "no such test"]);

    let status_of = |name: &str| {
        document["tests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|test| test["name"] == name)
            .map(|test| test["status"].as_str().unwrap().to_string())
            .unwrap_or_default()
    };
    assert_eq!(status_of("math > off"), "skipped");
    assert_eq!(status_of("math > later"), "todo");
    assert_eq!(status_of("math > adds"), "filtered");
}

#[test]
fn a_path_filter_keeps_only_matching_files() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let document = json(project.path(), &["src/ui/"]);

    assert_eq!(document["files"], 1);
    assert_eq!(document["fileReports"][0]["file"], "src/ui/button.test.js");
}

#[test]
fn only_restricts_the_file_it_appears_in() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[
        (
            "src/focused.test.js",
            r#"// @flow
import { expect, it } from "@uniflowed/test";

it("ignored", () => { expect(1).toBe(2); });
it.only("focused", () => { expect(1).toBe(1); });
"#,
        ),
        (
            "src/other.test.js",
            "// @flow\nimport { expect, it } from \"@uniflowed/test\";\nit(\"elsewhere\", () => { expect(1).toBe(1); });\n",
        ),
    ]);

    let document = json(project.path(), &[]);

    assert_eq!(
        document["passed"], 2,
        "the other file is unaffected: {document}"
    );
    assert_eq!(document["failed"], 0);
    let ignored = document["tests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|test| test["name"] == "ignored")
        .unwrap();
    assert_eq!(ignored["status"], "not-only");
}

#[test]
fn bail_stops_the_run_and_says_so() {
    if !host_ready() {
        return;
    }
    let files: Vec<(String, String)> = (0..12)
        .map(|index| {
            (
                format!("src/f{index}.test.js"),
                format!(
                    "// @flow\nimport {{ expect, it }} from \"@uniflowed/test\";\nit('case {index}', () => {{ expect(1).toBe(2); }});\n"
                ),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect();
    let project = Project::new(&borrowed);

    let document = json(project.path(), &["--bail"]);

    assert_eq!(document["bailed"], true, "{document}");
    let not_run = document["fileReports"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|file| file["status"] == "not-run")
        .count();
    assert!(
        not_run > 0,
        "bailing must leave files unscheduled: {document}"
    );
}

#[test]
fn retry_re_runs_a_failing_test_and_reports_the_attempts() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/flaky.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\nit(\"always fails\", () => { expect(1).toBe(2); });\n",
    )]);

    let document = json(project.path(), &["--retry", "2"]);

    assert_eq!(document["tests"][0]["attempts"], 3, "{document}");
    assert_eq!(document["failed"], 1);
}

#[test]
fn retry_does_not_re_run_a_passing_test() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/steady.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\nit(\"passes\", () => { expect(1).toBe(1); });\n",
    )]);

    let document = json(project.path(), &["--retry", "3"]);

    assert_eq!(document["tests"][0]["attempts"], 1);
}

#[test]
fn a_run_records_its_timings_for_the_next_one() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let cold = json(project.path(), &[]);
    assert_eq!(cold["scheduledWarm"], 0);

    let warm = json(project.path(), &[]);
    assert_eq!(warm["scheduledWarm"], warm["files"]);
    assert_eq!(warm["scheduledCold"], 0);
}

#[test]
fn a_corrupt_timings_file_schedules_cold_instead_of_failing() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);
    project.write(".uf/test-timings.json", "{ not json at all");

    let document = json(project.path(), &[]);

    assert_eq!(document["scheduledCold"], document["files"]);
    assert_eq!(document["passed"], 2);
}

#[test]
fn async_tests_and_promise_matchers_work() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/async.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it("resolves", async () => {
  await expect(Promise.resolve(3)).resolves.toBe(3);
});

it("rejects", async () => {
  await expect(Promise.reject(new Error("nope"))).rejects.toThrow("nope");
});

it("awaits real work", async () => {
  const value = await new Promise((resolve) => setTimeout(() => resolve(7), 5));
  expect(value).toBe(7);
});
"#,
    )]);

    let document = json(project.path(), &[]);

    assert_eq!(document["passed"], 3, "{document}");
}

#[test]
fn modern_flow_syntax_runs_in_a_test() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/flow.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

enum Colour { Red, Green }

hook useDouble(value: number): number {
  return value * 2;
}

it("runs component, hook, match and enum syntax", () => {
  const name = match (Colour.Red) {
    Colour.Red => "red",
    Colour.Green => "green",
  };
  expect(name).toBe("red");
  expect(useDouble(21)).toBe(42);
  expect(Colour.cast("Red")).toBe(Colour.Red);
});
"#,
    )]);

    let document = json(project.path(), &[]);

    assert_eq!(document["passed"], 1, "{document}");
}

#[test]
fn the_summary_names_the_slowest_files_and_the_host() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let (_, stdout, _) = run(project.path(), &[]);

    assert!(stdout.contains("slowest files"), "{stdout}");
    assert!(stdout.contains("host"), "{stdout}");
    assert!(
        stdout.contains("node") || stdout.contains("bun"),
        "{stdout}"
    );
}

#[test]
fn list_reports_what_each_declaration_would_do() {
    let project = Project::new(&MIXED);

    let (success, stdout, _) = run(project.path(), &["--list"]);

    assert!(success, "{stdout}");
    assert!(stdout.contains("math > adds"), "{stdout}");
    assert!(stdout.contains("skip"), "{stdout}");
    assert!(stdout.contains("todo"), "{stdout}");
    assert_plain(&stdout);
}

#[test]
fn list_honours_the_name_filter() {
    let project = Project::new(&MIXED);

    let (_, stdout, _) = run(project.path(), &["--list", "-t", "adds"]);

    assert!(stdout.contains("adds"), "{stdout}");
    assert!(!stdout.contains("renders"), "{stdout}");
}

#[test]
fn watch_and_json_cannot_be_combined() {
    let project = Project::new(&MIXED);

    let (success, _, stderr) = run(project.path(), &["--watch", "--json"]);

    assert!(!success);
    assert!(stderr.contains("cannot be combined"), "{stderr}");
}

#[test]
fn a_project_with_no_tests_is_a_green_run() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[("src/lib.js", "// @flow\nexport const a: number = 1;\n")]);

    let (success, _, _) = run(project.path(), &[]);
    let document = json(project.path(), &[]);

    assert!(success);
    assert_eq!(document["files"], 0);
    assert_eq!(document["success"], true);
}
