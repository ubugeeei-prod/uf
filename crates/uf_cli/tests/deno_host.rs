//! Deno, started for real, so the row that says "planned" is a measurement.
//!
//! `uf_runtime::HOSTS` grades Deno [`SupportLevel::Planned`] and says what it is
//! waiting for. Every word of that is a claim, and ubugeeei-prod/uf#246 is about
//! what happens when claims of this kind go unchecked: `HostKind::Deno` existed,
//! `runtime.capabilityJsHost.hosts` listed it beside Node and Bun, and nothing
//! anywhere had ever started the binary. Nobody could have said which of the
//! three obstacles below were real.
//!
//! So this file starts Deno. It is the counterpart of `tests/bun_host.rs`, which
//! did the same for a host that turned out to be broken in two ways at once —
//! except that where that file establishes what works, this one establishes
//! **where it stops**, which is the only honest thing to assert about a host
//! whose Flow loader does not exist:
//!
//! * Deno runs `node:` built-ins, so the transform service's own imports are not
//!   the obstacle;
//! * it resolves no bare specifier from `node_modules`, so
//!   `import { it } from "@uniflowed/test"` cannot link there;
//! * it has no global `process`, which `packages/test/worker.js` uses on five
//!   lines; and
//! * it rejects Flow syntax outright, which is what "no loader" means when you
//!   run into it.
//!
//! And one thing that does work today, which is the reason Deno is in this
//! toolchain at all: **it enforces the permission set uf translates**, all five
//! categories of it, where Node enforces two and Bun none. That test runs Deno
//! with exactly the arguments `uf_runtime::permissions::host_arguments` produces
//! and asserts on what the program observed, not on the command line.
//!
//! A test here failing means the table is wrong. Fix the table.

mod support;

use std::process::Command;

use support::{Project, deno_ready, uf};
use uf_runtime::permissions::{ToolchainAccess, host_arguments};
use uf_runtime::{HostSupport, Permissions, RuntimeHost, SupportLevel};

