//! The Bun half of the Capability JS Host, started for real.
//!
//! `runtime.capabilityJsHost.hosts` lists Bun, the README lists it, and
//! `docs/architecture.md` describes `packages/host/bun-preload.js` as the Bun
//! counterpart of what `register.js` is on Node. Nothing ran it. The library
//! suite runs on Node — that is what `uf test#library` means — so every claim
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
//! both tests below assert on a *finished* process rather than on its output
//! alone, and why the run is given a deadline instead of `output()`: a
//! regression in the second defect would otherwise hang `cargo test` rather
//! than fail it.
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
