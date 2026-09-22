//! Deno, started for real, so the row that grades it is a measurement.
//!
//! `uf_runtime::HOSTS` grades Deno [`SupportLevel::Implemented`] and names its
//! loader: `@uniflowed/host/deno-preload`, which installs uf's transform through
//! `node:module`'s `registerHooks` — a hook Deno implemented in 2.8. Every word
//! of that is a claim, and ubugeeei-prod/uf#246 is about what happens when
//! claims of this kind go unchecked: `HostKind::Deno` existed, listed beside
//! Node and Bun, while nothing anywhere had ever started the binary.
//!
//! So this file starts Deno, in three halves that have to be read together.
//!
//! **Where the road stops without the loader**, which is what the host is when
//! it is handed the project as it is written: Deno runs the `node:` built-ins
//! the transform client imports, and it can neither import `@uniflowed/test` nor
//! parse a Flow annotation.
//!
//! **What the hook buys**, asked of the preload directly and then through
//! `uf test`. A module is compiled as Deno asks for it — which includes the
//! three kinds of module the ahead-of-time pass uf's Deno loader was before 2.8
//! could not reach: a path computed at run time, a re-import under a fresh
//! query (which is what `uf test --watch` does after an edit), and a module a
//! mock stands in for. And the compiled modules are the Node loader's: one
//! cache, one key, one framing.
//!
//! **What Deno enforces**: the permission set uf translates, all five
//! categories of it, where Node enforces two and Bun none — including for a
//! project that declared no set at all, which is still not run with `-A`.
//!
//! A test here failing means the table is wrong. Fix the table.
//!
//! With one exception, which is Deno's rather than the table's. Deno 2.9.6
//! sometimes aborts inside V8 as it starts — `Fatal error in :0: unreachable
//! code`, under its own "Deno has panicked" banner — and when it does, it takes
//! several of these tests down in one CI job and none in the job beside it, on
//! the same tree (ubugeeei-prod/uf#1132). So each test here that starts Deno
//! runs inside [`once_more_if_v8_aborts`]: a failure whose output holds that
//! abort runs a second time, announced on stderr and as a warning on the CI
//! run. Every other failure — uf's loader, uf's permission set, a failed
//! expectation, a panic in Deno's own Rust code — is the test's the first time,
//! and an abort that happens twice is the test's too.

mod support;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use support::{Project, deno_ready, host_ready, repo_root, uf, uf_path};
use uf_runtime::permissions::{ToolchainAccess, host_arguments};
use uf_runtime::{HostSupport, Permission, Permissions, RuntimeHost, SupportLevel};

/// What one Deno run said.
struct Run {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Run `entry` under `deno run`, with `flags` before it and `env` set.
///
/// Without the dynamic-loader variables this process inherited, which is what
/// `uf test` does for its own Deno workers: `cargo test` sets `LD_LIBRARY_PATH`
/// on Linux, and Deno will not start `uf transform` under a scoped
/// `--allow-run` while one is set. A test that means to pass one names it in
/// `env`, which is applied after.
fn deno(project: &Project, flags: &[String], entry: &str, env: &[(&str, String)]) -> Run {
    let mut command = Command::new("deno");
    for (name, _) in std::env::vars_os() {
        if name
            .to_str()
            .is_some_and(|name| name.starts_with("LD_") || name.starts_with("DYLD_"))
        {
            command.env_remove(name);
        }
    }
    let output = command
        .arg("run")
        .args(flags)
        .arg(project.path().join(entry))
        .current_dir(project.path())
        .envs(env.iter().map(|(name, value)| (*name, value.as_str())))
        .output()
        .expect("deno is on PATH");
    Run {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Run a test that starts Deno, and run it once more if V8 aborted inside Deno.
///
/// The module documentation says why, and ubugeeei-prod/uf#1132 has the runs.
/// The second run is announced where the test harness cannot swallow it — a
/// test that passes has its captured output thrown away — so how often Deno
/// does this stays something every CI run shows.
fn once_more_if_v8_aborts(body: impl Fn()) {
    let test = std::thread::current()
        .name()
        .unwrap_or("a Deno host test")
        .to_owned();
    retry_once_when(&body, is_a_v8_abort_inside_deno, |failure| {
        announce_second_run(&test, failure);
    });
}

/// Run `body`, and if it panicked with a message `retryable` accepts, tell
/// `announce` and run `body` once more. Any other panic, and any panic of the
/// second run, is the caller's.
fn retry_once_when(body: &dyn Fn(), retryable: fn(&str) -> bool, announce: impl FnOnce(&str)) {
    let Err(payload) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) else {
        return;
    };
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or_default();
    if !retryable(message) {
        std::panic::resume_unwind(payload);
    }
    announce(message);
    body();
}

/// Whether `output` holds V8 aborting inside Deno: Deno's panic banner, then a
/// panic, then V8's `Fatal error in …` as that panic's message.
///
/// Narrow on purpose. A test's failure message carries the output of the Deno
/// it started, directly or through `uf`, and only this earns a second run. A
/// `SyntaxError`, a `NotCapable`, a failed expectation and a run that timed out
/// do not, and neither does a panic in Deno's own Rust code, which what uf
/// hands Deno can cause.
fn is_a_v8_abort_inside_deno(output: &str) -> bool {
    let mut lines = output.lines();
    lines.any(|line| line.contains("Deno has panicked. This is a bug in Deno."))
        && lines.any(|line| line.contains(" panicked at "))
        && lines.any(|line| line.contains("Fatal error in "))
}

/// Say that `test` runs a second time, and why, where it stays visible: on the
/// real stderr, with the banner Deno printed, and on a CI run as a warning.
fn announce_second_run(test: &str, failure: &str) {
    let banner = failure
        .find("Deno has panicked")
        .map_or(failure, |at| &failure[at..])
        .lines()
        .take(20)
        .collect::<Vec<_>>();
    let version = banner
        .iter()
        .find_map(|line| line.trim_start().strip_prefix("Version: "))
        .unwrap_or("of an unknown version");
    let fatal = banner
        .iter()
        .find_map(|line| line.find("Fatal error in ").map(|at| &line[at..]))
        .unwrap_or("Fatal error");
    // The handles rather than `eprintln!`: the test harness captures what the
    // macros print, and drops it for a test that passes.
    let _ = writeln!(
        std::io::stderr().lock(),
        "\n{test}: Deno {version} aborted inside V8 ({fatal}), so the test runs a second time \
         (ubugeeei-prod/uf#1132). What Deno printed:\n{}\n",
        banner.join("\n")
    );
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        let message = format!(
            "{test} ran a second time because Deno {version} aborted inside V8 with `{fatal}` \
             (ubugeeei-prod/uf#1132)"
        )
        .replace('%', "%25");
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "::warning title=Deno aborted inside V8::{message}");
        let _ = stdout.flush();
    }
}

