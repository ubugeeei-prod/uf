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
//!
//! One test drives [`uf_test::Worker`] rather than the `uf` binary, because
//! what it asserts is a promise the worker protocol makes and the protocol is
//! only observable on the wire — see
//! `a_worker_that_answers_for_a_finished_file_does_not_disturb_the_running_one`.
//! It still needs a real host, which is why it lives here and not in `uf_test`.

mod support;

use std::path::Path;
use std::time::Duration;

use camino::Utf8PathBuf;
use uf_test::{FileStatus, HostCommand, HostKind, TestStatus, Worker};

use support::{Project, assert_plain, host_ready, uf, worker_command};

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

/// Two files on one worker, the second releasing a callback the first left
/// behind.
///
/// The issue writes the reproduction as `setTimeout(…, 50)` and lets the next
/// file happen to be running fifty milliseconds later. That reproduces the bug
/// and makes a poor test: what has to be shown is that the line arrived *after*
/// its file had been reported, and a sleep shows that only while the machine is
/// idle. So the second file hands the first one a switch, through a module
/// neither of them is: both import `switch.js` without the cache-busting query
/// the worker puts on a test file, so both get the one instance the worker's
/// registry holds. The callback is still detached — scheduled by a case that
/// returned long before it fires — but it cannot run until the second file has
/// started, and the second file cannot finish until it has.
///
/// The first file is deliberately the longer of the two, because the schedule
/// is longest-expected-first and a cold file's expectation is its size: this is
/// what puts it ahead of the second rather than the tie-break on path.
const STRAGGLER: [(&str, &str); 3] = [
    (
        "src/switch.js",
        r#"// @flow
let release: () => void = () => {};
export const begun: Promise<void> = new Promise((resolve) => {
  release = resolve;
});
export function begin(): void {
  release();
}

let settle: () => void = () => {};
export const printed: Promise<void> = new Promise((resolve) => {
  settle = resolve;
});
export function donePrinting(): void {
  settle();
}
"#,
    ),
    (
        "src/a-leaves-a-callback.test.js",
        r#"// @flow
// This file is padded to be the longer of the two, so that the schedule —
// longest expected first, and a cold file is expected to cost what its size
// suggests — runs it before the file that releases its callback. Without that
// the order would rest on the tie-break, which is alphabetical and would
// happen to agree; resting on a coincidence is not the same as being ordered.
import { expect, it } from "@uniflowed/test";
import { begun, donePrinting } from "./switch.js";

it("schedules something it does not wait for", () => {
  begun.then(() => {
    setTimeout(() => {
      console.log("from the previous file");
      donePrinting();
    }, 0);
  });
  expect(true).toBe(true);
});
"#,
    ),
    (
        "src/b-runs-after-it.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";
import { begin, printed } from "./switch.js";

it("is running when it fires", async () => {
  begin();
  await printed;
  expect(true).toBe(true);
});
"#,
    ),
];

/// One file's report out of the `--json` document.
fn file_report<'a>(document: &'a serde_json::Value, file: &str) -> &'a serde_json::Value {
    document["fileReports"]
        .as_array()
        .expect("the document lists its files")
        .iter()
        .find(|report| report["file"] == file)
        .unwrap_or_else(|| panic!("{file} is missing from {document}"))
}