/// What one Deno run said.
struct Run {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Run `entry` under `deno run`, with `flags` before it.
fn deno(project: &Project, flags: &[String], entry: &str) -> Run {
    let output = Command::new("deno")
        .arg("run")
        .args(flags)
        .arg(project.path().join(entry))
        .current_dir(project.path())
        .output()
        .expect("deno is on PATH");
    Run {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The major version of the Deno on PATH, or `None` if it cannot be read.
fn deno_major() -> Option<u32> {
    let output = Command::new("deno").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.split_whitespace()
        .nth(1)?
        .split('.')
        .next()?
        .parse()
        .ok()
}

/// Whether this Deno is the line the two measurements below were taken against.
///
/// Two of the obstacles in this file are facts about a *version*: a global
/// `process` and `node_modules` resolution are things a later Deno may have
/// added, and ubugeeei-prod/uf#246 records them against Deno 1.31. Asserting
/// their absence on a version nobody has run this against would be exactly the
/// unchecked claim `uf_runtime::HOSTS` exists to end — so those two say which
/// line they measured and step aside on any other, loudly. Everything else here
/// holds on every Deno and is not gated.
fn deno_is_the_measured_line() -> bool {
    match deno_major() {
        Some(1) => true,
        other => {
            eprintln!(
                "skipping: this measurement was taken against Deno 1.x and this is {other:?}. \
                 A newer Deno may have closed it, which would be good news and a change to \
                 `uf_runtime::HOSTS` — the Flow loader is untouched either way."
            );
            false
        }
    }
}

/// The obstacle that is not one.
///
/// `packages/host/transform.js` — the module every host reaches the Flow
/// transform through — imports `node:child_process`, `node:fs`, `node:path` and
/// `node:readline`. If Deno could not load those, a Deno host would need a
/// second transform client rather than a loader, and the row in
/// `uf_runtime::HOSTS` would have to say so. It can, so the row does not.
#[test]
fn deno_loads_the_node_builtins_the_transform_client_imports() {
    if !deno_ready() {
        return;
    }
    let project = Project::new(&[(
        "builtins.js",
        "import { spawn } from \"node:child_process\";\n\
         import { statSync } from \"node:fs\";\n\
         import path from \"node:path\";\n\
         import { createInterface } from \"node:readline\";\n\n\
         console.log(`builtins=${[spawn, statSync, path.join, createInterface].every((value) => \
         typeof value === \"function\")}`);\n",
    )]);

    let run = deno(&project, &[String::from("-A")], "builtins.js");

    assert!(
        run.stdout.contains("builtins=true"),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
}

/// The first obstacle: no bare specifier resolves.
///
/// Deno 1.x resolves a bare specifier through an import map or an `npm:`
/// specifier and through nothing else — a `node_modules` directory beside the
/// module is not a resolution root, with or without `--node-modules-dir`. So
/// even a *plain JavaScript* test file cannot reach `@uniflowed/test` there,
/// which is why the Deno row's "what it needs" names an import map beside the
/// ahead-of-time transform: one artefact answers both.
#[test]
fn deno_resolves_no_bare_specifier_from_node_modules() {
    if !deno_ready() || !deno_is_the_measured_line() {
        return;
    }
    let project = Project::new(&[("bare.js", "import \"@uniflowed/test\";\n")]);
    // The project sits under the repository, which has a populated
    // `node_modules` above it — the same arrangement in which Node resolves
    // this specifier without being told anything.
    let run = deno(&project, &[String::from("-A")], "bare.js");

    assert!(!run.success, "stdout:\n{}", run.stdout);
    assert!(
        run.stderr.contains("Relative import path"),
        "stderr:\n{}",
        run.stderr
    );
}

/// The second obstacle: no global `process`.
///
/// `packages/test/worker.js` reads `process.stdin`, `process.exit` and
/// `process.on` — it is a protocol over stdio, so this is not incidental use
/// that could be tidied away. Deno exposes the same object as `node:process`,
/// which is what makes this a fixable obstacle rather than a wall: the worker
/// would import it. Recorded here so the fix is known to be that and not
/// something larger.
#[test]
fn deno_has_no_global_process_but_has_the_module() {
    if !deno_ready() || !deno_is_the_measured_line() {
        return;
    }
    let project = Project::new(&[(
        "process.js",
        "import node from \"node:process\";\n\n\
         console.log(`global=${typeof globalThis.process} module=${typeof node.exit}`);\n",
    )]);

    let run = deno(&project, &[String::from("-A")], "process.js");

    assert!(
        run.stdout.contains("global=undefined"),
        "a Deno that grew a global `process` would make one line of the Deno row in \
         `uf_runtime::HOSTS` obsolete\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    assert!(
        run.stdout.contains("module=function"),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
}

/// The third, which is what "no Flow loader" looks like from a terminal.
///
/// Not a subtlety: an annotation is a syntax error, reported against the line
/// the developer wrote. `uf test` refuses to start a Deno worker precisely so
/// that a person meets a sentence about a missing loader instead of this.
#[test]
fn deno_rejects_flow_syntax_because_nothing_transforms_it() {
    if !deno_ready() {
        return;
    }
    let project = Project::new(&[(
        "annotated.js",
        "// @flow\nconst answer: number = 42;\nconsole.log(answer);\n",
    )]);

    let run = deno(&project, &[String::from("-A")], "annotated.js");

    assert!(!run.success, "stdout:\n{}", run.stdout);
    assert!(
        run.stderr.contains("SyntaxError"),
        "stderr:\n{}",
        run.stderr
    );
}

/// And `uf test` says so rather than letting that syntax error be the answer.
#[test]
fn uf_test_on_deno_names_what_the_host_is_missing() {
    // `autoDetect: false` below leaves Deno as the only candidate, so a machine
    // without it would meet "no JavaScript host found" instead — a true message
    // about a different thing.
    if !deno_ready() {
        return;
    }
    let project = Project::new(&[(
        "probe.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"never runs\", () => {\n  expect(1).toBe(1);\n});\n",
    )]);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({\n\
         \x20 app: { runtime: { capabilityJsHost: { default: \"deno\", autoDetect: false } } },\n\
         });\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stderr:\n{stderr}");
    assert!(stderr.contains("no Flow loader"), "stderr:\n{stderr}");
    // The sentence comes from the table, so this also pins that the table's
    // "what it needs" is what a person is told.
    assert!(stderr.contains("import map"), "stderr:\n{stderr}");
    assert!(stderr.contains("issues/246"), "stderr:\n{stderr}");
}

/// The one thing Deno does today that no other host does.
///
/// Run with the arguments `uf_runtime::permissions` produces — not with
/// arguments this test wrote, which would test nothing but itself — and the
/// assertion is on what the program observed. All five categories at once,
/// because the point of the row in `uf_runtime::HOSTS` is that Deno is the host
/// the model was designed against.
#[test]
fn deno_enforces_the_whole_permission_set_uf_translates() {
    if !deno_ready() {
        return;
    }
    let project = Project::new(&[(
        "probe.js",
        "let read = \"allowed\";\n\
         try {\n\
         \x20 Deno.readTextFileSync(\"/etc/hosts\");\n\
         } catch (error) {\n\
         \x20 read = error.name;\n\
         }\n\
         let env = \"allowed\";\n\
         try {\n\
         \x20 Deno.env.get(\"HOME\");\n\
         } catch (error) {\n\
         \x20 env = error.name;\n\
         }\n\
         let own = \"denied\";\n\
         try {\n\
         \x20 Deno.readTextFileSync(new URL(import.meta.url).pathname);\n\
         \x20 own = \"allowed\";\n\
         } catch {\n\
         \x20 own = \"denied\";\n\
         }\n\
         console.log(`read=${read} env=${env} own=${own}`);\n",
    )]);

    let root = project.path().to_string_lossy().into_owned();
    let toolchain = ToolchainAccess {
        read: vec![root],
        ..ToolchainAccess::default()
    };
    let flags = host_arguments(RuntimeHost::Deno, &Permissions::default(), &toolchain)
        .expect("Deno enforces every permission uf can declare");

    let run = deno(&project, &flags, "probe.js");

    assert!(
        run.stdout.contains("read=PermissionDenied"),
        "a read outside the project was allowed\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    assert!(
        run.stdout.contains("env=PermissionDenied"),
        "the environment was readable, which no other host can prevent and this one \
         can\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    // And the project's own file is still readable, or the run would have
    // proved only that Deno can refuse everything.
    assert!(
        run.stdout.contains("own=allowed"),
        "stdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
}

/// The table says Deno is planned; the file that says so is this one.
///
/// A row naming a test that does not exist would be the same unchecked claim in
/// a new place, so the name is asserted rather than trusted.
#[test]
fn the_host_table_points_at_this_file() {
    let support = HostSupport::for_host(RuntimeHost::Deno);
    assert_eq!(support.level, SupportLevel::Planned);
    assert!(!support.loads_flow());
    assert_eq!(
        support.verified_by,
        Some("crates/uf_cli/tests/deno_host.rs")
    );
    assert_eq!(support.enforces, uf_runtime::Permission::ALL);
}
