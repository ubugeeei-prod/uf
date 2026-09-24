//! `uf test` end to end: real files, a real host, real assertions.
//!
//! Every test here runs `uf test` over a project written into this
//! repository's workspace, which imports `@uniflowed/test` and executes on
//! Node exactly as a user's project would. Nothing is stubbed, because the
//! runner's whole job is to drive a JavaScript host and a stubbed host would
//! only prove the stub works.
//!
//! They fail where Node or the installed workspace is missing unless
//! `UF_ALLOW_FIXTURE_SKIP=1` says this machine genuinely cannot run them, so a
//! checkout that never ran `npm ci` cannot silently pass `cargo test`.
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

use support::{
    Project, assert_plain, host_ready, store_with_marked_node, uf, uf_with_tools, worker_command,
};

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
fn json_names_the_host_support_contract() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let document = json(project.path(), &[]);
    let host = &document["host"];

    assert_eq!(host["kind"], serde_json::json!("node"));
    assert_eq!(host["runtimeHost"], serde_json::json!("node"));
    assert_eq!(host["loadsFlow"], serde_json::json!(true));
    assert_eq!(host["collectsCoverage"], serde_json::json!(false));
    assert_eq!(host["support"]["level"], serde_json::json!("implemented"));
    assert_eq!(
        host["support"]["flowLoader"],
        serde_json::json!("@uniflowed/host/register")
    );
    assert_eq!(
        host["support"]["enforcesPermissions"],
        serde_json::json!(["read", "write"])
    );
    assert_eq!(host["support"]["trackingIssue"], serde_json::Value::Null);
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
/// neither of them is: `switch.js`. Since #1504 each test file gets its own
/// copy of every project module, so the switch keeps its state on
/// `globalThis`, under a registered symbol, which the worker's process still
/// shares between files. The callback is still detached — scheduled by a case that
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
// One switch per process, not per module instance: each test file imports its
// own copy of this module, and both files have to throw the same switch.
type Switch = {
  begun: Promise<void>,
  begin: () => void,
  printed: Promise<void>,
  donePrinting: () => void,
};
const KEY = Symbol.for("uf-test/straggler-switch");
function made(): Switch {
  let release: () => void = () => {};
  const begun: Promise<void> = new Promise((resolve) => {
    release = resolve;
  });
  let settle: () => void = () => {};
  const printed: Promise<void> = new Promise((resolve) => {
    settle = resolve;
  });
  return { begun, begin: () => release(), printed, donePrinting: () => settle() };
}
const existing: mixed = Reflect.get(globalThis, KEY);
// $FlowFixMe[incompatible-type] this module is the only writer of KEY.
const shared: Switch = existing != null ? existing : made();
Reflect.set(globalThis, KEY, shared);

export const begun: Promise<void> = shared.begun;
export const printed: Promise<void> = shared.printed;
export function begin(): void {
  shared.begin();
}
export function donePrinting(): void {
  shared.donePrinting();
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
fn a_run_narrowed_to_a_path_keeps_what_the_last_full_run_recorded() {
    if !host_ready() {
        return;
    }
    // A run of one directory used to rewrite the timings with only the files
    // it ran, so the next full run scheduled every other file cold — on this
    // repository, its slowest files started last.
    let project = Project::new(&MIXED);
    run(project.path(), &[]);
    run(project.path(), &["src/ui"]);

    let full = json(project.path(), &[]);
    assert_eq!(full["scheduledWarm"], full["files"]);
    assert_eq!(full["scheduledCold"], 0);
}

#[test]
fn a_run_narrowed_to_a_path_still_forgets_a_deleted_file() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);
    run(project.path(), &[]);
    std::fs::remove_file(project.path().join("src/ui/button.test.js")).unwrap();
    run(project.path(), &["src/math"]);

    let timings = std::fs::read_to_string(project.path().join(".uf/test-timings.json"))
        .expect("both runs record their timings");
    assert!(timings.contains("src/math.test.js"), "{timings}");
    assert!(!timings.contains("src/ui/button.test.js"), "{timings}");
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
fn a_warm_run_of_a_short_suite_starts_only_the_workers_it_can_use() {
    if !host_ready() {
        return;
    }
    // ubugeeei-prod/uf#944. Two files of a few milliseconds are one worker's
    // work: a second worker would spend longer booting than it could save. The
    // first run has nothing recorded, starts a worker per file as a cold run
    // always has, and records what a worker cost to start; the second sizes
    // its pool from that.
    let project = Project::new(&MIXED);
    run(project.path(), &[]);

    let timings = std::fs::read_to_string(project.path().join(".uf/test-timings.json"))
        .expect("the first run records its timings");
    assert!(timings.contains("\"workerStartMicros\""), "{timings}");

    let (_, stdout, _) = run(project.path(), &[]);
    let workers = stdout
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("workers"))
        .map(str::trim);
    assert_eq!(workers, Some("1"), "{stdout}");
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

/// A fast file and a slow one, where the slow one cannot finish until the
/// fast one's result has been read off `uf test`'s stdout.
///
/// The slow file waits for a file named `go` beside it, which the test writes
/// only once it has *read* the fast file's line through a pipe. A runner that
/// printed nothing until the run was over would never let it be written, and
/// the slow file would fail — so a green run is itself the proof that the line
/// arrived while the run was still going, with no sleep deciding it.
const FAST_AND_SLOW: [(&str, &str); 2] = [
    (
        "src/a-fast.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"is quick\", () => {\n  expect(1).toBe(1);\n});\n",
    ),
    (
        "src/b-slow.test.js",
        r#"// @flow
// Padded, so that a cold schedule — longest expected first, by size — starts
// this file before the quick one rather than leaving it to the tie-break.
import { existsSync } from "node:fs";
import { expect, it } from "@uniflowed/test";

it("waits until the quick file has been reported", async () => {
  const go = new URL("./go", import.meta.url);
  const deadline = Date.now() + 50_000;
  while (!existsSync(go) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  expect(existsSync(go)).toBe(true);
}, { timeout: 60_000 });
"#,
    ),
];

#[test]
fn each_file_is_reported_on_a_pipe_as_soon_as_it_finishes() {
    use std::io::BufRead as _;

    if !host_ready() {
        return;
    }
    let project = Project::new(&FAST_AND_SLOW);

    // Two workers, so both files are running at once; `CI` and `NO_COLOR`
    // set, so this is the plain form a CI log gets.
    // A plain `Command` rather than `uf()`, because the output has to be read
    // while the process is still running rather than collected at its end.
    let mut child = std::process::Command::new(support::uf_path())
        .env("CI", "1")
        .env("NO_COLOR", "1")
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "-j", "2"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = std::io::BufReader::new(child.stdout.take().unwrap());
    let (lines, received) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            if lines.send(line).is_err() {
                break;
            }
        }
    });

    let mut seen = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(90);
    let mut released = false;
    while let Ok(line) =
        received.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
    {
        if !released && line.contains("src/a-fast.test.js") {
            // The quick file's line is here and the slow file is still
            // waiting: let it go.
            project.write("src/go", "");
            released = true;
        }
        seen.push(line);
    }
    let output = child.wait_with_output().unwrap();
    reader.join().unwrap();
    let stdout = seen.join("\n");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        released,
        "the quick file was never reported:\n{stdout}\n{stderr}"
    );
    assert!(
        output.status.success(),
        "the slow file only passes if the quick one was reported while it ran:\n{stdout}\n{stderr}"
    );
    assert_plain(&stdout);
    assert!(
        !stderr.contains('\r'),
        "a CI log is never redrawn: {stderr:?}"
    );
    let at = |needle: &str| {
        seen.iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle} is not in:\n{stdout}"))
    };
    // Each file has its line, in the order they finished, and the summary
    // comes after both. The mark is `✓` or `+` by locale, so it is the counts
    // that are looked for.
    assert!(
        at("src/a-fast.test.js") < at("src/b-slow.test.js"),
        "{stdout}"
    );
    assert!(
        seen[at("src/b-slow.test.js")].contains("1 passed"),
        "{stdout}"
    );
    assert!(at("src/b-slow.test.js") < at("slowest files"), "{stdout}");
    assert!(stdout.contains("2 passed, 0 failed"), "{stdout}");
}