/// The `major.minor` of the Deno on PATH, or `None` if it cannot be read.
fn deno_version() -> Option<(u32, u32)> {
    let output = Command::new("deno").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut numbers = text.split_whitespace().nth(1)?.split(['.', '-', '+']);
    Some((numbers.next()?.parse().ok()?, numbers.next()?.parse().ok()?))
}

/// Whether the Deno on PATH has the hook the loader is built on.
///
/// Asked after [`deno_ready`] by every test that loads Flow. A Deno older than
/// 2.8 cannot run a uf project — `uf test` refuses it by version, which
/// `uf_test_refuses_a_deno_older_than_the_hook` checks without needing one —
/// so these tests have nothing to measure there. They fail rather than skip
/// unless skipping is allowed, for the reason `deno_ready` gives: a host claim
/// whose check quietly did not run is a claim nobody checked.
fn deno_with_hooks() -> bool {
    match deno_version() {
        Some(version) if version >= (2, 8) => true,
        other => {
            assert!(
                std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
                "this test needs Deno 2.8 or newer, which has `registerHooks`, and the Deno on \
                 PATH is {other:?}"
            );
            eprintln!("skipping: the Deno on PATH is {other:?}, which predates `registerHooks`");
            false
        }
    }
}

/// The preload, in this checkout.
fn preload() -> String {
    repo_root()
        .join("packages/host/deno-preload.js")
        .to_string_lossy()
        .into_owned()
}

/// The flags a Deno worker is started with: the preload, then the permission
/// set `uf_runtime::permissions` translates from the toolchain's own access.
///
/// `run_uf` is whether that access includes starting `uf transform`. A run
/// refused it can load only what the transform cache already holds, which is
/// how a test below proves a warm run compiles nothing.
fn loader_flags(project: &Project, run_uf: bool) -> Vec<String> {
    let root = project.path().to_string_lossy().into_owned();
    let mut toolchain = ToolchainAccess {
        read: vec![
            root.clone(),
            repo_root().to_string_lossy().into_owned(),
            uf_path().to_owned(),
        ],
        write: vec![format!("{root}/.uf")],
        env: [
            "UF_BINARY",
            "UF_PROJECT_ROOT",
            "UF_IN_SOURCE_TESTS",
            "PATH",
            "NODE_V8_COVERAGE",
        ]
        .map(String::from)
        .to_vec(),
        ..ToolchainAccess::default()
    };
    if run_uf {
        toolchain.run.push(uf_path().to_owned());
    }
    let mut flags = vec![String::from("--preload"), preload()];
    flags.extend(
        host_arguments(RuntimeHost::Deno, &Permissions::default(), &toolchain)
            .expect("Deno enforces every permission uf can declare"),
    );
    flags
}

/// The two variables every uf-started host is given.
fn loader_env(project: &Project) -> Vec<(&'static str, String)> {
    vec![
        ("UF_BINARY", uf_path().to_owned()),
        (
            "UF_PROJECT_ROOT",
            project.path().to_string_lossy().into_owned(),
        ),
    ]
}

const DOUBLE: &str =
    "// @flow\nexport function double(value: number): number {\n  return value * 2;\n}\n";

const COMPUTED: &str = "// @flow\nexport const seven: number = 7;\n";

/// A Flow module that reaches every kind of import the hook has to answer.
const EVERY_IMPORT: &str = r#"// @flow
import { expect } from "@uniflowed/test";
import { double } from "./double.js";

const value: number = double(21);
expect(value).toBe(42);
console.log(`static=${value} package=${typeof expect}`);

