//! The permission set, enforced by a host that is actually running.
//!
//! `uf.config.js` declares what a project's code may reach and
//! `uf_runtime::permissions` translates it per host. The unit tests there
//! establish the translation; **this file establishes the enforcement**, which
//! is a different claim and the only one worth making. A test that asserted
//! `--allow-fs-read` had been passed would pass just as happily against a Node
//! that ignored the flag, against a flag Node had renamed, and against a run
//! whose loader never started — and ubugeeei-prod/uf#535 asks for the opposite:
//! *a test per host that a denied read is denied.*
//!
//! So `uf test` is run for real, over a real project, with the real binary. The
//! test file it runs is Flow — a `: number` annotation and all — and it tries
//! to read a file outside the project. Both halves matter: a run where the
//! transform had quietly failed would deny the read too, and would prove
//! nothing about the permission model.
//!
//! Node is the host here because it is the reference one: it loads Flow through
//! a module hook and enforces the two permissions it has. Bun has no permission
//! model at all, and Deno — which enforces all five, and is therefore where
//! this file's subject is checked most thoroughly — loads Flow by a road of its
//! own. Those two are `tests/bun_host.rs` and `tests/deno_host.rs`.

mod support;

use std::path::Path;

use support::{Project, host_ready, uf};

/// A project whose tests run under a declared permission set.
///
/// `permissions: {}` is the strictest thing that can be written: nothing beyond
/// what uf itself needs to load and transform the project. It is also the case
/// most likely to be got wrong, because every grant in force is one uf added on
/// the project's behalf rather than one somebody wrote down.
const STRICT_CONFIG: &str = "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
                             export default defineConfig({ permissions: {} });\n";

fn write_probe(project: &Project, outside: &Path) {
    project.write(
        "probe.test.js",
        &format!(
            r#"// @flow
import {{ readFileSync }} from "node:fs";
import {{ expect, it }} from "@uniflowed/test";

it("runs Flow and is denied a read outside the project", () => {{
  // Flow syntax, so a run that reached this line at all is a run whose loader
  // and transform survived the permission model.
  const answer: number = 42;
  expect(answer).toBe(42);

  let outcome = "allowed";
  try {{
    readFileSync({outside:?}, "utf8");
  }} catch (error) {{
    outcome = String(error.code ?? error.name);
  }}
  expect(outcome).toBe("ERR_ACCESS_DENIED");
}});
"#,
            outside = outside.to_string_lossy(),
        ),
    );
}

/// A file the project has no grant for, in a directory it has no grant for.
///
/// The system temp directory rather than somewhere in the repository: every
/// path uf grants is derived from the project root, the packages directory or
/// the `uf` binary, and a probe that happened to sit under one of those would
/// make this test pass for the wrong reason.
/// `name` because the two runs below happen at once — `cargo test` threads
/// them — and one file shared between them would let the first test's cleanup
/// delete the file the second is in the middle of reading.
fn outside_file(name: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("uf-permission-probe-{}-{name}", std::process::id()));
    std::fs::write(&path, "not for the test to read\n").expect("the temp directory is writable");
    path
}

/// The whole claim, in one run.
///
/// Before this existed, `uf test` passed no permission flags at all and a test
/// body could read anything the developer could. The failure it guards is not
/// "the flag was dropped" but "the sandbox was not there", which is why the
/// assertion is on what the test body observed rather than on the command line.
#[test]
fn a_test_body_is_denied_a_read_the_project_did_not_declare() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[]);
    project.write("uf.config.js", STRICT_CONFIG);
    let outside = outside_file("denied");
    write_probe(&project, &outside);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");
    let _ = std::fs::remove_file(&outside);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the case asserts the read was denied, so a failure here is a read that was \
         allowed — or a loader the permission model stopped from starting, which the message \
         will say.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("1 passed") || stderr.contains("1 passed"),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// And the same project with no permission block reads it happily.
