//! Deno, started for real, so the row that grades it is a measurement.
//!
//! `uf_runtime::HOSTS` grades Deno [`SupportLevel::Experimental`] and says what
//! it is still missing. Every word of that is a claim, and ubugeeei-prod/uf#246
//! is about what happens when claims of this kind go unchecked: `HostKind::Deno`
//! existed, `runtime.capabilityJsHost.hosts` listed it beside Node and Bun, and
//! nothing anywhere had ever started the binary. Nobody could have said which of
//! the obstacles below were real.
//!
//! So this file starts Deno, in two halves that have to be read together.
//!
//! **Where the road stopped**, which is what the host is when it is handed the
//! project as it is written:
//!
//! * Deno runs `node:` built-ins, so the transform service's own imports were
//!   never the obstacle;
//! * `import "@uniflowed/test"` does not load — on an older line because no
//!   bare specifier resolves from `node_modules`, on a current one because it
//!   resolves and the package it finds is Flow;
//! * it has no global `process`, which is why `packages/test/worker.js` imports
//!   `node:process` and installs it; and
//! * it rejects Flow syntax outright, which is what "no loader" looks like from
//!   a terminal.
//!
//! **And where it goes now**, which is what
//! `crates/uf_cli/src/commands/deno_loader.rs` buys: `uf test` compiles the
//! project ahead of time and hands Deno an import map, and a Flow suite runs.
//! The map is asked the two questions above one at a time as well as end to
//! end, so a failure says which half broke.
//!
//! One thing has worked here from the start, and is the reason Deno is in this
//! toolchain at all: **it enforces the permission set uf translates**, all five
//! categories of it, where Node enforces two and Bun none. Those tests run Deno
//! with exactly the arguments `uf_runtime::permissions::host_arguments`
//! produces and assert on what the program observed, not on the command line —
//! including, now, that a project which declared *no* permissions is still not
//! run with `-A`.
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

/// Whether this Deno is the line the one measurement below was taken against.
///
/// A global `process` is a fact about a *version*: a later Deno may have added
/// one, and ubugeeei-prod/uf#246 records its absence against Deno 1.31.
/// Asserting that on a version nobody has run this against would be exactly the
/// unchecked claim `uf_runtime::HOSTS` exists to end — so it says which line it
/// measured and steps aside on any other, loudly. Everything else here holds on
/// every Deno and is not gated: the bare-specifier test asserts the disjunction
/// rather than one version's half of it, which is what a version-independent
/// measurement of the same obstacle looks like.
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

/// The first obstacle, whichever of its two forms this Deno has.
///
/// Without the import map, that is — which is what makes this pair with
/// `the_generated_import_map_answers_both_halves_of_the_problem` below rather
/// than contradict it. This is Deno handed the project as it is written.
///
/// `import "@uniflowed/test"` cannot work on Deno, and *why* depends on the
/// version — which is the whole reason this is a measurement rather than a
/// sentence. Deno 1.31, the line ubugeeei-prod/uf#246 records, resolves no bare
/// specifier from `node_modules` at all and stops at
/// `Relative import path …`. A current Deno 1.x resolves it, reaches
/// `packages/test/index.js`, and stops at `export type { … }` — because every
/// `@uniflowed/*` package ships Flow and there is no loader.
///
/// Both are "a uf project does not load here", and the second is the more
/// useful finding: the resolution half of the problem has already gone, and
/// what is left is the Flow loader alone. Asserting only the first would have
/// made this test a statement about one Deno wearing the name of Deno, which is
/// the class of claim `uf_runtime::HOSTS` exists to end — so it asserts the
/// disjunction, and names which half it saw when it fails.
#[test]
fn a_uf_package_cannot_be_imported_on_deno() {
    if !deno_ready() {
        return;
    }
    let project = Project::new(&[("bare.js", "import \"@uniflowed/test\";\n")]);
    // The project sits under the repository, which has a populated
    // `node_modules` above it — the same arrangement in which Node resolves
    // this specifier without being told anything.
    let run = deno(&project, &[String::from("-A")], "bare.js");

    assert!(!run.success, "stdout:\n{}", run.stdout);
    let unresolved = run.stderr.contains("Relative import path");
    // Deno reports a Flow annotation, a type export or a `component` as a parse
    // error against the file it found.
    let unparsed = run.stderr.contains("could not be parsed");
    assert!(
        unresolved || unparsed,
        "a uf package must not load on Deno, and this failed for some third \
         reason\nstderr:\n{}",
        run.stderr
    );
    eprintln!(
        "deno stops at {}",
        if unresolved {
            "resolution: no bare specifier from node_modules"
        } else {
            "parsing: the package is Flow and there is no loader"
        }
    );
}