// A path no reading of this file could have listed in advance.
const name = ["./", "computed", ".js"].join("");
const computed = await import(new URL(name, import.meta.url).href);
console.log(`computed=${computed.seven}`);

// The worker's own cache-busting re-import, which is what a watch run does.
const again = await import(new URL("./double.js?uf-run=2", import.meta.url).href);
console.log(`query=${again.double(4)}`);
"#;

/// A Flow module whose only imports are relative ones.
const RELATIVE_ONLY: &str = "// @flow\nimport { double } from \"./double.js\";\n\nconst value: number = double(21);\nconsole.log(`double=${value}`);\n";

/// The obstacle that is not one.
///
/// `packages/host/transform.js` — the module every host reaches the Flow
/// transform through — imports `node:child_process`, `node:fs`, `node:path` and
/// `node:readline`, and Deno's loader calls its `spawnSync`. If Deno could not
/// load those, a Deno host would need a second transform client rather than a
/// loader. It can, so it does not.
#[test]
fn deno_loads_the_node_builtins_the_transform_client_imports() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() {
            return;
        }
        let project = Project::new(&[(
            "builtins.js",
            "import { spawn, spawnSync } from \"node:child_process\";\n\
         import { statSync } from \"node:fs\";\n\
         import path from \"node:path\";\n\
         import { createInterface } from \"node:readline\";\n\n\
         console.log(`builtins=${[spawn, spawnSync, statSync, path.join, createInterface].every((value) => \
         typeof value === \"function\")}`);\n",
        )]);

        let run = deno(&project, &[String::from("-A")], "builtins.js", &[]);

        assert!(
            run.stdout.contains("builtins=true"),
            "stdout:\n{}\nstderr:\n{}",
            run.stdout,
            run.stderr
        );
    });
}

/// Without the loader, a uf package does not load on Deno.
///
/// Deno resolves `@uniflowed/test` from `node_modules` and reaches
/// `packages/test/index.js`, which is Flow. Deno 1.31, the line
/// ubugeeei-prod/uf#246 was filed against, stopped a step earlier and resolved
/// no bare specifier at all; the disjunction is asserted so the test measures
/// the obstacle rather than one version's spelling of it.
#[test]
fn a_uf_package_cannot_be_imported_on_deno_without_the_loader() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() {
            return;
        }
        let project = Project::new(&[("bare.js", "import \"@uniflowed/test\";\n")]);
        let run = deno(&project, &[String::from("-A")], "bare.js", &[]);

        assert!(!run.success, "stdout:\n{}", run.stdout);
        let unresolved = run.stderr.contains("Relative import path");
        let unparsed =
            run.stderr.contains("could not be parsed") || run.stderr.contains("SyntaxError");
        assert!(
            unresolved || unparsed,
            "a uf package must not load on Deno without the loader, and this failed for some third \
         reason\nstderr:\n{}",
            run.stderr
        );
    });
}

/// What "no Flow loader" looks like from a terminal: a syntax error against
/// the line the developer wrote.
#[test]
fn deno_rejects_flow_syntax_without_the_loader() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() {
            return;
        }
        let project = Project::new(&[(
            "annotated.js",
            "// @flow\nconst answer: number = 42;\nconsole.log(answer);\n",
        )]);

        let run = deno(&project, &[String::from("-A")], "annotated.js", &[]);

        assert!(!run.success, "stdout:\n{}", run.stdout);
        assert!(
            run.stderr.contains("SyntaxError"),
            "stderr:\n{}",
            run.stderr
        );
    });
}

/// A dynamic-loader variable in Deno's environment is named in the error.
///
/// Deno will not let a process whose `--allow-run` names programs start one
/// while `LD_LIBRARY_PATH` or another `LD_*` or `DYLD_*` variable is set, and
/// `node:child_process` answers that refusal with no process and no reason.
/// `uf test` leaves such variables out of its workers; a Deno somebody starts
/// with the preload themselves has to have them unset, and this is what tells
/// them which one. A fresh project, so its cache is empty and the loader has to
/// start `uf transform`.
#[test]
fn a_loader_variable_deno_refuses_is_named() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = Project::new(&[("entry.js", RELATIVE_ONLY), ("double.js", DOUBLE)]);
        let mut env = loader_env(&project);
        env.push((
            "LD_LIBRARY_PATH",
            String::from("/nonexistent/uf-deno-host-test"),
        ));

        let run = deno(&project, &loader_flags(&project, true), "entry.js", &env);

        assert!(
            !run.success,
            "stdout:\n{}\nstderr:\n{}",
            run.stdout, run.stderr
        );
        assert!(
            run.stderr.contains("LD_LIBRARY_PATH"),
            "the error has to name the variable in the way\nstderr:\n{}",
            run.stderr
        );
    });
}