///
/// The control. Without it the test above would still pass if `readFileSync`
/// were failing for some reason of its own — a path that does not exist, a
/// worker that never ran the body — and the whole file would be measuring
/// nothing. This is the *only* difference between the two runs.
#[test]
fn the_same_read_is_allowed_when_no_permission_set_is_declared() {
    if !host_ready() {
        return;
    }
    let project = Project::new(&[]);
    let outside = outside_file("allowed");
    write_probe(&project, &outside);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");
    let _ = std::fs::remove_file(&outside);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The case expects `ERR_ACCESS_DENIED` and the read succeeds, so this run
    // *fails* — and that failure is the assertion. A green run here would mean
    // the read was denied without anybody asking for it.
    assert!(
        !output.status.success(),
        "with no permission set the read must succeed, so the probe must \
         fail\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("allowed") || stderr.contains("allowed"),
        "the failure should be the probe's own comparison against \"allowed\"\nstdout:\n{stdout}\n\
         stderr:\n{stderr}"
    );
}

/// A set Node cannot enforce stops the run and says who can.
///
/// This is the design ubugeeei-prod/uf#535 turns on, and the one thing a
/// translation layer must not do quietly. Node has no network dimension at all;
/// passing the four flags it does have and dropping `net` would produce a run
/// that reads as sandboxed and lets a test post anywhere it likes.
#[test]
fn a_permission_node_cannot_enforce_stops_the_run() {
    let project = Project::new(&[(
        "probe.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"never runs\", () => {\n  expect(1).toBe(1);\n});\n",
    )]);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({\n  permissions: { net: [\"registry.npmjs.org\"] },\n});\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stderr:\n{stderr}");
    assert!(stderr.contains("cannot enforce"), "stderr:\n{stderr}");
    assert!(stderr.contains("`net`"), "stderr:\n{stderr}");
    // Naming the host that can is what makes the refusal actionable rather than
    // a dead end.
    assert!(stderr.contains("Deno"), "stderr:\n{stderr}");
}

/// A misspelled permission is an error, not a silently empty grant.
///
/// Every other block in `uf.config.js` ignores a key it does not recognize,
/// which is right where an unknown key means an option from a newer uf. Here it
/// would mean a project running with a permission it believes it declared.
#[test]
fn a_misspelled_permission_is_refused_at_load() {
    let project = Project::new(&[(
        "probe.test.js",
        "// @flow\nimport { expect, it } from \"@uniflowed/test\";\n\n\
         it(\"never runs\", () => {\n  expect(1).toBe(1);\n});\n",
    )]);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\n\
         export default defineConfig({ permissions: { nett: [\"a\"] } });\n",
    );

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "probe.test.js"])
        .output()
        .expect("uf runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "stderr:\n{stderr}");
    assert!(stderr.contains("nett"), "stderr:\n{stderr}");
}

/// `uf explain test` says which host enforces what, and what uf added itself.
#[test]
fn explain_names_the_host_and_the_grants_uf_makes_for_itself() {
    let project = Project::new(&[]);
    project.write("uf.config.js", STRICT_CONFIG);

    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["explain", "test", "--json"])
        .output()
        .expect("uf runs");

    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("json");
    let stage = document["stages"]
        .as_array()
        .expect("stages")
        .iter()
        .find(|stage| stage["name"] == "permissions")
        .unwrap_or_else(|| panic!("no permissions stage in {stdout}"));

    let provider = stage["provider"].as_str().unwrap_or_default();
    assert!(provider.contains("Node.js"), "{provider}");
    assert!(provider.contains("read, write"), "{provider}");

    let detail = stage["detail"].as_str().unwrap_or_default();
    // The grants uf adds are the ones nobody wrote down, so they are the ones
    // that have to be visible.
    assert!(detail.contains("read: 0 declared, "), "{detail}");
    assert!(
        detail.contains("net: 0 declared, 0 added by uf, not enforced"),
        "{detail}"
    );
}