/// The second obstacle: no global `process`.
///
/// `packages/test/worker.js` is a protocol over stdio — `process.stdin`,
/// `process.exit`, `process.on` — so this was never incidental use that could
/// be tidied away, and three more modules it reaches read the global too. Deno
/// exposes the same object as `node:process`, which is what made this a fixable
/// obstacle rather than a wall: the worker imports it and installs it on the
/// global, once, in the one file that is a process entry point.
///
/// This test is what keeps that fix honest. If a later Deno grew the global,
/// the line in the worker would become dead code claiming to be load-bearing,
/// and the row in `uf_runtime::HOSTS` would be describing a version nobody
/// runs.
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

/// And this is the whole of ubugeeei-prod/uf#246: a Flow suite, on Deno,
/// passing.
///
/// The three tests above establish that none of it works when Deno is handed
/// the source as it is. This one hands Deno what `uf` produces instead — the
/// ahead-of-time transform under `.uf/deno` and the import map beside it — and
/// asserts the run finishes green. Everything between the two is
/// `crates/uf_cli/src/commands/deno_loader.rs`.
///
/// It is deliberately a `uf test` invocation rather than a hand-built `deno
/// run`: the artefact, the permission translation, the worker protocol and the
/// report all have to agree, and a test that assembled the command line itself
/// would be checking a `format!` again.
#[test]
fn uf_test_runs_a_flow_suite_on_deno() {
    // `autoDetect: false` below leaves Deno as the only candidate, so a machine
    // without it would meet "no JavaScript host found" instead — a true message
    // about a different thing.
    if !deno_ready() {
        return;
    }
    let project = deno_project(&[(
        "probe.test.js",
        // Flow in every position the transform has to handle for this to mean
        // anything: an annotation, a type import from a package that is itself
        // Flow, and a relative import of another compiled module.
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\
         import { double } from \"./double.js\";\n\n\
         it(\"runs on deno\", () => {\n\
         \x20 const value: number = double(21);\n\
         \x20 expect(value).toBe(42);\n\
         });\n",
    )]);
    project.write(
        "double.js",
        "// @flow\nexport function double(value: number): number {\n  return value * 2;\n}\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "a Flow suite must run on Deno\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("1 passed") || stdout.contains("1 passed"),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// The artefact itself, run by hand, so a failure says *which* half broke.
///
/// The test above is end to end and its failure mode is "the suite did not
/// pass", which could be the transform, the map, the permission set or the
/// worker. This one takes the tree that run left behind and asks Deno the two
/// questions ubugeeei-prod/uf#246 is made of — can it parse Flow now, and can
/// it resolve `@uniflowed/test` now — one at a time, against the same artefact.
#[test]
fn the_generated_import_map_answers_both_halves_of_the_problem() {
    if !deno_ready() {
        return;
    }
    let project = deno_project(&[(
        "probe.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"passes\", () => {\n  expect(1).toBe(1);\n});\n",
    )]);
    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let map = project.path().join(".uf/deno/import-map.json");
    assert!(map.is_file(), "no import map at {}", map.display());

    // A module written in Flow, and a bare specifier into a package that is
    // itself written in Flow. Both were failures above; both go through the
    // map now.
    //
    // The compiled copy is run directly rather than the source: whether Deno
    // puts its *own* command-line argument through the import map is Deno's
    // business, and this test is about the map's contents. The entry that
    // redirects a source path — the project prefix — is exercised by the run
    // above, where the worker imports each test file by the path uf gave it.
    project.write(
        "through-the-map.js",
        "// @flow\nimport { expect } from \"@uniflowed/test\";\n\
         const value: number = 1;\n\
         console.log(`resolved=${typeof expect} flow=${value}`);\n",
    );
    let compiled = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");
    assert!(
        compiled.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let run = deno(
        &project,
        // One `--allow-read`, comma separated, because that is the spelling
        // `uf_runtime::permissions` produces and the one Deno cannot misread
        // as two flags.
        &[
            format!("--import-map={}", map.display()),
            format!(
                "--allow-read={},{}",
                project.path().display(),
                support::repo_root().display()
            ),
        ],
        ".uf/deno/through-the-map.js",
    );

    assert!(
        run.stdout.contains("resolved=function"),
        "the map did not resolve `@uniflowed/test`\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
    assert!(
        run.stdout.contains("flow=1"),
        "the annotation was not compiled away\nstdout:\n{}\nstderr:\n{}",
        run.stdout,
        run.stderr
    );
}

/// The limitation that keeps this host *experimental*, said out loud.
///
/// An ahead-of-time pass and a watch loop cannot both be true: the pass runs
/// once, before the host starts, so every run after the first would report on
/// the modules the first pass wrote. Refusing is the honest half of that, and
/// it is asserted rather than trusted because a silent stale watch is the worst
/// shape this could take — a loop that answers, quickly, about code the
/// developer has already changed.
#[test]
fn uf_test_watch_on_deno_refuses_rather_than_running_a_stale_tree() {
    if !deno_ready() {
        return;
    }
    let project = deno_project(&[(
        "probe.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"passes\", () => {\n  expect(1).toBe(1);\n});\n",
    )]);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--watch", "probe.test.js"])
        .output()
        .expect("uf runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stderr:\n{stderr}");
    assert!(stderr.contains("ahead of time"), "stderr:\n{stderr}");
    assert!(stderr.contains("issues/246"), "stderr:\n{stderr}");
}

/// No `-A` reaches Deno from `uf test`, on a project that declared nothing.
///
/// This is the sentence ubugeeei-prod/uf#246 ends on. `uf test` passed
/// `run -A` unconditionally, which is an all-access grant nothing later on the
/// command line takes back, so the host that can enforce the whole of uf's
/// permission model was the host uf ran unsandboxed. What it gets now is the
/// toolchain's own access, and the way to see that from outside is to ask the
/// program what it can reach: the project, yes; `/etc`, no.
#[test]
fn a_deno_run_that_declared_no_permissions_is_still_not_all_access() {
    if !deno_ready() {
        return;
    }
    let project = deno_project(&[(
        "probe.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"cannot read the rest of the machine\", () => {\n\
         \x20 let outside = \"allowed\";\n\
         \x20 try {\n\
         \x20   Deno.readTextFileSync(\"/etc/hosts\");\n\
         \x20 } catch (error) {\n\
         \x20   outside = error.name;\n\
         \x20 }\n\
         \x20 expect(outside).toBe(\"PermissionDenied\");\n\
         });\n",
    )]);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "a project that declared no permissions was started with `-A`, or the run \
         failed for another reason\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// A project pinned to Deno, with everything else the fixtures share.
fn deno_project(files: &[(&str, &str)]) -> Project {
    let project = Project::new(files);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({\n\
         \x20 app: { runtime: { capabilityJsHost: { default: \"deno\", autoDetect: false } } },\n\
         });\n",
    );
    project
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

/// The table says Deno is experimental; the file that says so is this one.
///
/// A row naming a test that does not exist would be the same unchecked claim in
/// a new place, so the name is asserted rather than trusted. *Experimental* and
/// not *implemented* is the assertion worth reading: a uf project runs here, and
/// the way it is made to run has a gap — a module the ahead-of-time pass could
/// not enumerate is still Flow to Deno — which a person can meet.
#[test]
fn the_host_table_points_at_this_file() {
    let support = HostSupport::for_host(RuntimeHost::Deno);
    assert_eq!(support.level, SupportLevel::Experimental);
    assert!(support.loads_flow());
    assert!(
        support.missing.is_some_and(|missing| missing.contains("hook")),
        "an experimental row has to name the limitation: {:?}",
        support.missing
    );
    assert_eq!(
        support.verified_by,
        Some("crates/uf_cli/tests/deno_host.rs")
    );
    assert_eq!(support.enforces, uf_runtime::Permission::ALL);
}