/// And with it, every kind of import compiles — under the permission set uf
/// translates, not under `-A`.
///
/// A static relative import and a bare specifier into a package that is itself
/// Flow are what any loader has to do. The other two are what the ahead-of-time
/// pass this replaced could not: a path computed at run time, which no scan
/// could have listed, and a re-import with a query on it, which is how the test
/// worker gets a fresh module on the next run of a watch session.
#[test]
fn the_preload_compiles_every_module_deno_asks_for() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = Project::new(&[
            ("entry.js", EVERY_IMPORT),
            ("double.js", DOUBLE),
            ("computed.js", COMPUTED),
        ]);

        let run = deno(
            &project,
            &loader_flags(&project, true),
            "entry.js",
            &loader_env(&project),
        );

        assert!(
            run.success,
            "stdout:\n{}\nstderr:\n{}",
            run.stdout, run.stderr
        );
        for expected in ["static=42 package=function", "computed=7", "query=8"] {
            assert!(
                run.stdout.contains(expected),
                "{expected}\nstdout:\n{}\nstderr:\n{}",
                run.stdout,
                run.stderr
            );
        }
        // Written under the one writable path the set grants, which is the cache
        // the next run reads.
        let cached = std::fs::read_dir(project.path().join(".uf/cache/transform"))
            .map(|entries| entries.count())
            .unwrap_or(0);
        assert!(cached > 0, "the hook compiled modules and cached none");
    });
}

/// A warm run starts no `uf transform` at all.
///
/// The second run is refused the right to start one — no `--allow-run` — so
/// if the hook reached for the compiler instead of the cache, Deno would answer
/// `NotCapable` and the run would fail. It passing is the cache being read
/// first, which is the property that makes a per-module compiler process cheap
/// enough to be the design.
#[test]
fn a_module_already_compiled_is_read_rather_than_compiled_again() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = Project::new(&[("entry.js", RELATIVE_ONLY), ("double.js", DOUBLE)]);

        let cold = deno(
            &project,
            &loader_flags(&project, true),
            "entry.js",
            &loader_env(&project),
        );
        assert!(cold.success, "stderr:\n{}", cold.stderr);

        let warm = deno(
            &project,
            &loader_flags(&project, false),
            "entry.js",
            &loader_env(&project),
        );
        assert!(
            warm.success && warm.stdout.contains("double=42"),
            "a warm run reached for `uf transform`\nstdout:\n{}\nstderr:\n{}",
            warm.stdout,
            warm.stderr
        );
    });
}

/// Node and Deno share one transform cache, and agree on what is in it.
///
/// `packages/host/internal/flow-cache.js` is one key and one framing for
/// both loaders. So a module the *Node* loader compiled is one Deno reads
/// without compiling — which it has to, because this Deno run may not start
/// the compiler. Two copies of the key that had drifted would fail here as a
/// `NotCapable`, rather than in somebody's project as a cache that two hosts
/// keep overwriting.
#[test]
fn a_module_the_node_loader_compiled_is_one_deno_reads() {
    once_more_if_v8_aborts(|| {
        if !host_ready() || !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = Project::new(&[("entry.js", RELATIVE_ONLY), ("double.js", DOUBLE)]);

        let node = Command::new("node")
            .args(["--import", "@uniflowed/host/register"])
            .arg(project.path().join("entry.js"))
            .current_dir(project.path())
            .env("UF_BINARY", uf_path())
            .env("UF_PROJECT_ROOT", project.path())
            .env_remove("UF_IN_SOURCE_TESTS")
            .output()
            .expect("node runs");
        assert!(
            node.status.success(),
            "stderr:\n{}",
            String::from_utf8_lossy(&node.stderr)
        );

        let run = deno(
            &project,
            &loader_flags(&project, false),
            "entry.js",
            &loader_env(&project),
        );
        assert!(
            run.success && run.stdout.contains("double=42"),
            "Deno did not find what Node compiled\nstdout:\n{}\nstderr:\n{}",
            run.stdout,
            run.stderr
        );
    });
}

/// And this is the whole of ubugeeei-prod/uf#246's Deno half: a Flow suite,
/// on Deno, passing — through `uf test`, so the preload, the permission
/// translation, the worker protocol and the report all have to agree.
#[test]
fn uf_test_runs_a_flow_suite_on_deno() {
    once_more_if_v8_aborts(|| {
        // `autoDetect: false` below leaves Deno as the only candidate, so a machine
        // without it would meet "no JavaScript host found" instead — a true message
        // about a different thing.
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = deno_project(&[(
            "probe.test.js",
            // Flow in every position the transform has to handle for this to mean
            // anything: an annotation, a type import from a package that is itself
            // Flow, a relative import of another compiled module, and one reached
            // by a path the file computes.
            r#"// @flow
import { expect, it } from "@uniflowed/test";
import { double } from "./double.js";

it("runs on deno", () => {
  const value: number = double(21);
  expect(value).toBe(42);
});

it("compiles a module reached by a computed path", async () => {
  const name = ["./", "computed", ".js"].join("");
  const computed = await import(new URL(name, import.meta.url).href);
  expect(computed.seven).toBe(7);
});
"#,
        )]);
        project.write("double.js", DOUBLE);
        project.write("computed.js", COMPUTED);

        let output = uf()
            .arg("--cwd")
            .arg(project.path())
            .args(["test", "probe.test.js"])
            // What `cargo test` sets on Linux for the process it tests, set here so
            // the condition is the same on every machine. Deno refuses to start a
            // child under a scoped `--allow-run` while it is in the environment,
            // so a worker that inherited it could compile nothing; `uf test` has to
            // leave it out. A directory that does not exist, so it changes nothing
            // else about the processes that do see it.
            .env("LD_LIBRARY_PATH", "/nonexistent/uf-deno-host-test")
            .output()
            .expect("uf runs");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "a Flow suite must run on Deno, with a loader variable in uf's environment\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert!(
            stderr.contains("2 passed") || stdout.contains("2 passed"),
            "stdout:\n{stdout}\nstderr:\n{stderr}"
        );
    });
}