#[test]
fn a_line_printed_after_its_file_finished_is_not_reported_under_the_next_one() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&STRAGGLER);

    // One worker, so the second file is served by the process the first one
    // left its callback running in. That is the only condition the bug needs.
    let document = json(project.path(), &["-j", "1"]);

    assert_eq!(document["passed"], 2, "{document}");
    assert_eq!(document["failed"], 0, "{document}");
    assert_eq!(document["files"], 2, "src/switch.js declares no tests");

    let printed_under_a_case_of_the_second = document["tests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|test| test["file"] == "src/b-runs-after-it.test.js")
        .any(|test| {
            serde_json::to_string(&test["output"])
                .unwrap()
                .contains("from the previous file")
        });
    assert!(
        !printed_under_a_case_of_the_second,
        "a line the previous file printed was reported under a case of this one: {document}"
    );

    // The runner's own notes and what the file printed are both `output`, so
    // they are told apart the way a reader tells them apart: by the prefix.
    let second = file_report(&document, "src/b-runs-after-it.test.js");
    let chunks = second["output"]
        .as_array()
        .expect("a file report lists its output");
    let (notes, printed): (Vec<_>, Vec<_>) = chunks.iter().partition(|chunk| {
        chunk["text"]
            .as_str()
            .is_some_and(|text| text.starts_with("[uf] "))
    });
    assert!(
        !printed
            .iter()
            .any(|chunk| chunk["text"] == "from the previous file\n"),
        "a line the previous file printed became this file's own: {document}"
    );

    // Dropped, but not silently: the note names the file the line came from,
    // which is where a reader chasing the message has to look. It cannot be
    // put in that file's report — that report was streamed to the terminal
    // when the file finished, before this line existed.
    let [note] = notes.as_slice() else {
        panic!("the drop is noted exactly once: {document}");
    };
    let text = note["text"].as_str().unwrap();
    assert!(text.contains("src/a-leaves-a-callback.test.js"), "{text}");
    assert!(text.contains("from the previous file"), "{text}");
    assert_eq!(note["stream"], "stderr", "{text}");

    // And the file it came from does not gain it either. Saying so here is the
    // point: the line is genuinely lost from the report, and the note is what
    // is offered in its place.
    let first = file_report(&document, "src/a-leaves-a-callback.test.js");
    assert!(
        !serde_json::to_string(first)
            .unwrap()
            .contains("from the previous file"),
        "{first}"
    );
}

/// A worker that answers from a script instead of running anything.
///
/// The three kinds of late event are not equally easy to provoke from a test
/// file. A stray `console.log` is; a case that reports after its file has ended
/// is not, and an abandoned promise ends the worker as it goes, so watching for
/// the report it produced is watching for a write that races a `process.exit`.
/// None of that is what is being tested. What is being tested is what `uf` does
/// with each kind, so each kind is a line in a script here — which is also the
/// only way to write "and the second file was reported correctly *anyway*" as
/// an assertion rather than a hope.
///
/// It speaks the same protocol as `packages/test/worker.js` and nothing else:
/// a request per line in, one event per line out, each event stamped with the
/// generation it belongs to.
const CANNED_WORKER: &str = r#"import { createInterface } from "node:readline";

const write = (event) => process.stdout.write(`${JSON.stringify(event)}\n`);
let served = 0;

createInterface({ input: process.stdin }).on("line", (line) => {
  if (line.trim() === "") {
    return;
  }
  const now = JSON.parse(line).generation;
  served += 1;
  if (served === 1) {
    write({ event: "test", name: "the first file's case", status: "passed", generation: now });
    write({ event: "file", status: "completed", generation: now });
    return;
  }
  // Everything the first file had left running, arriving in the middle of the
  // second: a line it printed, a case that only now reported, and a promise it
  // abandoned, which is a *file* result and would otherwise end this file.
  const before = now - 1;
  write({
    event: "output",
    stream: "stdout",
    test: "the first file's case",
    text: "from the previous file\n",
    generation: before,
  });
  write({
    event: "test",
    name: "the first file's case",
    status: "failed",
    message: "resolved after its file had gone",
    generation: before,
  });
  write({
    event: "file",
    status: "run-failed",
    message: "unhandled rejection: left behind",
    generation: before,
  });
  write({ event: "output", stream: "stdout", test: "the second file's case", text: "mine\n", generation: now });
  write({ event: "test", name: "the second file's case", status: "passed", generation: now });
  write({ event: "file", status: "completed", generation: now });
});

process.stdin.on("close", () => process.exit(0));
"#;