/// A failure is drawn with its frame under its own file's line, as that file
/// finishes, and named again at the end so the last screen says what broke.
#[test]
fn a_failure_is_drawn_under_its_file_and_named_again_in_the_summary() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&MIXED);

    let (success, stdout, _) = run(project.path(), &[]);

    assert!(!success);
    let lines: Vec<&str> = stdout.lines().collect();
    let at = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle} is not in:\n{stdout}"))
    };
    // The marks and the separator between counts are `✗`/`·` or `x`/`,` by
    // locale, so the words are what is looked for.
    let file = at("src/math.test.js ");
    for count in ["1 passed", "1 failed", "1 skipped", "1 todo"] {
        assert!(lines[file].contains(count), "{count}: {stdout}");
    }
    assert!(
        lines[file + 1].trim_end().ends_with(" math > fails"),
        "{stdout}"
    );
    assert!(
        at("expected \"flow\" to be \"typescript\"") > file,
        "{stdout}"
    );
    assert!(
        lines[at("src/ui/button.test.js ")].contains("1 passed"),
        "{stdout}"
    );
    // A passing test is counted on its file's line rather than given one.
    assert!(!stdout.contains("math > adds"), "{stdout}");
    let recap = at("src/math.test.js:11  math > fails");
    assert!(
        recap > at("expected \"flow\" to be \"typescript\""),
        "{stdout}"
    );
    assert!(recap < at("slowest files"), "{stdout}");
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

/// `a.test.js` reaches `deep.js` through `shared.js`; `b.test.js` imports
/// `other.js` and nothing else.
const REACHABLE: [(&str, &str); 6] = [
    (".gitignore", ".uf/\nnode_modules/\n"),
    ("src/deep.js", "// @flow\nexport const deep = 1;\n"),
    (
        "src/shared.js",
        "// @flow\nimport { deep } from \"./deep.js\";\nexport const shared = deep;\n",
    ),
    (
        "src/a.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\nimport { shared } from \"./shared.js\";\n\nit(\"reaches deep\", () => {\n  expect(shared).toBe(1);\n});\n",
    ),
    (
        "src/b.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\nimport { other } from \"./other.js\";\n\nit(\"reaches other\", () => {\n  expect(other).toBe(1);\n});\n",
    ),
    ("src/other.js", "// @flow\nexport const other = 1;\n"),
];