#[test]
fn uf_test_host_override_reports_deno_and_preserves_config() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = deno_project(&[(
            "probe.test.js",
            "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"passes\", () => {\n  expect(1).toBe(1);\n});\n",
        )]);

        let config = "export default { test: { runtime: \"node\" } };\n";
        project.write("uf.config.js", config);

        let output = uf()
            .arg("--cwd")
            .arg(project.path())
            .args(["test", "--host", "deno", "--json", "probe.test.js"])
            .output()
            .expect("uf runs");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "a Flow suite must run on Deno\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        assert_eq!(
            std::fs::read_to_string(project.path().join("uf.config.js")).unwrap(),
            config,
            "a host override must not rewrite the project"
        );
        let document: serde_json::Value =
            serde_json::from_str(&stdout).expect("`uf test --json` is one document");
        let host = &document["host"];
        assert_eq!(host["kind"], serde_json::json!("deno"));
        assert_eq!(host["runtimeHost"], serde_json::json!("deno"));
        assert_eq!(host["loadsFlow"], serde_json::json!(true));
        // The ahead-of-time pass's artefact is gone with the pass.
        assert!(host.get("denoImportMap").is_none(), "{host}");
        assert_eq!(host["support"]["level"], serde_json::json!("implemented"));
        assert_eq!(
            host["support"]["flowLoader"],
            serde_json::json!("@uniflowed/host/deno-preload")
        );
        assert!(
            host["support"]["missing"]
                .as_str()
                .is_some_and(|missing| missing.contains("coverage")),
            "{host}"
        );
        // No worker was handed an import map, and no `.uf/deno` tree was written.
        assert!(!project.path().join(".uf/deno").exists());
    });
}

#[test]
fn uf_test_json_reports_reasoned_deno_skips() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = deno_project(&[(
            "probe.test.js",
            "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"runs the host portable half\", () => {\n  expect(21 * 2).toBe(42);\n});\n\n\
         it.skipBecause(\n\
         \x20 \"reads Node's coverage switch\",\n\
         \x20 \"Deno counts coverage in a format of its own, so this file cannot read \
         NODE_V8_COVERAGE there.\",\n\
         );\n",
        )]);

        let output = uf()
            .arg("--cwd")
            .arg(project.path())
            .args(["test", "--host", "deno", "--json", "probe.test.js"])
            .output()
            .expect("uf runs");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "a Deno run with a supported test and a reasoned skip must stay green\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        let document: serde_json::Value =
            serde_json::from_str(&stdout).expect("`uf test --json` is one document");
        assert_eq!(document["host"]["kind"], serde_json::json!("deno"));
        assert_eq!(document["passed"], serde_json::json!(1));
        assert_eq!(document["skipped"], serde_json::json!(1));
        assert_eq!(document["success"], serde_json::json!(true));

        let tests = document["tests"]
            .as_array()
            .expect("the payload carries case records");
        let skipped = tests
            .iter()
            .find(|record| record["name"] == "reads Node's coverage switch")
            .expect("the skipped case is reported");
        assert_eq!(skipped["status"], serde_json::json!("skipped"));
        assert!(
            skipped["skipReason"]
                .as_str()
                .is_some_and(|reason| reason.contains("format of its own")),
            "{skipped}"
        );
    });
}

/// Every line a child writes, from both streams, as one channel.
fn lines_of(child: &mut std::process::Child) -> Receiver<String> {
    let (sender, lines) = channel();
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
    lines: &Receiver<String>,
    needle: &str,
    budget: Duration,
    transcript: &mut String,
) -> bool {
    let deadline = Instant::now() + budget;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(line) = lines.recv_timeout(left) else {
            return false;
        };
        transcript.push_str(&line);
        transcript.push('\n');
        if line.contains(needle) {
            return true;
        }
        // Deno aborted inside V8, and what the session was waiting for is not
        // coming: fail now, so `once_more_if_v8_aborts` has a second run to
        // start rather than two minutes to wait.
        if line.contains("Fatal error in ") && is_a_v8_abort_inside_deno(transcript) {
            return false;
        }
    }
    false
}

