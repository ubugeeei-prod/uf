//! Project rules: a rule a project writes in JavaScript, declared in
//! `uf.config.js`, reporting through `uf lint` — and the ceilings that keep one
//! from hanging the run.
//!
//! These start a real rule host, so they need Node.js on PATH and
//! `@uniflowed/host` in the workspace's `node_modules`, which is where `uf
//! test`'s own suites already need them.

mod support;

use std::time::{Duration, Instant};

use support::{Project, uf};

/// A plugin with one ordinary rule and one that never returns.
const PLUGIN: &str = r#"// @noflow
export default {
  name: "acme",
  rules: {
    "no-foo": {
      meta: {
        fixable: "code",
        messages: { reserved: "`{{ name }}` is reserved for the build" },
      },
      create(context) {
        return {
          Identifier(node) {
            if (node.name === "foo") {
              context.report({
                node,
                messageId: "reserved",
                data: { name: node.name },
                fix: (fixer) => fixer.replaceText(node, "bar"),
              });
            }
          },
        };
      },
    },
    "never-returns": {
      create() {
        return {
          Program() {
            for (;;) {}
          },
        };
      },
    },
  },
};
"#;

const APP: &str = "// @flow\n\nexport const foo: number = 1;\n";

/// A project whose `lint.rules` is `rules`, written as the inside of an object.
fn project(rules: &str) -> Project {
    let config = format!(
        "export default {{\n  plugins: [\"./rules/acme.js\"],\n  lint: {{ rules: {{ {rules} }} }},\n}};\n"
    );
    Project::new(&[
        ("uf.config.js", config.as_str()),
        ("rules/acme.js", PLUGIN),
        ("app.js", APP),
    ])
}

/// `uf lint --json` in `project`: the exit code, the report, and stderr.
fn lint(project: &Project) -> (i32, serde_json::Value, String) {
    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["--color", "never", "lint", "--json"])
        .output()
        .expect("uf started");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let report = serde_json::from_str(&stdout)
        .unwrap_or_else(|error| panic!("JSON on stdout ({error}): {stdout}\n{stderr}"));
    (output.status.code().unwrap_or(-1), report, stderr)
}

#[test]
fn a_projects_own_rule_reports_under_uf_lint() {
    let project = project("\"acme/no-foo\": \"error\"");

    let (code, report, stderr) = lint(&project);

    let found = report["diagnostics"]
        .as_array()
        .expect("a diagnostics array")
        .iter()
        .find(|diagnostic| diagnostic["rule"] == "acme/no-foo")
        .unwrap_or_else(|| panic!("no acme/no-foo finding: {report}\n{stderr}"));
    assert_eq!(found["path"], "app.js");
    assert_eq!(found["line"], 3);
    assert_eq!(found["column"], 14);
    assert_eq!(found["severity"], "error");
    assert_eq!(found["message"], "`foo` is reserved for the build");
    assert_eq!(code, 1, "an error-level finding fails the run: {report}");
    assert_eq!(report["projectRules"]["rules"][0]["rule"], "acme/no-foo");
    assert_eq!(
        report["projectRules"]["problems"],
        serde_json::json!([]),
        "{report}\n{stderr}"
    );
}

#[test]
fn a_run_without_project_rules_starts_no_host_and_says_nothing_about_them() {
    let project = project("\"flow/unclear-type\": \"warn\"");

    let (_, report, _) = lint(&project);

    assert!(report.get("projectRules").is_none(), "{report}");
}

#[test]
fn a_rule_that_never_returns_is_stopped_and_named() {
    let project = project("\"acme/never-returns\": \"warn\"");
    let started = Instant::now();

    let (code, report, stderr) = lint(&project);

    assert!(
        started.elapsed() < Duration::from_secs(90),
        "the run was not bounded"
    );
    assert_eq!(code, 1, "{report}\n{stderr}");
    assert!(
        report["projectRules"]["problems"]
            .to_string()
            .contains("did not finish"),
        "{report}"
    );
}

#[test]
fn an_enabled_rule_no_plugin_defines_is_named() {
    let project = project("\"acme/missing\": \"error\"");

    let (code, report, stderr) = lint(&project);

    assert_eq!(code, 1, "{report}\n{stderr}");
    assert!(
        report["projectRules"]["problems"]
            .to_string()
            .contains("no plugin in `plugins` defines it"),
        "{report}"
    );
}