#[test]
fn a_worker_that_answers_for_a_finished_file_does_not_disturb_the_running_one() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[("canned-worker.js", CANNED_WORKER)]);
    let root = Utf8PathBuf::from_path_buf(project.path().to_path_buf()).unwrap();
    let command = HostCommand::new(
        HostKind::Node,
        Utf8PathBuf::from("node"),
        root.join("canned-worker.js"),
        root,
    );
    let mut worker = Worker::spawn(&command).expect("node starts");

    let budget = Duration::from_secs(5);
    let first = worker.run_file("/first.test.js", "first.test.js", None, budget, budget);
    let second = worker.run_file("/second.test.js", "second.test.js", None, budget, budget);
    worker.kill();

    assert_eq!(first.status, FileStatus::Completed);
    assert_eq!(first.records.len(), 1, "{first:?}");

    // The `file` event from the first file did not end the second, which is the
    // damage worth measuring: accepting it would have cut the report here and
    // stamped this file `run-failed` with a message from code it does not
    // contain, and the case below would never have been reported at all.
    assert_eq!(second.status, FileStatus::Completed, "{second:?}");
    let [record] = second.records.as_slice() else {
        panic!("only this file's own case is a record of it: {second:?}");
    };
    assert_eq!(record.name, "the second file's case");
    assert_eq!(record.status, TestStatus::Passed);
    assert_eq!(record.output[0].text, "mine\n");

    // One note for the three dropped events, quoting the first of them, and
    // naming the file they came from rather than the number.
    let [note] = second.output.as_slice() else {
        panic!("the drops are one note, before nothing else: {second:?}");
    };
    assert!(
        note.text.contains("3 events arrived from `first.test.js`"),
        "{note:?}"
    );
    assert!(
        note.text.contains("output \"from the previous file\""),
        "{note:?}"
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

    // One worker, so the claim is about bail and not about how many cores the
    // machine running this happens to have. Every worker takes a file before
    // any of them can report a failure, so on a host with twelve cores all
    // twelve files start and none is left unscheduled — which said nothing
    // about whether bail worked.
    let document = json(project.path(), &["--bail", "-j", "1"]);

    assert_eq!(document["bailed"], true, "{document}");
    let not_run = document["fileReports"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|file| file["status"] == "not-run")
        .count();
    assert_eq!(
        not_run, 11,
        "one worker must stop after the file that failed: {document}"
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

#[test]
fn a_file_that_leaves_a_fake_clock_installed_does_not_hang_the_next_one() {
    if !host_ready() {
        return;
    }
    // Two real files through one real worker, which is the only shape this is
    // visible in: `uf test` fans files across workers by size, so whether the
    // second file follows the first in the same process is a property of the
    // schedule rather than of the run.
    let project = Project::new(&[
        (
            "src/stops-time.test.js",
            r#"// @flow
import { expect, it, uft } from "@uniflowed/test";

// No `afterEach` putting it back, deliberately. Every file that fakes a clock
// today has one, and a convention is not a guarantee — the worker is.
uft.useFakeTimers();

it("fakes the clock and never restores it", () => {
  expect(uft.isFakeTimers()).toBe(true);
});
"#,
        ),
        (
            "src/time-passes.test.js",
            r#"// @flow
import { expect, it, uft } from "@uniflowed/test";

it("still has a clock that moves", async () => {
  // This assertion first, and deliberately: under a leaked fake clock the
  // await below never settles — and neither does the timeout the runner races
  // each case against, which is why the failure this guards against was a file
  // that hung with nothing on screen. Asserting the clock is real turns that
  // silence into a named failure; the await after it is what says the clock is
  // not merely reported real but actually running.
  expect(uft.isFakeTimers()).toBe(false);
  await new Promise((resolve) => setTimeout(resolve, 1));
});
"#,
        ),
    ]);
    let root = Utf8PathBuf::from_path_buf(project.path().to_path_buf()).unwrap();
    let mut worker = Worker::spawn(&worker_command(project.path())).expect("node starts");

    let case_budget = Duration::from_secs(5);
    // Generous, because the first file's `import` is what pays to transform
    // `@uniflowed/test` and everything under it through `uf transform`, and a
    // cold cache on a loaded machine spends tens of seconds there before a
    // line of the test runs. A budget tight enough to be a useful bound on
    // *this* file is one that fires on a busy laptop and reports a compile as
    // a hang — the same mistake ubugeeei-prod/uf#420 is about. What bounds the
    // regression instead is the assertion the second file opens with: a leaked
    // clock is reported as a failed expectation long before this expires.
    let file_budget = Duration::from_secs(180);
    let first = worker.run_file(
        root.join("src/stops-time.test.js").as_str(),
        "src/stops-time.test.js",
        None,
        case_budget,
        file_budget,
    );
    let second = worker.run_file(
        root.join("src/time-passes.test.js").as_str(),
        "src/time-passes.test.js",
        None,
        case_budget,
        file_budget,
    );
    worker.kill();

    assert_eq!(first.status, FileStatus::Completed, "{first:?}");
    assert_eq!(first.records.len(), 1, "{first:?}");
    assert_eq!(first.records[0].status, TestStatus::Passed, "{first:?}");

    // The whole of it: the file after the one that stopped time gets a clock.
    assert_eq!(second.status, FileStatus::Completed, "{second:?}");
    let [record] = second.records.as_slice() else {
        panic!("the second file ran its one case: {second:?}");
    };
    assert_eq!(record.status, TestStatus::Passed, "{record:?}");
}

#[test]
fn a_file_that_registers_nothing_fails_rather_than_passing() {
    if !host_ready() {
        return;
    }
    // The shape of ubugeeei-prod/uf#482 without depending on another runner's
    // behaviour: three declarations discovery can read, bound to something
    // that is not `@uniflowed/test`, so the file loads, registers nothing, and
    // reports nothing. It used to be "files 1, passed 0, failed 0" and a tick.
    let project = Project::new(&[(
        "src/probe.test.js",
        r#"// @flow
const record: { [string]: () => void } = {};
const test = (name: string, body: () => void) => {
  record[name] = body;
};

test("collected", () => {});
test("but never registered", () => {});
test("nor run", () => {});
"#,
    )]);

    let (ok, stdout, stderr) = run(project.path(), &[]);

    assert!(
        !ok,
        "a run that executed nothing is not a passing run:\n{stdout}\n{stderr}"
    );
    let document = json(project.path(), &[]);
    assert_eq!(document["success"], false);
    assert_eq!(document["failedFiles"], 1);
    assert_eq!(document["fileReports"][0]["status"], "registered-nothing");
    // The count comes from discovery, so the report says what it expected
    // rather than only that something was missing.
    let reason = document["fileReports"][0]["reason"]
        .as_str()
        .unwrap_or_default();
    assert!(reason.contains("discovery found 3 there"), "{reason}");
    assert!(
        stderr.contains("could not run 1 file"),
        "the summary says so too:\n{stderr}"
    );
}

#[test]
fn a_form_uf_cannot_list_but_can_run_keeps_the_run_green() {
    if !host_ready() {
        return;
    }
    // The other side of the rule above, and the reason "unsupported" is not
    // one thing. `it.each` is uf's own `it` in a shape discovery cannot expand
    // — it would have to evaluate the table to know the names — so `--list`
    // reports it by name and cannot count its cases. The worker runs every row
    // regardless, so the cases are executed and reported, and failing the run
    // over a gap in the *listing* would turn a working suite red.
    let project = Project::new(&[(
        "src/rows.test.js",
        r#"// @flow
import { expect, it } from "@uniflowed/test";

it.each([1, 2, 3])("doubles %s", (n: number) => {
  expect(n * 2).toBe(n + n);
});
"#,
    )]);

    let (ok, stdout, stderr) = run(project.path(), &[]);

    assert!(
        ok,
        "every row ran, so the run is green:\n{stdout}\n{stderr}"
    );
    let document = json(project.path(), &[]);
    assert_eq!(document["passed"], 3);
    assert_eq!(document["success"], true);
    // Named in the report all the same: a form uf cannot expand is never
    // silently dropped, it just is not a reason to fail.
    assert_eq!(document["unsupportedDeclarations"], 1);
    assert_eq!(document["foreignDeclarations"], 0);
}

#[test]
fn a_test_from_another_runner_is_neither_listed_as_runnable_nor_passed_over() {
    if !host_ready() {
        return;
    }
    // The file exactly as ubugeeei-prod/uf#482 reported it. `node:test`
    // accepts the registration and holds it for a runner that is never
    // started, so nothing runs — and `--list` said "1 runnable test" while the
    // run said "0 passed, 0 failed" and exited 0.
    let project = Project::new(&[(
        "scripts/Probe.test.js",
        r#"// @flow
import assert from "node:assert/strict";
import test from "node:test";

test("a case uf lists but does not run", () => {
  assert.equal(1, 2);
});
"#,
    )]);

    let listed = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--list", "Probe"])
        .output()
        .unwrap();
    let listing = String::from_utf8(listed.stdout).unwrap();
    assert!(
        listing.contains("discovered 0 runnable tests"),
        "a case uf cannot run is not a runnable test:\n{listing}"
    );
    assert!(
        listing.contains("node:test"),
        "and the listing says whose it is:\n{listing}"
    );

    let (ok, stdout, stderr) = run(project.path(), &["Probe"]);

    assert!(
        !ok,
        "a run that executed none of the file's cases is not a passing run:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("test declaration from another runner"),
        "and it says why:\n{stderr}"
    );
    // And the `it.each` half of the same count is not what made it red: a form
    // uf runs and cannot list ahead of time keeps a suite green.
    let document = json(project.path(), &["Probe"]);
    assert_eq!(document["unsupportedDeclarations"], 1);
    assert_eq!(document["foreignDeclarations"], 1);
    assert_eq!(document["success"], false);
}