/// A watch session on Deno reports an edit on the next run.
///
/// This host used to refuse `--watch` outright: its loader was an ahead-of-time
/// pass, and every run after the first would have reported on the tree that
/// pass wrote. With a hook, the worker's fresh `?uf-run=` import is a module
/// the hook is asked about like any other, so an edit to a dependency reaches
/// the next run — which this asserts by breaking the dependency under a running
/// session and waiting for the failure it causes.
#[test]
fn uf_test_watch_on_deno_reports_an_edit_on_the_next_run() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = deno_project(&[(
            "probe.test.js",
            "// @flow\nimport { expect, it } from \"@uniflowed/test\";\nimport { double } from \"./double.js\";\n\n\
         it(\"doubles\", () => {\n  expect(double(21)).toBe(42);\n});\n",
        )]);
        project.write("double.js", DOUBLE);

        // A plain `Command` rather than `uf()`, which runs to completion: this one
        // is spawned, read while it runs, and killed.
        let mut child = Command::new(uf_path())
            .arg("--cwd")
            .arg(project.path())
            .args([
                "test",
                "--watch",
                "--watch-interval",
                "100",
                "probe.test.js",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
            // Past the resolution of any filesystem's modification time, so the
            // watcher cannot read the edit as the file it already recorded.
            std::thread::sleep(Duration::from_millis(1_100));
            project.write(
            "double.js",
            "// @flow\nexport function double(value: number): number {\n  return value * 3;\n}\n",
        );
            wait_for(
                &lines,
                "1 failed",
                Duration::from_secs(120),
                &mut transcript,
            )
        };
        let _ = child.kill();
        let _ = child.wait();

        assert!(first, "the first run never passed:\n{transcript}");
        assert!(second, "the edit never reached a run:\n{transcript}");
        assert!(
            transcript.contains("expected 63 to be 42"),
            "the re-run did not load the edited module:\n{transcript}"
        );
    });
}

/// Module mocking works on Deno, through the same `registerHooks`.
///
/// `@uniflowed/host/module-mocks.js` needs synchronous, in-thread hooks, and
/// used to raise `UnsupportedError` everywhere but Node. Deno 2.8 has the hook
/// its Flow loader is built on, and the interception is installed through the
/// same call — so `uft.mock` replaces a module and `uft.unmock` restores it.
#[test]
fn module_mocking_on_deno_replaces_a_module_before_it_is_imported() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = deno_project(&[(
            "mock.test.js",
            r#"// @flow
import { expect, it, uft } from "@uniflowed/test";

it("replaces a module before it is imported", async () => {
  await uft.mock("./client.js", () => ({ BASE: "https://stub.test", send: () => "stubbed" }));
  const client = await import("./client.js");
  expect(client.send("/hello")).toBe("stubbed");
  expect(client.BASE).toBe("https://stub.test");
});

it("stops at unmock", async () => {
  await uft.mock("./client.js", () => ({ BASE: "x", send: () => "stubbed" }));
  await uft.unmock("./client.js");
  const client = await import("./client.js");
  expect(client.send("/hello")).toBe("real https://api.test/hello");
});
"#,
        )]);
        project.write(
            "client.js",
            "// @flow\nexport const BASE: string = \"https://api.test\";\n\n\
         export function send(path: string): string {\n  return `real ${BASE}${path}`;\n}\n",
        );

        let output = uf()
            .arg("--cwd")
            .arg(project.path())
            .args(["test", "--json", "mock.test.js"])
            .output()
            .expect("uf runs");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "stdout:\n{stdout}\nstderr:\n{stderr}"
        );
        let document: serde_json::Value =
            serde_json::from_str(&stdout).expect("`uf test --json` is one document");
        assert_eq!(document["passed"], serde_json::json!(2), "{stdout}");
    });
}

/// Coverage is refused on Deno before a worker starts.
///
/// The refusal has to come before anything is compiled, or an unsupported mode
/// could fail on the project's Flow and tell the user about their syntax
/// instead of the mode they asked for. This test gives Deno a file that would
/// not compile and asserts nothing was.
#[test]
fn uf_test_coverage_on_deno_refuses_before_a_worker_starts() {
    if !deno_ready() || !deno_with_hooks() {
        return;
    }
    let project = deno_project(&[(
        "broken.test.js",
        "// @flow\nimport { it } from \"@uniflowed/test\";\n\n\
         const value: = 1;\n\
         it(\"would never parse\", () => {});\n",
    )]);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--coverage", "broken.test.js"])
        .output()
        .expect("uf runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stderr:\n{stderr}");
    assert!(stderr.contains("uf test --coverage"), "stderr:\n{stderr}");
    assert!(stderr.contains("Node-only today"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("Deno cannot provide that same Flow-source report"),
        "stderr:\n{stderr}"
    );
    assert!(
        !project.path().join(".uf/cache/transform").exists(),
        "`uf test --coverage` on Deno must refuse before anything is compiled"
    );
}

/// A Deno older than the hook is refused by version, before a worker starts.
///
/// Measured with a stand-in rather than an old Deno: the refusal reads what
/// `deno --version` prints, and a script that prints a 2.7 is that input
/// exactly. What is asserted is the sentence a person meets — which release was
/// found, which one is needed, why, and what to do — and that no worker ran
/// to meet a syntax error first.
#[cfg(unix)]
#[test]
fn uf_test_refuses_a_deno_older_than_the_hook() {
    use std::os::unix::fs::PermissionsExt;

    if !host_ready() {
        return;
    }
    let project = deno_project(&[(
        "probe.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"passes\", () => {\n  expect(1).toBe(1);\n});\n",
    )]);
    let bin = project.path().join("old-deno");
    std::fs::create_dir_all(&bin).unwrap();
    let fake = bin.join("deno");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho 'deno 2.7.4 (stable, release, x86_64-unknown-linux-gnu)'\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .env("PATH", path)
        .output()
        .expect("uf runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stderr:\n{stderr}");
    for expected in ["2.7", "2.8", "registerHooks", "deno upgrade"] {
        assert!(stderr.contains(expected), "{expected}\nstderr:\n{stderr}");
    }
    assert!(
        !project.path().join(".uf/cache/transform").exists(),
        "a refused Deno must not have compiled anything"
    );
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
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
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
         \x20 expect(outside).toBe(\"NotCapable\");\n\
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
    });
}

