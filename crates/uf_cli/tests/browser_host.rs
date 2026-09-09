//! `uf test --browser`, against a real browser, over a real project.
//!
//! `docs/hosts.md` says why this file exists in one sentence: **an unchecked
//! row is worth nothing.** Bun's row said "implemented" for a long time on the
//! strength of a README paragraph, and when somebody finally started a Bun the
//! preload turned out to be broken in two independent ways at once. A browser
//! is a heavier dependency than Bun and a longer path — a driver, a module
//! server, an import map, a CommonJS wrapper and a page — so the claim that it
//! runs a component test is worth less than usual until something runs one.
//!
//! Two kinds of test are here and they are separated deliberately.
//!
//! The **refusals** need no browser and always run. Each of them is a promise
//! `uf test --browser` makes about what it will not quietly do, and a promise
//! that is only checked where a browser is installed is a promise that is not
//! checked in CI.
//!
//! The **run** needs a browser and says so when it steps aside, in the shape
//! [`support::host_ready`] already uses: a silently reduced suite is how a
//! runner starts lying. What it asserts is not "some tests passed" — it is that
//! a component was measured, which is the only thing browser mode is for. A
//! `getBoundingClientRect().width` of three hundred cannot come from happy-dom,
//! which answers zero for every element it has ever been shown.

mod support;

use std::time::Duration;

use support::{Project, repo_root, uf};

/// How long a browser run may take before the test calls it hung.
///
/// A cold Chromium with a cold profile is seven seconds on a laptop and worse
/// in CI, and this run also transforms twenty modules on the way. It is the
/// line between "failed" and "hung", not a performance assertion.
const DEADLINE: Duration = Duration::from_secs(180);

/// A component test that only a layout engine can answer.
const MEASURED: &str = r#"// @flow
import { render, screen } from "@uniflowed/react-testing";
import { expect, it } from "@uniflowed/test";

component Half() {
  return (
    <div style={{ width: "600px" }}>
      <div data-testid="half" style={{ width: "50%", height: "40px" }} />
    </div>
  );
}

it("is measured by a real layout engine", () => {
  render(<Half />);
  const half = screen.getByTestId("half");
  expect(half.getBoundingClientRect().width).toBe(300);
  expect(globalThis.getComputedStyle(half).width).toBe("300px");
});
"#;

/// Whether a browser run can happen here.
///
/// The same lookup `uf test --browser` performs, asked before the run so the
/// skip says "no browser" rather than leaving a refusal to be read as a
/// failure. `UF_BROWSER` is honoured for the same reason it is honoured there.
fn browser_ready() -> bool {
    let named = std::env::var("UF_BROWSER").ok();
    let found = uf_test::find_browser(
        named.as_deref(),
        &|name| which(name),
        &camino::Utf8Path::is_file,
    );
    if found.is_err() {
        eprintln!(
            "skipping: `uf test --browser` needs a browser installed, or UF_BROWSER set to one"
        );
    }
    let installed = repo_root()
        .join("node_modules/@uniflowed/test/browser-worker.js")
        .is_file();
    if !installed {
        eprintln!("skipping: `uf test --browser` needs `npm ci` at the workspace root");
    }
    found.is_ok() && installed
}

/// A `PATH` lookup, which the crate under test takes as an argument.
fn which(name: &str) -> Option<camino::Utf8PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|directory| {
        let candidate = directory.join(name);
        candidate
            .is_file()
            .then(|| camino::Utf8PathBuf::from_path_buf(candidate).ok())
            .flatten()
    })
}

#[test]
fn a_component_is_measured_by_a_real_layout_engine() {
    if !browser_ready() {
        return;
    }
    let project = Project::new(&[("measured.test.js", MEASURED)]);

    let output = uf()
        .args(["test", "--browser", "--json"])
        .current_dir(project.path())
        .timeout(DEADLINE)
        .output()
        .expect("uf runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "a browser run of a measured component must pass\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // The report rather than the exit status alone: a run that discovered no
    // files also exits zero, and "0 tests passed" is exactly the green nobody
    // wants from a mode they asked for because they stopped trusting one.
    let report: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("`--json` is one document: {error}\n{stdout}"));
    assert_eq!(report["passed"], 1, "one case, measured in a browser\n{stdout}");
    assert_eq!(report["failed"], 0, "{stdout}");
    assert_eq!(report["files"], 1, "{stdout}");
}

#[test]
fn no_browser_is_refused_by_name_rather_than_run_on_the_shim() {
    // The most important refusal in the file. Falling back to happy-dom here
    // would report a pass for the exact assertions `--browser` was reached for,
    // and the report would carry no hint that a browser was never involved.
    let project = Project::new(&[(
        "a.test.js",
        "// @flow\nimport { it } from \"@uniflowed/test\";\nit(\"runs\", () => {});\n",
    )]);

    let output = uf()
        .args(["test", "--browser"])
        .env("UF_BROWSER", "/no/such/browser")
        .current_dir(project.path())
        .output()
        .expect("uf runs");

    assert!(!output.status.success(), "a missing browser stops the run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("/no/such/browser"), "{stderr}");
    assert!(
        stderr.contains("will not quietly pick a different one"),
        "{stderr}"
    );
}

#[test]
fn coverage_in_a_browser_says_which_host_can_collect_it() {
    let project = Project::new(&[(
        "a.test.js",
        "// @flow\nimport { it } from \"@uniflowed/test\";\nit(\"runs\", () => {});\n",
    )]);

    let output = uf()
        .args(["test", "--browser", "--coverage"])
        .env("UF_BROWSER", "/no/such/browser")
        .current_dir(project.path())
        .output()
        .expect("uf runs");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot measure anything"), "{stderr}");
    // And it is *this* refusal rather than "there is no browser at
    // /no/such/browser", even though both are true: installing a browser would
    // not make the combination work, so the message that sends a reader
    // somewhere useful has to be the one that wins.
    assert!(!stderr.contains("/no/such/browser"), "{stderr}");
    // It says where the numbers do come from, because "run it on Node" is the
    // answer rather than a consolation.
    assert!(stderr.contains("uf test --coverage"), "{stderr}");
}

#[test]
fn rewriting_snapshots_in_a_browser_is_refused_before_anything_runs() {
    let project = Project::new(&[(
        "a.test.js",
        "// @flow\nimport { it } from \"@uniflowed/test\";\nit(\"runs\", () => {});\n",
    )]);

    let output = uf()
        .args(["test", "--browser", "-u"])
        .env("UF_BROWSER", "/no/such/browser")
        .current_dir(project.path())
        .output()
        .expect("uf runs");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("has no filesystem"), "{stderr}");
    // And it says what to do instead, because "record on Node, check in the
    // browser" is the workflow rather than a consolation.
    assert!(stderr.contains("uf test -u"), "{stderr}");
}

#[test]
fn a_permission_set_stops_a_browser_run_rather_than_being_half_applied() {
    let project = Project::new(&[(
        "a.test.js",
        "// @flow\nimport { it } from \"@uniflowed/test\";\nit(\"runs\", () => {});\n",
    )]);
    project.write(
        "uf.config.js",
        "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\nexport default defineConfig({ permissions: { read: [\"./fixtures\"] } });\n",
    );

    let output = uf()
        .args(["test", "--browser"])
        .env("UF_BROWSER", "/no/such/browser")
        .current_dir(project.path())
        .output()
        .expect("uf runs");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot enforce this project's `permissions`"),
        "{stderr}"
    );
}