/// Run git in `dir`, failing the test with git's own words when it refuses.
fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        // A commit needs an author, and a CI machine has no global identity.
        .args([
            "-c",
            "user.name=uf",
            "-c",
            "user.email=uf@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git started");
    assert!(
        output.status.success(),
        "git {}: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `REACHABLE`, committed as a repository of its own.
fn committed_project() -> Project {
    let project = Project::new(&REACHABLE);
    git(project.path(), &["init", "--quiet"]);
    git(project.path(), &["add", "--all"]);
    git(
        project.path(),
        &["commit", "--quiet", "--message", "fixture"],
    );
    project
}

#[test]
fn changed_lists_only_the_tests_a_change_since_the_ref_reaches() {
    let project = committed_project();
    let dir = project.path();

    // Nothing has changed, so nothing runs, and the opening line says so.
    let (success, stdout, stderr) = run(dir, &["--list", "--changed", "HEAD"]);
    assert!(success, "{stdout}{stderr}");
    assert!(stdout.contains("0 of 2 test files reach them"), "{stdout}");
    assert!(!stdout.contains("reaches deep"), "{stdout}");
    assert!(!stdout.contains("reaches other"), "{stdout}");

    // Two modules away from the only test that depends on it.
    project.write("src/deep.js", "// @flow\nexport const deep = 2;\n");
    let (success, stdout, stderr) = run(dir, &["--list", "--changed", "HEAD"]);
    assert!(success, "{stdout}{stderr}");
    assert!(stdout.contains("1 of 2 test files reach them"), "{stdout}");
    assert!(stdout.contains("reaches deep"), "{stdout}");
    assert!(!stdout.contains("reaches other"), "{stdout}");

    // A deleted module reaches the test that still imports it: that test is
    // what the deletion breaks.
    std::fs::remove_file(dir.join("src/other.js")).expect("delete src/other.js");
    let (success, stdout, stderr) = run(dir, &["--list", "--changed", "HEAD"]);
    assert!(success, "{stdout}{stderr}");
    assert!(stdout.contains("reaches other"), "{stdout}");
}

#[test]
fn changed_runs_everything_after_a_manifest_change_and_refuses_a_ref_git_cannot_find() {
    let project = committed_project();
    let dir = project.path();

    project.write(
        "package.json",
        "{ \"name\": \"changed-fixture\", \"private\": true, \"type\": \"module\" }\n",
    );
    let (success, stdout, stderr) = run(dir, &["--list", "--changed", "HEAD"]);
    assert!(success, "{stdout}{stderr}");
    assert!(stdout.contains("package.json since HEAD"), "{stdout}");
    assert!(stdout.contains("reaches deep"), "{stdout}");
    assert!(stdout.contains("reaches other"), "{stdout}");

    let (success, _, stderr) = run(dir, &["--list", "--changed", "no-such-ref"]);
    assert!(!success);
    assert!(stderr.contains("no-such-ref"), "{stderr}");
}

#[test]
fn changed_cannot_be_combined_with_watch_or_coverage() {
    let project = Project::new(&MIXED);

    for flag in ["--watch", "--coverage"] {
        let (success, _, stderr) = run(project.path(), &[flag, "--changed", "main"]);

        assert!(!success, "{flag}");
        assert!(stderr.contains("cannot be combined"), "{flag}: {stderr}");
    }
}

/// `--changed` is uf's selection, so a Bun runner is handed the files it
/// reached and nothing else, rather than the whole suite.
#[test]
fn changed_narrows_what_a_bun_runner_is_handed() {
    if !host_ready() || !support::bun_ready() {
        return;
    }
    let project = Project::new(&REACHABLE);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\nexport default defineConfig({ test: { runner: \"bun\" } });\n",
    );
    let dir = project.path();
    git(dir, &["init", "--quiet"]);
    git(dir, &["add", "--all"]);
    git(dir, &["commit", "--quiet", "--message", "fixture"]);
    project.write(
        "src/deep.js",
        "// @flow\nexport const deep = 1;\n// an edit that changes nothing it exports\n",
    );

    let (success, stdout, stderr) = run(dir, &["--changed", "HEAD"]);

    assert!(success, "{stdout}\n{stderr}");
    assert!(stdout.contains("1 of 2 test files reach them"), "{stdout}");
    assert!(
        stdout.contains("1 passed, 0 failed, 0 skipped"),
        "{stdout}\n{stderr}"
    );
    let report =
        std::fs::read_to_string(dir.join(".uf/bun-test/junit.xml")).unwrap_or_else(|error| {
            panic!("the Bun run wrote its report: {error}\n{stdout}\n{stderr}")
        });
    assert!(report.contains("reaches deep"), "{report}");
    assert!(!report.contains("reaches other"), "{report}");
}

/// `MIXED`, with a third test file, so two shards cannot hold one file each.
fn sharded_project() -> Project {
    let project = Project::new(&MIXED);
    project.write(
        "src/third.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"counts to three\", () => {\n  expect(1 + 2).toBe(3);\n});\n",
    );
    project
}

/// A JUnit document without its `time` attributes: the one thing two runs of
/// one suite may disagree about.
fn without_times(document: &str) -> String {
    const TIME: &str = " time=\"";
    let mut out = String::with_capacity(document.len());
    let mut rest = document;
    while let Some(at) = rest.find(TIME) {
        out.push_str(&rest[..at]);
        let after = &rest[at + TIME.len()..];
        rest = after.find('"').map_or("", |end| &after[end + 1..]);
    }
    out.push_str(rest);
    out
}

/// The test files a shard's record says it ran.
fn record_files(dir: &Path, name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(dir.join(".uf/test-shards").join(name))
        .unwrap_or_else(|error| panic!("the shard wrote {name}: {error}"));
    let record: serde_json::Value = serde_json::from_str(&text).expect("a record is JSON");
    record["report"]["files"]
        .as_array()
        .expect("the record lists its files")
        .iter()
        .map(|file| file["file"].as_str().expect("a path").to_owned())
        .collect()
}

/// The promise `--shard` and `--merge-shards` make together: every file runs in
/// one shard, and the merge reports what one run over the suite reports.
#[test]
fn shards_run_every_file_once_and_merge_into_the_report_one_run_makes() {
    if !host_ready() {
        return;
    }
    let project = sharded_project();
    let dir = project.path();

    let (whole, _, stderr) = run(
        dir,
        &["--reporter", "junit", "--reporter-outfile", "whole.xml"],
    );
    assert!(!whole, "one case fails on purpose: {stderr}");
    let (_, first, first_err) = run(dir, &["--shard", "1/2"]);
    let (_, second, second_err) = run(dir, &["--shard", "2/2"]);
    assert!(
        first.contains(".uf/test-shards/1-of-2.json"),
        "{first}\n{first_err}"
    );
    assert!(second.contains("of 3 test files"), "{second}\n{second_err}");

    let mut ran = record_files(dir, "1-of-2.json");
    let rest = record_files(dir, "2-of-2.json");
    assert!(!ran.is_empty() && !rest.is_empty(), "{ran:?} {rest:?}");
    ran.extend(rest);
    ran.sort();
    assert_eq!(
        ran,
        [
            "src/math.test.js",
            "src/third.test.js",
            "src/ui/button.test.js"
        ]
    );

    let (merged, stdout, stderr) = run(
        dir,
        &[
            "--merge-shards",
            "--reporter",
            "junit",
            "--reporter-outfile",
            "merged.xml",
        ],
    );
    assert!(
        !merged,
        "the merged suite fails as the whole one did:\n{stdout}"
    );
    assert!(stdout.contains("3 test files from 2 shards"), "{stdout}");
    assert!(stderr.contains("uf test failed with 1 failure"), "{stderr}");
    let whole_xml = std::fs::read_to_string(dir.join("whole.xml")).expect("the whole run's JUnit");
    let merged_xml = std::fs::read_to_string(dir.join("merged.xml")).expect("the merge's JUnit");
    similar_asserts::assert_eq!(without_times(&whole_xml), without_times(&merged_xml));

    let whole_json = json(dir, &[]);
    let merged_json = json(dir, &["--merge-shards"]);
    for key in ["files", "passed", "failed", "skipped", "todo", "success"] {
        assert_eq!(whole_json[key], merged_json[key], "{key}");
    }
    let cases = |document: &serde_json::Value| {
        without_timings(&serde_json::to_string_pretty(&document["tests"]).expect("serialises"))
    };
    similar_asserts::assert_eq!(cases(&whole_json), cases(&merged_json));
    assert_eq!(merged_json["shards"]["count"], 2);
    assert_eq!(merged_json["host"], serde_json::Value::Null);
}

/// Shards cut from different durations ran some files twice and others never,
/// and the merge says so rather than reporting either.
#[test]
fn shards_cut_from_different_timings_are_refused_when_merged() {
    if !host_ready() {
        return;
    }
    let project = sharded_project();
    let dir = project.path();

    // One shard cut before any duration was recorded, the other after a whole
    // run recorded them.
    run(dir, &["--shard", "1/2"]);
    run(dir, &[]);
    run(dir, &["--shard", "2/2"]);
    let (merged, stdout, stderr) = run(dir, &["--merge-shards"]);

    assert!(!merged, "{stdout}");
    assert!(stderr.contains("cut from different suites"), "{stderr}");
}

#[test]
fn a_merge_refuses_a_missing_shard_and_the_flags_that_shape_a_run() {
    if !host_ready() {
        return;
    }
    let project = sharded_project();
    let dir = project.path();
    run(dir, &["--shard", "1/2"]);

    let (merged, _, stderr) = run(dir, &["--merge-shards"]);
    assert!(!merged);
    assert!(stderr.contains("no record for shard 2 of 2"), "{stderr}");

    let (merged, _, stderr) = run(dir, &["--merge-shards", "-j", "2", "src"]);
    assert!(!merged);
    assert!(
        stderr.contains("runs nothing, so it cannot take 2 flags: -j, PATH"),
        "{stderr}"
    );
}

#[test]
fn a_shard_is_refused_with_watch_and_named_when_it_is_not_one() {
    let project = Project::new(&MIXED);

    let (success, _, stderr) = run(project.path(), &["--watch", "--shard", "1/2"]);
    assert!(!success);
    assert!(
        stderr.contains("--watch and --shard cannot be combined"),
        "{stderr}"
    );

    let (success, _, stderr) = run(project.path(), &["--shard", "3/2"]);
    assert!(!success);
    assert!(stderr.contains("there is no shard 3 of 2"), "{stderr}");
}

/// A test file, and a benchmark file whose benchmark waits a couple of
/// milliseconds a call, so its median is nowhere near zero.
const BENCHED: [(&str, &str); 2] = [
    (
        "src/sum.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"adds\", () => {\n  expect(1 + 1).toBe(2);\n});\n",
    ),
    (
        "src/wait.bench.js",
        "// @flow\nimport { bench } from \"@uniflowed/test\";\n\nbench(\"waits\", () => new Promise((resolve) => setTimeout(resolve, 2)), {\n  warmup: 1,\n  iterations: 5,\n});\n",
    ),
];

/// The case named `name` in a `--json` document.
fn case_named<'a>(document: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    document["tests"]
        .as_array()
        .and_then(|cases| cases.iter().find(|case| case["name"] == name))
        .unwrap_or_else(|| panic!("no case named {name}: {document:#}"))
}

#[test]
fn a_benchmark_is_skipped_by_a_run_of_the_tests_and_timed_by_a_run_of_the_benchmarks() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&BENCHED);

    let tests = json(project.path(), &[]);
    assert_eq!(case_named(&tests, "waits")["status"], "bench");
    assert_eq!(case_named(&tests, "adds")["status"], "passed");

    let benches = json(project.path(), &["--bench"]);
    let waits = case_named(&benches, "waits");
    assert_eq!(waits["status"], "passed", "{benches:#}");
    assert_eq!(waits["bench"]["samples"], 5, "{waits:#}");
    assert!(
        waits["bench"]["medianMicros"]
            .as_u64()
            .is_some_and(|median| median >= 1_000),
        "{waits:#}"
    );
    assert_eq!(case_named(&benches, "adds")["status"], "not-bench");
    assert_eq!(benches["benchmarks"]["found"], false);
}

#[test]
fn a_benchmark_slower_than_its_saved_baseline_fails_the_run_by_name() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&BENCHED);
    let dir = project.path();

    let (saved, stdout, stderr) = run(dir, &["--bench", "--save-baseline"]);
    assert!(saved, "{stdout}\n{stderr}");
    assert!(
        stdout.contains("saved to .uf/bench-baseline.json"),
        "{stdout}"
    );
    let path = dir.join(".uf/bench-baseline.json");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the baseline was written"))
            .expect("the baseline is JSON");
    assert_eq!(written["benchmarks"][0]["name"], "waits", "{written:#}");

    let baseline = |median: u64| {
        format!(
            "{{\"version\":1,\"benchmarks\":[{{\"file\":\"src/wait.bench.js\",\"name\":\"waits\",\"medianMicros\":{median},\"samples\":5}}]}}"
        )
    };
    // A hundredth of what a call that waits two milliseconds takes.
    std::fs::write(&path, baseline(20)).expect("rewrite the baseline");
    let (passed, stdout, stderr) = run(dir, &["--bench"]);
    assert!(!passed, "{stdout}");
    assert!(
        stderr.contains("1 benchmark ran slower than .uf/bench-baseline.json allows"),
        "{stderr}"
    );
    assert!(stderr.contains("src/wait.bench.js > waits"), "{stderr}");

    // A hundred times what it takes is an improvement, which passes.
    std::fs::write(&path, baseline(10_000_000)).expect("rewrite the baseline");
    let (passed, stdout, stderr) = run(dir, &["--bench"]);
    assert!(passed, "{stdout}\n{stderr}");
    assert!(stdout.contains("improved"), "{stdout}");
}