/// Copy a fixture into a project, leaving out what a build or an install
/// leaves behind in it.
fn copy_fixture(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("the fixture is readable") {
        let entry = entry.expect("the fixture is readable");
        let name = entry.file_name();
        if matches!(name.to_str(), Some("dist" | ".uf" | "node_modules")) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type().expect("the fixture is readable").is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_fixture(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// A port nothing is listening on, for a server this test starts.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .expect("a loopback port can be bound")
}

/// One `GET` as a document request: the status and the whole response.
fn get(port: u16, path: &str) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .ok()?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/html\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    let status = response.split_whitespace().nth(1)?.parse().ok()?;
    Some((status, response))
}

/// The served-app fixture, pinned to Deno.
const SERVED_APP_ON_DENO: &str = r#"// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    router: { entry: "app.js", root: "app" },
    runtime: { capabilityJsHost: { default: "deno", autoDetect: false } },
  },
  build: {
    entries: ["app.js"],
    outDir: "dist",
  },
});
"#;

/// `uf build` and `uf start` on Deno: the driver's half of the host.
///
/// `@uniflowed/vite`'s driver installs the Deno hooks itself, after its static
/// imports — which is what keeps Rolldown's native binding loadable, because
/// Deno cannot `require()` an addon while a `load` hook is registered. This is
/// the served-app fixture the adapters are tested against, built on Deno and
/// served on Deno: a page the build prerendered, and a dynamic route and a
/// route handler the application answers.
#[test]
fn uf_build_and_uf_start_run_on_deno() {
    once_more_if_v8_aborts(|| {
        if !deno_ready() || !deno_with_hooks() {
            return;
        }
        let project = Project::new(&[]);
        copy_fixture(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/served-app"),
            project.path(),
        );
        project.write("uf.config.js", SERVED_APP_ON_DENO);

        let build = uf()
            .arg("--cwd")
            .arg(project.path())
            .arg("build")
            .output()
            .expect("uf runs");
        let stdout = String::from_utf8_lossy(&build.stdout);
        let stderr = String::from_utf8_lossy(&build.stderr);
        assert!(
            build.status.success(),
            "stdout:\n{stdout}\nstderr:\n{stderr}"
        );
        let guide = std::fs::read_to_string(project.path().join("dist/guide/index.html"))
            .unwrap_or_default();
        assert!(
            guide.contains("served-app guide"),
            "the build prerendered no /guide\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );

        let port = free_port();
        let mut server = Command::new(uf_path())
            .arg("--cwd")
            .arg(project.path())
            .args(["start", "--port", &port.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("uf starts");
        let lines = lines_of(&mut server);

        let deadline = Instant::now() + Duration::from_secs(120);
        let mut home = None;
        while Instant::now() < deadline {
            if let Some((200, body)) = get(port, "/") {
                home = Some(body);
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        let post = get(port, "/posts/deno");
        let health = get(port, "/api/health");
        let _ = server.kill();
        let _ = server.wait();
        let mut transcript = String::new();
        while let Ok(line) = lines.recv_timeout(Duration::from_millis(500)) {
            transcript.push_str(&line);
            transcript.push('\n');
        }

        let Some(home) = home else {
            panic!("`uf start` on Deno never answered /:\n{transcript}");
        };
        assert!(home.contains("served-app home"), "{home}");
        assert!(
            post.as_ref()
                .is_some_and(|(status, body)| *status == 200 && body.contains("post: deno")),
            "{post:?}\n{transcript}"
        );
        assert!(
            health
                .as_ref()
                .is_some_and(|(status, body)| *status == 200 && body.contains("\"status\":\"ok\"")),
            "{health:?}\n{transcript}"
        );
        assert!(
            transcript.contains("deno"),
            "the server did not say which host it is on:\n{transcript}"
        );
    });
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

/// The one thing Deno does that no other host does.
///
/// Run with the arguments `uf_runtime::permissions` produces — not with
/// arguments this test wrote, which would test nothing but itself — and the
/// assertion is on what the program observed. All five categories at once,
/// because the point of the row in `uf_runtime::HOSTS` is that Deno is the host
/// the model was designed against.
#[test]
fn deno_enforces_the_whole_permission_set_uf_translates() {
    once_more_if_v8_aborts(|| {
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

        let run = deno(&project, &flags, "probe.js", &[]);

        // `NotCapable`, which is what Deno 2 calls a denial by its own permission
        // model; Deno 1 called it `PermissionDenied`, a name Deno 2 keeps for the
        // operating system refusing. uf starts no Deno older than 2.8.
        assert!(
            run.stdout.contains("read=NotCapable"),
            "a read outside the project was allowed\nstdout:\n{}\nstderr:\n{}",
            run.stdout,
            run.stderr
        );
        assert!(
            run.stdout.contains("env=NotCapable"),
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
    });
}

/// The table says Deno is implemented; the file that says so is this one.
///
/// A row naming a test that does not exist would be the same unchecked claim in
/// a new place, so the name is asserted rather than trusted — along with the
/// loader it names, which is the preload these tests start, and the gap it
/// still owns, which a grade of *implemented* does not get to drop.
#[test]
fn the_host_table_points_at_this_file() {
    let support = HostSupport::for_host(RuntimeHost::Deno);
    assert_eq!(support.level, SupportLevel::Implemented);
    assert!(support.loads_flow());
    assert_eq!(support.flow_loader, Some("@uniflowed/host/deno-preload"));
    assert!(
        support
            .missing
            .is_some_and(|missing| missing.contains("coverage")),
        "{:?}",
        support.missing
    );
    assert_eq!(
        support.verified_by,
        Some("crates/uf_cli/tests/deno_host.rs")
    );
    assert_eq!(support.enforces, Permission::ALL);
}

/// What Deno 2.9.6 printed when V8 aborted in CI run 34955031179, with its
/// `Args` and its stack trace shortened.
const V8_ABORT: &str = "\
============================================================
Deno has panicked. This is a bug in Deno. Please report this
at https://github.com/denoland/deno/issues/new.
If you can reliably reproduce this panic, include the
reproduction steps and the stack trace URL below in your report.

Platform: linux x86_64
Version: 2.9.6
Args: [\"deno\", \"run\", \"--preload\", \"packages/host/deno-preload.js\", \"entry.js\"]

View stack trace at:
https://panic.deno.com/v2.9.6/x86_64-unknown-linux-gnu/k4m4xF-l2j1D071j1Di01j1Dwz1j1Dqgm70Dyljk1DoktsxCmp98-B

thread 'main' (45485) panicked at cli/lib.rs:701:5:
Fatal error in :0: unreachable code
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
";

/// A second run is for V8 aborting inside Deno, and for nothing uf did.
#[test]
fn only_v8_aborting_inside_deno_earns_a_second_run() {
    assert!(is_a_v8_abort_inside_deno(V8_ABORT));
    // As a failure message carries it: after uf's own output, through a stream
    // that indents what it relays.
    let relayed: String = V8_ABORT.lines().map(|line| format!("  {line}\n")).collect();
    assert!(is_a_v8_abort_inside_deno(&format!(
        "the first run never passed:\n RUN  probe.test.js\n{relayed}"
    )));

    // A panic in Deno's own Rust code is one that what uf hands Deno can cause.
    let rust_panic = V8_ABORT.replace(
        "cli/lib.rs:701:5:\nFatal error in :0: unreachable code",
        "ext/node/ops/require.rs:88:5:\ncalled `Option::unwrap()` on a `None` value",
    );
    assert_ne!(rust_panic, V8_ABORT);
    assert!(!is_a_v8_abort_inside_deno(&rust_panic));

    for failure in [
        "",
        "error: Uncaught SyntaxError: Unexpected token ':'",
        "error: Uncaught (in promise) NotCapable: Requires run access to \"uf\"",
        "the edit never reached a run:\n 1 passed\n",
        // V8's words, without Deno's banner above them.
        "thread 'main' panicked at cli/lib.rs:701:5:\nFatal error in :0: unreachable code",
        // Or in an order Deno does not print them in.
        "Fatal error in :0: unreachable code\nDeno has panicked. This is a bug in Deno.\n",
    ] {
        assert!(!is_a_v8_abort_inside_deno(failure), "{failure}");
    }
}

/// And it is one second run: a failure that is not an abort is the test's the
/// first time, and an abort that happens again is the test's the second time.
#[test]
fn a_test_runs_at_most_twice_and_twice_only_after_an_abort() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // A body that fails with each of `failures` in turn and then passes: how
    // many times it ran, how many second runs were announced, and whether the
    // test it stands for passed.
    let attempt = |failures: &[&str]| {
        let runs = AtomicUsize::new(0);
        let announced = AtomicUsize::new(0);
        let passed = catch_unwind(AssertUnwindSafe(|| {
            retry_once_when(
                &|| {
                    if let Some(failure) = failures.get(runs.fetch_add(1, Ordering::SeqCst)) {
                        panic!("{failure}");
                    }
                },
                is_a_v8_abort_inside_deno,
                |_| {
                    announced.fetch_add(1, Ordering::SeqCst);
                },
            );
        }))
        .is_ok();
        (runs.into_inner(), announced.into_inner(), passed)
    };

    assert_eq!(attempt(&[]), (1, 0, true));
    assert_eq!(attempt(&[V8_ABORT]), (2, 1, true));
    assert_eq!(attempt(&[V8_ABORT, V8_ABORT]), (2, 1, false));
    assert_eq!(
        attempt(&[V8_ABORT, "the edit never reached a run"]),
        (2, 1, false)
    );
    assert_eq!(
        attempt(&["a warm run reached for `uf transform`"]),
        (1, 0, false)
    );
}
