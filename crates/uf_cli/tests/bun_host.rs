//! The Bun half of the Capability JS Host, started for real.
//!
//! `runtime.capabilityJsHost.hosts` lists Bun, the README lists it, and
//! `docs/architecture.md` describes `packages/host/bun-preload.js` as the Bun
//! counterpart of what `register.js` is on Node. Nothing ran it. The library
//! suite runs on Node — that is what `uf test` means here — so every claim
//! uf makes about Bun was unchecked, and the preload was in fact broken in two
//! independent ways at once (ubugeeei-prod/uf#418):
//!
//! * its `onLoad` returned `undefined` for any module it was not responsible
//!   for, and Bun rejects that with
//!   `TypeError: onLoad() expects an object returned`. The filter was every
//!   `.js`, `.jsx` and `.mjs`, so the first ordinary dependency a project had
//!   took the process down before a line of it ran; and
//! * the transform service it starts is a child process, and a live child
//!   holds its host open. Node hides that — its module hooks run on a loader
//!   thread and the process exits with the main thread — and Bun does not, so
//!   a program that got as far as printing its output then sat there forever.
//!
//! One of those turns into the other when it is fixed on its own, which is why
//! every test below asserts on a *finished* process rather than on its output
//! alone, and why the run is given a deadline instead of `output()`: a
//! regression in the second defect would otherwise hang `cargo test` rather
//! than fail it.
//!
//! There was a third, and it is here because the fixtures for the first two
//! could not see it: `ref()` and `unref()` count on Bun where they set a flag
//! on Node, so a program with *two* imports on one line of its graph held the
//! host open however many replies had arrived. A one-import fixture transforms
//! its modules one after another and exits either way. See
//! [`two_modules_imported_at_once_do_not_hold_the_host_open`], and
//! ubugeeei-prod/uf#419 for how it was found — every program that imports
//! `@uniflowed/test` is fifteen modules, and none of them could be reached on
//! Bun at all.
//!
//! These use the real `uf` binary as the transform service and the real
//! preload, over a project on a real disk. Nothing here is a stand-in.

mod support;

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use support::{Project, bun_ready, host_ready, repo_root, uf_path};

/// How long a two-module project may take to load, transform and print.
///
/// Generous on purpose: it is not a performance assertion, it is the line
/// between "failed" and "hung", and the machine may be building something
/// else. A run that is over this has not started `uf transform` slowly, it has
/// stopped exiting.
const DEADLINE: Duration = Duration::from_secs(120);

/// What one Bun run said.
struct Run {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Run `entry` under `bun --preload @uniflowed/host/bun-preload`, or fail.
///
/// The preload is reached as a path rather than as a package specifier, so the
/// file under test is this checkout's and not whatever a resolver found.
///
/// The two streams go to files rather than pipes, and that is not tidiness: a
/// pipe holds about 64 KB and a child that fills one blocks until somebody
/// reads it. Nothing here can read while it is also watching the clock, so a
/// talkative failure would look exactly like the hang these tests exist to
/// catch — and would say so in the panic message.
fn run_on_bun(project: &Project, entry: &str) -> Run {
    let preload = repo_root().join("packages/host/bun-preload.js");
    let out_path = project.path().join("bun.stdout");
    let err_path = project.path().join("bun.stderr");
    let mut child = Command::new("bun")
        .arg("--preload")
        .arg(&preload)
        .arg(project.path().join(entry))
        .current_dir(project.path())
        .env("UF_BINARY", uf_path())
        .env("UF_PROJECT_ROOT", project.path())
        .stdout(Stdio::from(std::fs::File::create(&out_path).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&err_path).unwrap()))
        .spawn()
        .expect("bun is on PATH");

    let deadline = Instant::now() + DEADLINE;
    let status = loop {
        match child.try_wait().expect("waiting on bun") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "bun did not exit within {DEADLINE:?} after running {entry}. The program \
                     itself finishes; what keeps the process alive is the `uf transform` child \
                     the preload started, which `TransformService` unreferences between requests \
                     precisely so this cannot happen.\nstdout:\n{}\nstderr:\n{}",
                    std::fs::read_to_string(&out_path).unwrap_or_default(),
                    std::fs::read_to_string(&err_path).unwrap_or_default(),
                );
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };

    Run {
        status: status.code(),
        stdout: std::fs::read_to_string(&out_path).unwrap_or_default(),
        stderr: std::fs::read_to_string(&err_path).unwrap_or_default(),
    }
}