#[test]
fn a_run_of_the_benchmarks_is_refused_with_what_would_change_its_numbers() {
    let project = Project::new(&BENCHED);

    for flags in [
        &["--watch"][..],
        &["--shard", "1/2"][..],
        &["--coverage"][..],
        &["--browser"][..],
    ] {
        let mut args = vec!["--bench"];
        args.extend_from_slice(flags);
        let (success, _, stderr) = run(project.path(), &args);

        assert!(!success, "{flags:?}");
        assert!(
            stderr.contains(&format!("{} and --bench cannot be combined", flags[0])),
            "{flags:?}: {stderr}"
        );
    }
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
fn a_file_that_leaves_the_document_dirty_does_not_break_the_next_one() {
    if !host_ready() {
        return;
    }
    // The third occurrence of one seam, after the leaked stub
    // (ubugeeei-prod/uf#417) and the leaked clock (#581): a worker serves many
    // files out of one process, and what a file reaches around this package to
    // change is handed to whichever file the schedule puts next. The document
    // is the easiest of the three to miss, because nothing in `@uniflowed/test`
    // installs it — `@uniflowed/react-testing` puts one on the global object on
    // the first render and keeps it for the life of the process on purpose,
    // since replacing it would strand every React root already mounted in the
    // old one.
    //
    // Two real files through one real worker, which is the only shape this is
    // visible in: `uf test` fans files across workers by size, so whether the
    // second file follows the first in the same process is a property of the
    // schedule rather than of the run. That is also the defect — on `main` the
    // library suite passed on a cold schedule and failed six to ten cases on a
    // warm one, with `getByRole "img": found 6 elements` naming the file that
    // queried rather than the file that wrote. See ubugeeei-prod/uf#607.
    let project = Project::new(&[
        (
            "src/writes-the-body.test.js",
            r#"// @flow
import { expect, it } from "@uniflowed/test";
import { render } from "@uniflowed/react-testing";

it("writes markup into the body by hand and never takes it out", () => {
  // Which a hydration test must do — hydration is React attaching to markup
  // that is already there — and which `cleanup()` therefore does not undo: it
  // unmounts what `render` mounted, and this is not that. `rsc-split.test.js`
  // and `streaming.test.js` both do exactly this to a real document.
  render(<p>a document, please</p>);
  const left = globalThis.document.createElement("img");
  left.setAttribute("alt", "left behind");
  globalThis.document.body.replaceChildren(left);
  globalThis.document.body.setAttribute("class", "left-behind");

  expect(globalThis.document.body.children.length).toBe(1);
});
"#,
        ),
        (
            "src/queries-the-body.test.js",
            r#"// @flow
import { expect, it } from "@uniflowed/test";
import { render, screen } from "@uniflowed/react-testing";

it("is handed a document nobody else has written to", () => {
  // Nothing has rendered in this file, so there is no document yet: the worker
  // takes back the one the file before installed, with everything that file
  // wrote into it, and hands this file the process a fresh worker would. That
  // assertion comes first because it names the leak rather than leaving it to
  // be inferred from a count further down.
  expect(typeof globalThis.document).toBe("undefined");

  // Rendering puts the same window back, emptied.
  render(<img alt="the only one" src="/b.png" />);
  expect(screen.getAllByRole("img").length).toBe(1);
  expect(globalThis.document.body.getAttributeNames().length).toBe(0);
});
"#,
        ),
    ]);
    let root = Utf8PathBuf::from_path_buf(project.path().to_path_buf()).unwrap();
    let mut worker = Worker::spawn(&worker_command(project.path())).expect("node starts");

    let case_budget = Duration::from_secs(15);
    // Generous for the reason the clock test above gives: the first file's
    // `import` is what pays to transform `@uniflowed/react-testing`, React and
    // everything under them through `uf transform`, and a cold cache on a
    // loaded machine spends tens of seconds there before a line of the test
    // runs. What bounds the regression is the assertion the second file opens
    // with, not this.
    let file_budget = Duration::from_secs(240);
    let first = worker.run_file(
        root.join("src/writes-the-body.test.js").as_str(),
        "src/writes-the-body.test.js",
        None,
        case_budget,
        file_budget,
    );
    let second = worker.run_file(
        root.join("src/queries-the-body.test.js").as_str(),
        "src/queries-the-body.test.js",
        None,
        case_budget,
        file_budget,
    );
    worker.kill();

    assert_eq!(first.status, FileStatus::Completed, "{first:?}");
    assert_eq!(first.records.len(), 1, "{first:?}");
    assert_eq!(first.records[0].status, TestStatus::Passed, "{first:?}");

    assert_eq!(second.status, FileStatus::Completed, "{second:?}");
    let [record] = second.records.as_slice() else {
        panic!("the second file ran its one case: {second:?}");
    };
    assert_eq!(record.status, TestStatus::Passed, "{record:?}");
}

#[test]
fn a_file_that_changes_the_window_hands_the_next_file_the_process_it_found() {
    if !host_ready() {
        return;
    }
    // The fourth occurrence of the seam above, found once `uf test` started
    // packing more files into each worker (ubugeeei-prod/uf#944): the body was
    // put back and the window around it was not. Two shapes of it failed this
    // repository's own suite depending on which files shared a worker —
    // `matchMedia is not a function` after `ui.test.js` removed it, and
    // `server-actions.test.js` unable to build a `FormData` because a render
    // had replaced Node's with the document's, which refuses Node's `Blob`.
    //
    // Three files through one worker: one that renders and changes the window,
    // one that never renders, and one that renders again.
    let project = Project::new(&[
        (
            "src/changes-the-window.test.js",
            r#"// @flow
import { expect, it } from "@uniflowed/test";
import { render } from "@uniflowed/react-testing";

it("changes the window a render installed and never changes it back", () => {
  render(<p>a window, please</p>);
  globalThis.window.matchMedia = undefined;
  globalThis.document.documentElement.setAttribute("class", "dark");

  expect(globalThis.window.matchMedia).toBe(undefined);
});
"#,
        ),
        (
            "src/renders-nothing.test.js",
            r#"// @flow
import { afterEach, expect, it } from "@uniflowed/test";
import { cleanup } from "@uniflowed/react-testing";

// What `packages/router/intercepting-routes.test.js` does in a file whose cases
// render on the server and never into a document. The root the file before
// left mounted must already be gone, or unmounting it here reads a `window`
// this file was never given.
afterEach(() => {
  cleanup();
});

it("is handed Node's own globals rather than the document's", () => {
  expect(typeof globalThis.document).toBe("undefined");
  expect(typeof globalThis.window).toBe("undefined");

  const form = new FormData();
  form.append("avatar", new Blob(["bytes"]), "avatar.png");
  expect(form.get("avatar") instanceof Blob).toBe(true);
});
"#,
        ),
        (
            "src/renders-again.test.js",
            r#"// @flow
import { expect, it } from "@uniflowed/test";
import { render, screen } from "@uniflowed/react-testing";

it("gets the window back as it was created", () => {
  render(<output>again</output>);

  expect(screen.getByText("again").tagName).toBe("OUTPUT");
  expect(typeof globalThis.window.matchMedia).toBe("function");
  expect(globalThis.document.documentElement.getAttributeNames().length).toBe(0);
});
"#,
        ),
    ]);
    let root = Utf8PathBuf::from_path_buf(project.path().to_path_buf()).unwrap();
    let mut worker = Worker::spawn(&worker_command(project.path())).expect("node starts");

    // Generous for the reason the document test above gives: a cold transform
    // cache on a loaded machine. The assertions each file opens with are what
    // bound the regression.
    let case_budget = Duration::from_secs(15);
    let file_budget = Duration::from_secs(240);
    let outcomes: Vec<_> = [
        "src/changes-the-window.test.js",
        "src/renders-nothing.test.js",
        "src/renders-again.test.js",
    ]
    .iter()
    .map(|file| {
        (
            *file,
            worker.run_file(
                root.join(file).as_str(),
                file,
                None,
                case_budget,
                file_budget,
            ),
        )
    })
    .collect();
    worker.kill();

    for (file, outcome) in &outcomes {
        assert_eq!(outcome.status, FileStatus::Completed, "{file}: {outcome:?}");
        let [record] = outcome.records.as_slice() else {
            panic!("{file} ran its one case: {outcome:?}");
        };
        assert_eq!(record.status, TestStatus::Passed, "{file}: {record:?}");
    }
}

/// A file that changes the process the way a test is allowed to, and never
/// changes it back.
///
/// Every change goes through `uft`, because that is the promise under test:
/// what a file changes through the test API does not outlive the file. Each
/// change is one ubugeeei-prod/uf#417, #581 or #607 found leaking. The
/// assertions come first, so whichever of two such files runs second is the
/// one that fails if anything leaked — and the order two files take through one
/// worker is the schedule's business, not this test's.
fn changes_process_state(name: &str) -> String {
    format!(
        r#"// @flow
import {{ expect, it, uft }} from "@uniflowed/test";

it("starts from the state every file starts from, and leaves it changed", async () => {{
  expect(process.env.UF_ISOLATION_PROBE).toBe(undefined);
  expect(globalThis.ufIsolationProbe).toBe(undefined);
  // Before the await, deliberately: under a leaked fake clock the timer below
  // never fires, and neither does the one the runner races the case against,
  // so a leak has to be named here or it is a hang.
  expect(uft.isFakeTimers()).toBe(false);
  await new Promise((resolve) => setTimeout(resolve, 1));

  uft.stubEnv("UF_ISOLATION_PROBE", "{name}");
  uft.stubGlobal("ufIsolationProbe", "{name}");
  uft.useFakeTimers();
  expect(process.env.UF_ISOLATION_PROBE).toBe("{name}");
}});
"#
    )
}

#[test]
fn two_files_that_change_the_process_in_one_worker_each_start_clean() {
    if !host_ready() {
        return;
    }
    // ubugeeei-prod/uf#944 is about keeping workers warm, and a warm worker is
    // one process serving file after file — which is the shape #417, #581 and
    // #607 each leaked through, one piece of process state at a time. The
    // tests above hold the worker to each piece with the protocol driven by
    // hand; this one holds the whole command to all of them at once: the real
    // `uf test`, the real loader, and `-j 1`, which is the only way to be sure
    // two files share a worker rather than hoping the schedule put them
    // together.
    let project = Project::new(&[
        ("src/first.test.js", &changes_process_state("first")),
        ("src/second.test.js", &changes_process_state("second")),
    ]);

    let document = json(project.path(), &["-j", "1"]);

    assert_eq!(document["failed"], 0, "{document}");
    assert_eq!(document["failedFiles"], 0, "{document}");
    assert_eq!(document["passed"], 2, "{document}");
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

#[test]
fn a_native_project_names_missing_native_modules_instead_of_running_with_a_web_shim() {
    let project = Project::new(&[(
        "src/native.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"runs\", () => { expect(1).toBe(1); });\n",
    )]);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\nexport default defineConfig({ app: { framework: \"react-native\" } });\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--json", "native.test.js"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(
        !output.status.success(),
        "React Native tests must not fall through to the web runner:\n{stdout}\n{stderr}"
    );
    let report: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["fileReports"][0]["status"], "load-failed");
    let reason = report["fileReports"][0]["reason"].as_str().unwrap();
    assert!(
        reason.contains("uf test (react-native): install"),
        "{reason}"
    );
    assert_eq!(report["passed"], 0);
}

/// A project whose `uf.config.js` names `runner`.
fn with_runner(runner: &str, files: &[(&str, &str)]) -> Project {
    let project = Project::new(files);
    project.write(
        "uf.config.js",
        &format!(
            "// @flow\nimport {{ defineConfig }} from \"@uniflowed/config\";\n\nexport default defineConfig({{ test: {{ runner: \"{runner}\" }} }});\n"
        ),
    );
    project
}

/// One Flow test file with a case that passes and a case that fails.
const PASS_AND_FAIL: (&str, &str) = (
    "src/math.test.js",
    r#"// @flow
import { describe, expect, it } from "@uniflowed/test";

const add = (a: number, b: number): number => a + b;

describe("math", () => {
  it("adds", () => {
    expect(add(1, 2)).toBe(3);
  });

  it("fails on purpose", () => {
    expect(add(1, 1)).toBe(3);
  });
});
"#,
);

/// `test.runner: "bun"` runs the suite with `bun test`, Flow and all, and says
/// what it ran. ubugeeei-prod/uf#942.
#[test]
fn a_bun_runner_runs_the_suite_with_bun_test() {
    if !host_ready() || !support::bun_ready() {
        return;
    }
    let project = with_runner("bun", &[PASS_AND_FAIL]);

    let (success, stdout, stderr) = run(project.path(), &[]);

    assert!(!success, "one case fails:\n{stdout}\n{stderr}");
    assert!(
        stdout.contains("1 passed, 1 failed, 0 skipped"),
        "{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("bun test failed with 1 failure"),
        "{stderr}"
    );
}

/// The claim `test.runner` makes: the same suite under either runner reports
/// the same passes and the same failures.
#[test]
fn both_runners_report_the_same_passes_and_failures() {
    if !host_ready() || !support::bun_ready() {
        return;
    }
    let ours = Project::new(&[PASS_AND_FAIL]);
    let document = json(ours.path(), &[]);
    let mut by_uf: Vec<(String, String)> = document["tests"]
        .as_array()
        .expect("the report lists every case")
        .iter()
        .map(|case| {
            (
                case["name"].as_str().unwrap_or_default().to_owned(),
                case["status"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect();
    by_uf.sort();

    let theirs = with_runner("bun", &[PASS_AND_FAIL]);
    let (_, stdout, stderr) = run(theirs.path(), &[]);
    let report = std::fs::read_to_string(theirs.path().join(".uf/bun-test/junit.xml"))
        .unwrap_or_else(|error| {
            panic!("the Bun run wrote its report: {error}\n{stdout}\n{stderr}")
        });

    assert_eq!(
        by_uf,
        vec![
            (String::from("math > adds"), String::from("passed")),
            (
                String::from("math > fails on purpose"),
                String::from("failed")
            ),
        ]
    );
    assert!(report.contains("<testcase name=\"adds\""), "{report}");
    assert!(
        report.contains("<testcase name=\"fails on purpose\""),
        "{report}"
    );
    assert_eq!(report.matches("<failure").count(), 1, "{report}");
}

/// A pinned Bun is installed the way `test.runtime` installs one, so a Bun that
/// cannot be settled is refused naming the spec, before anything runs. The
/// publishers serve nothing here, so the attempt cannot reach the network.
#[test]
fn a_pinned_bun_that_cannot_be_installed_is_refused_before_anything_runs() {
    let project = with_runner("bun@0.0.1", &[PASS_AND_FAIL]);
    let tools = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir_all(tools.path().join("nothing")).expect("the empty publisher");

    let output = uf_with_tools(tools.path())
        .arg("--cwd")
        .arg(project.path())
        .arg("test")
        .output()
        .expect("uf started");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "{stdout}\n{stderr}");
    assert!(stderr.contains("`bun@0.0.1`"), "{stderr}");
    assert!(!stdout.contains("adds"), "nothing ran: {stdout}");
}

/// `bun test` runs on Bun alone, so a `test.runtime` naming another runtime is
/// refused by name, before anything runs, rather than ignored — which is what
/// the Bun runner did with it before it resolved its Bun through
/// `test.runtime`.
#[test]
fn a_bun_runner_on_a_runtime_that_is_not_bun_is_refused_by_name() {
    let project = Project::new(&[PASS_AND_FAIL]);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\nexport default defineConfig({ test: { runner: \"bun\", runtime: \"node\" } });\n",
    );

    let (success, stdout, stderr) = run(project.path(), &[]);

    assert!(!success, "{stdout}\n{stderr}");
    assert!(stderr.contains("test.runtime"), "{stderr}");
    assert!(
        stderr.contains("a Bun runner runs on the Bun it names"),
        "{stderr}"
    );
    assert!(!stdout.contains("adds"), "nothing ran: {stdout}");
}

/// A flag `bun test` has no meaning for is refused by name rather than dropped.
#[test]
fn a_flag_bun_test_cannot_honour_is_refused_by_name() {
    let project = with_runner("bun", &[PASS_AND_FAIL]);

    let (success, _, stderr) = run(project.path(), &["--list"]);

    assert!(!success);
    assert!(
        stderr.contains("`bun test` cannot honour 1 flag"),
        "{stderr}"
    );
    assert!(stderr.contains("--list"), "{stderr}");
}

/// A file whose cases never reach `bun test` fails the run by name, where
/// `bun test` itself would report nothing about it and exit 0.
#[test]
fn a_file_bun_runs_nothing_from_fails_the_run() {
    if !host_ready() || !support::bun_ready() {
        return;
    }
    let project = with_runner(
        "bun",
        &[
            (
                "src/sum.test.js",
                "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"adds\", () => {\n  expect(1 + 1).toBe(2);\n});\n",
            ),
            (
                "src/hidden.test.js",
                "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nif (globalThis.ufNeverSet === true) {\n  it(\"never registers\", () => {\n    expect(1).toBe(1);\n  });\n}\n",
            ),
        ],
    );

    let (success, stdout, stderr) = run(project.path(), &[]);

    assert!(!success, "{stdout}\n{stderr}");
    assert!(stderr.contains("ran no case from 1 file"), "{stderr}");
    assert!(stderr.contains("src/hidden.test.js"), "{stderr}");
}

/// `test.runtime` at a version runs the suite on the release in the store, and
/// a `node` the tests start themselves finds that same release first on
/// `PATH`.
///
/// Both halves, because either alone is a suite running on two Nodes: a worker
/// started from the store whose child processes find the machine's, or the
/// reverse. ubugeeei-prod/uf#940.
#[test]
fn a_versioned_test_runtime_runs_the_suite_on_the_release_in_the_store() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/path.test.js",
        "// @flow\nimport { execFileSync } from \"node:child_process\";\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"finds node on PATH\", () => {\n  expect(String(execFileSync(\"node\", [\"-p\", \"40 + 2\"])).trim()).toBe(\"42\");\n});\n",
    )]);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\nexport default defineConfig({ test: { runtime: \"node@99.0.0\" } });\n",
    );
    let (tools, marks) = store_with_marked_node("99.0.0");

    let output = uf_with_tools(tools.path())
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--json"])
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(output.status.success(), "{stdout}\n{stderr}");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("--json output");
    assert_eq!(document["passed"], 1, "{stdout}");
    let marked = std::fs::read_to_string(&marks).unwrap_or_default();
    assert!(
        !marked.is_empty(),
        "the suite ran on the machine's node rather than the store's:\n{stderr}"
    );
    assert!(
        marked.contains("-p 40 + 2"),
        "a node the test started found another release first on PATH:\n{marked}"
    );
    // Already in the store, so there was nothing to install and nothing said.
    assert!(!stderr.contains("installing"), "{stderr}");
}

/// A release that cannot be installed — offline, or not published — stops the
/// run with the key and the spec that asked for it.
#[test]
fn a_test_runtime_that_cannot_be_installed_is_refused_naming_the_spec() {
    let project = Project::new(&[(
        "src/sum.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"adds\", () => { expect(1 + 1).toBe(2); });\n",
    )]);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\nexport default defineConfig({ test: { runtime: \"node@99.0.1\" } });\n",
    );
    let (tools, _) = store_with_marked_node("99.0.0");

    let output = uf_with_tools(tools.path())
        .arg("--cwd")
        .arg(project.path())
        .arg("test")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stderr.contains("installing node@99.0.1"),
        "said first:\n{stderr}"
    );
    assert!(
        stderr.contains("test.runtime is `node@99.0.1`, and node@99.0.1 could not be installed"),
        "{stderr}"
    );
    assert!(!stdout.contains("adds"), "{stdout}");
}

/// A `node` that is a shim, and a run that has to end anyway.
///
/// Version managers put a shim on `PATH` rather than the runtime itself, and
/// some of them *fork* the real one instead of `exec`ing it — Volta's
/// `volta-shim` is where this was found. uf's child is then the shim, and the
/// process importing the test file is the shim's child, holding the worker's
/// stdout by inheritance.
///
/// Retiring a worker used to signal uf's own child, which ended the shim and
/// left that grandchild running with the pipe open. Every file ran, every
/// result was reported, and `uf test` never returned: no output, no exit, on
/// every run on such a machine. The signal goes to the worker's process group
/// now — see `uf_test::host::Worker::kill` — and what this asserts is the
/// thing that was missing, which is an end.
#[test]
fn a_forking_host_shim_does_not_leave_the_run_hanging() {
    use std::os::unix::fs::PermissionsExt as _;

    if !host_ready() {
        return;
    }
    let project = Project::new(&[(
        "src/sum.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\nit(\"adds\", () => { expect(1 + 1).toBe(2); });\n",
    )]);

    let found = std::process::Command::new("sh")
        .args(["-c", "command -v node"])
        .output()
        .unwrap();
    let real = String::from_utf8(found.stdout).unwrap().trim().to_owned();
    let shims = tempfile::tempdir().unwrap();
    let shim = shims.path().join("node");
    let marks = shims.path().join("pids");
    // `exec 3<&0` and then `<&3`, rather than `<&0` or nothing at all. POSIX
    // gives an asynchronous command `/dev/null` for standard input *before*
    // its own redirections, and dash reads that in the order it is written:
    // `<&0` there duplicates the `/dev/null` the shell had just installed, so
    // the worker met end of input and exited 0 before it was asked for
    // anything. Only macOS's `/bin/sh` passed the pipe through, which is why
    // this went out green and came back red. A descriptor saved before the
    // redirection cannot be the one the rule replaced.
    //
    // The two `echo`s are what keep the test non-vacuous: a shell that decided
    // to `exec` the last command would make this an ordinary host again, and a
    // regression test for a forking shim that stopped forking would pass
    // without exercising anything. The assertion below is that two different
    // processes were involved.
    std::fs::write(
        &shim,
        format!(
            "#!/bin/sh\nexec 3<&0\n'{real}' \"$@\" <&3 &\nchild=$!\n\
             printf 'shim %s child %s\\n' \"$$\" \"$child\" >> '{marks}'\nwait \"$child\"\n",
            marks = marks.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        shims.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--json"])
        .env("PATH", path)
        .timeout(Duration::from_secs(120))
        .output()
        .expect("uf runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // A run killed by the deadline has no exit code, only the signal that
    // ended it. That is the failure this test exists for, so it is named
    // before the status is judged.
    assert!(
        output.status.code().is_some(),
        "`uf test` never returned on a host whose `node` forks:\n{stdout}\n{stderr}"
    );
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("--json output");
    assert_eq!(document["passed"], 1, "{stdout}");
    assert_eq!(document["failed"], 0, "{stdout}");

    // And that the host uf started really was a shim in front of another
    // process, which is the whole premise.
    let marked = std::fs::read_to_string(&marks).unwrap_or_default();
    let forked = marked.lines().any(|line| {
        let mut words = line.split_whitespace();
        matches!(
            (words.next(), words.next(), words.next(), words.next()),
            (Some("shim"), Some(shim), Some("child"), Some(child)) if shim != child
        )
    });
    assert!(
        forked,
        "the shim exec'd rather than forked, so this proved nothing:\n{marked}"
    );
}

/// Every line a child writes to stdout and stderr, as one channel.
fn lines_of(child: &mut std::process::Child) -> std::sync::mpsc::Receiver<String> {
    use std::io::{BufRead as _, BufReader, Read};
    let (sender, lines) = std::sync::mpsc::channel();
    let streams: [Box<dyn Read + Send>; 2] = [
        Box::new(child.stdout.take().expect("stdout is piped")),
        Box::new(child.stderr.take().expect("stderr is piped")),
    ];
    for stream in streams {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
    }
    lines
}

/// Read lines into `transcript` until one contains `needle`, within `budget`.
fn wait_for(
    lines: &std::sync::mpsc::Receiver<String>,
    needle: &str,
    budget: Duration,
    transcript: &mut String,
) -> bool {
    let deadline = std::time::Instant::now() + budget;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Ok(line) = lines.recv_timeout(left) else {
            return false;
        };
        transcript.push_str(&line);
        transcript.push('\n');
        if line.contains(needle) {
            return true;
        }
    }
    false
}

/// A watch session reruns an edit in the worker it already has, against the
/// edited code.
///
/// Both halves are the promise. The worker is kept — the same process runs the
/// file before and after the edit, which is what makes a rerun cost the file
/// rather than a host start and the whole dependency graph — and it still sees
/// the edit, through a module between the test and the file that changed.
#[test]
fn a_watch_session_reruns_an_edit_in_the_worker_it_already_has() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[
        (
            "probe.test.js",
            "// @flow\nimport fs from \"node:fs\";\nimport { expect, it } from \"@uniflowed/test\";\n\
             import { double } from \"./math.js\";\n\n\
             it(\"doubles\", () => {\n  fs.mkdirSync(\".uf\", { recursive: true });\n  \
             fs.appendFileSync(\".uf/pids.txt\", `${process.pid}\\n`);\n  \
             expect(double(21)).toBe(42);\n});\n",
        ),
        (
            "math.js",
            "// @flow\nexport { double } from \"./double.js\";\n",
        ),
        (
            "double.js",
            "// @flow\nexport function double(value: number): number {\n  return value * 2;\n}\n",
        ),
    ]);

    let mut child = std::process::Command::new(support::uf_path())
        .arg("--cwd")
        .arg(project.path())
        .args([
            "test",
            "--watch",
            "--watch-interval",
            "100",
            "probe.test.js",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("uf starts");
    let lines = lines_of(&mut child);
    let mut transcript = String::new();

    let first = wait_for(
        &lines,
        "1 passed",
        Duration::from_secs(120),
        &mut transcript,
    );
    let second = first && {
        // Past any filesystem's modification-time resolution, so the watcher
        // cannot read the edit as the file it already recorded.
        std::thread::sleep(Duration::from_millis(1_100));
        project.write(
            "double.js",
            "// @flow\nexport function double(value: number): number {\n  return value * 3;\n}\n",
        );
        wait_for(
            &lines,
            "expected 63 to be 42",
            Duration::from_secs(120),
            &mut transcript,
        )
    };
    let _ = child.kill();
    let _ = child.wait();

    assert!(first, "the first run never passed:\n{transcript}");
    assert!(second, "the edit never reached a run:\n{transcript}");
    let pids =
        std::fs::read_to_string(project.path().join(".uf").join("pids.txt")).expect("the test ran");
    let pids: Vec<&str> = pids.lines().collect();
    assert_eq!(pids.len(), 2, "one line per run: {pids:?}");
    assert_eq!(
        pids[0], pids[1],
        "the rerun started a new worker instead of keeping the one it had:\n{transcript}"
    );
}

/// A watch session nobody gave an interval hears a save from the kernel, or
/// says why it is polling instead, and reruns the edit either way.
///
/// Which of the two happens is the machine's: a sandbox that denies the
/// file-event service, and many network mounts, accept the watch and report
/// nothing, and the session finds that out with a probe before relying on
/// it. What must hold on every machine is that the edit reaches a run and the
/// announcement does not claim a poll interval it is not using.
#[test]
fn a_watch_session_with_no_interval_hears_a_save_or_says_why_it_polls() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[
        (
            "probe.test.js",
            "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\
             import { double } from \"./double.js\";\n\n\
             it(\"doubles\", () => {\n  expect(double(21)).toBe(42);\n});\n",
        ),
        (
            "double.js",
            "// @flow\nexport function double(value: number): number {\n  return value * 2;\n}\n",
        ),
    ]);

    let mut child = std::process::Command::new(support::uf_path())
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--watch", "probe.test.js"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("uf starts");
    let lines = lines_of(&mut child);
    let mut transcript = String::new();

    let watching = wait_for(
        &lines,
        "watching",
        Duration::from_secs(120),
        &mut transcript,
    );
    let rerun = watching && {
        std::thread::sleep(Duration::from_millis(1_100));
        project.write(
            "double.js",
            "// @flow\nexport function double(value: number): number {\n  return value * 3;\n}\n",
        );
        wait_for(
            &lines,
            "expected 63 to be 42",
            Duration::from_secs(120),
            &mut transcript,
        )
    };
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        watching,
        "the session never started watching:\n{transcript}"
    );
    assert!(rerun, "the edit never reached a run:\n{transcript}");
    let announced = transcript
        .lines()
        .find(|line| line.contains("watching"))
        .unwrap_or_default();
    assert!(
        !announced.contains(" every "),
        "an interval nobody chose was announced: {announced}"
    );
}