#[test]
fn a_bun_project_transforms_flow_and_leaves_its_dependencies_alone() {
    if !host_ready() || !bun_ready() {
        return;
    }

    let project = Project::new(&[
        (
            "thing.js",
            "// @flow\nexport const answer: number = 42;\nexport type Answer = typeof answer;\n",
        ),
        // `react` is CommonJS and is not uf's to transform, which makes it the
        // exact module the broken hook declined and took the process down on.
        // It is also why "return the file's bytes unchanged" is not the fix it
        // looks like: anything that leaves `onLoad` is an ES module to Bun,
        // so a `react` handed back through the hook has no default export and
        // this line becomes `SyntaxError: Missing 'default' export`. The
        // module never reaches the hook at all now — `FLOW_MODULE_PATTERN` is
        // what Bun filters on, and it is `isFlowModule` written as a pattern.
        (
            "main.js",
            "// @flow\nimport React from \"react\";\n\
             import { answer } from \"./thing.js\";\n\n\
             console.log(`answer=${answer} react=${typeof React.createElement}`);\n",
        ),
    ]);

    let run = run_on_bun(&project, "main.js");

    assert_eq!(
        run.status,
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    // The Flow module went through `uf transform` — a `: number` annotation
    // and a `type` alias are not JavaScript, so a Bun that ran this at all ran
    // what uf produced.
    assert!(
        run.stdout.contains("answer=42"),
        "the Flow module did not run:\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    // And the CommonJS dependency is still a CommonJS dependency.
    assert!(
        run.stdout.contains("react=function"),
        "the dependency lost its default export:\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
}

#[test]
fn one_dependency_uf_does_not_own_does_not_take_the_process_down() {
    if !host_ready() || !bun_ready() {
        return;
    }

    // The narrowest form of the original defect, and the one the issue
    // describes: a project with no Flow syntax anywhere and a single ordinary
    // dependency. There is nothing here for the transform to do and the
    // preload still killed it, which made `bun --preload` strictly worse at
    // running plain JavaScript than Bun with no preload at all.
    let project = Project::new(&[(
        "main.js",
        "import React from \"react\";\n\nconsole.log(`plain react=${typeof React.createElement}`);\n",
    )]);

    let run = run_on_bun(&project, "main.js");

    assert_eq!(
        run.status,
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    assert!(
        run.stdout.contains("plain react=function"),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
}

/// Two modules imported at once, which is the smallest thing that hangs.
///
/// `ref()` and `unref()` set a flag on Node and **count** on Bun, and
/// `TransformService` said what it wanted every time rather than only on the
/// edge. Two sibling imports are two `transform` calls, so two `ref`s; the
/// first reply leaves one request outstanding and asks to hold again, so three;
/// and the drain performs one `unref`. The host is then held for ever.
///
/// It is ubugeeei-prod/uf#418's symptom by another route, and it hid behind the
/// shape of the fixture that found that one: a program that imports a single
/// module transforms them one after another and exits, so the tests above pass
/// either way. Anything with two imports on one line of the graph does not —
/// including every program that imports `@uniflowed/test`, which is fifteen.
///
/// Three modules rather than fifteen because the count is the subject: this
/// fails on the second `ref` and would fail identically on the fiftieth, and a
/// fixture that needed the whole test package to show it would be a fixture
/// nobody could read.
#[test]
fn two_modules_imported_at_once_do_not_hold_the_host_open() {
    if !host_ready() || !bun_ready() {
        return;
    }

    let project = Project::new(&[
        ("left.js", "// @flow\nexport const left: number = 1;\n"),
        ("right.js", "// @flow\nexport const right: number = 2;\n"),
        (
            "main.js",
            "// @flow\nimport { left } from \"./left.js\";\n\
             import { right } from \"./right.js\";\n\n\
             console.log(`sum=${left + right}`);\n",
        ),
    ]);

    let run = run_on_bun(&project, "main.js");

    assert_eq!(
        run.status,
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    assert!(
        run.stdout.contains("sum=3"),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
}

/// The program that asks Bun for a module mock and prints what it was told.
///
/// It reaches for the seven bindings a person actually writes rather than only
/// `mock`, because the promise `@uniflowed/test` makes is that *none* of them
/// silently does nothing — a `unmock` that returned quietly on a host with no
/// interception would leave a suite green and unmocked.
const MOCKING_PROGRAM: &str = r#"// @flow
import { UnsupportedError, uft } from "@uniflowed/test";

// In a function rather than at the top level, and that is not a style
// preference: the Flow parser uf vendors does not parse top-level `await`, and
// `uf transform` says so rather than guessing. A floating promise is right here
// — every step is real asynchronous work, so the host stays alive until it
// settles, and a rejection ends the process non-zero, which is what this test
// reads.
async function ask() {
  for (const [name, call] of [
    ["mock", () => uft.mock("./client.js", () => ({ send: () => "stand-in" }))],
    ["doMock", () => uft.doMock("./client.js", () => ({ send: () => "stand-in" }))],
    ["unmock", () => uft.unmock("./client.js")],
    ["doUnmock", () => uft.doUnmock("./client.js")],
    ["importActual", () => uft.importActual("./client.js")],
    ["importMock", () => uft.importMock("./client.js")],
    ["resetModules", () => uft.resetModules()],
  ]) {
    try {
      await call();
      console.log(`${name}=returned`);
    } catch (error) {
      const named = error instanceof UnsupportedError;
      console.log(`${name}=${named ? "UnsupportedError" : "other"}`);
      if (name === "mock") console.log(`reason=${error.message}`);
    }
  }
  // And the module is the module: nothing stood in for it.
  console.log(`client=${(await import("./client.js")).send()}`);
}

ask();
"#;

/// What `uft.mock` does on Bun, which is refuse and say why.
///
/// ubugeeei-prod/uf#283 shipped module mocking on Node and raised
/// `UnsupportedError` on Bun; #419 is the other half, and it waited for this
/// file so that a Bun code path would not be written unexercised. Running it
/// here is what says the *unsupported* half is exercised too — until now
/// nothing had ever started Bun and asked, so "Bun raises a useful error" was
/// as unchecked as every other Bun claim in ubugeeei-prod/uf#418.
///
/// It is a real error rather than a stub. `packages/host/module-mocks.js`
/// records what a Bun 1.1 plugin does with each of the three places a stand-in
/// could be given an identity of its own, and none of them survives an `import`
/// declaration — which is the case the feature is for. A mock that only a
/// dynamic import could see, or one that reached back into modules already
/// evaluated the way Bun's own `mock.module` does, would be the same API
/// meaning two things on two hosts. So this asserts the message, because the
/// message is what a person on Bun actually gets.
#[test]
fn module_mocking_on_bun_refuses_by_name_rather_than_doing_nothing() {
    if !host_ready() || !bun_ready() {
        return;
    }

    let project = Project::new(&[
        (
            "client.js",
            "// @flow\nexport const send = (): string => \"real\";\n",
        ),
        ("main.js", MOCKING_PROGRAM),
    ]);

    let run = run_on_bun(&project, "main.js");

    assert_eq!(
        run.status,
        Some(0),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    for expected in [
        "mock=UnsupportedError",
        "doMock=UnsupportedError",
        "unmock=UnsupportedError",
        "doUnmock=UnsupportedError",
        "importActual=UnsupportedError",
        "importMock=UnsupportedError",
        "resetModules=UnsupportedError",
        // Not "not implemented": the host, read off the runtime rather than
        // guessed, the hook it would need, and the two things that do work
        // here instead.
        "reason=",
        "Bun does",
        "registerHooks",
        "uft.spyOn",
        "@uniflowed/mock",
        // And nothing stood in for the module, which is the other half of
        // "refuse" — a binding that threw and mocked anyway would be worse
        // than one that did neither.
        "client=real",
    ] {
        assert!(
            run.stdout.contains(expected),
            "missing {expected:?}\nstdout:\n{}\nstderr:\n{}",
            run.stdout,
            run.stderr
        );
    }
}

/// The preload is a file this repository ships, and it has to be reachable
/// under the name the documentation gives.
#[test]
fn the_preload_is_where_the_documentation_says_it_is() {
    let preload: &Path = &repo_root().join("packages/host/bun-preload.js");
    assert!(
        preload.is_file(),
        "`bun --preload @uniflowed/host/bun-preload` names {}",
        preload.display()
    );
}
