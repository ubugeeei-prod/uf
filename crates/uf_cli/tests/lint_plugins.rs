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
///
/// The fix replaces the name alone. An identifier with a type annotation spans
/// the annotation too, as it does in every Flow ESTree, so replacing the whole
/// node would rename `foo: number` to `bar` and drop the type.
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
                fix: (fixer) =>
                  fixer.replaceTextRange([node.range[0], node.range[0] + node.name.length], "bar"),
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

/// A plugin whose one rule only ever moves whitespace, and says so.
///
/// The needle is built from pieces so the plugin's own source does not
/// contain it: `uf lint --fix` lints the rule file too, and a rule that matched
/// itself would rewrite its own search into one that matches everything.
const LAYOUT: &str = r#"// @noflow
export default {
  name: "layout",
  rules: {
    "one-space": {
      meta: { fixable: "whitespace" },
      create(context) {
        return {
          Program() {
            const text = context.sourceCode.text;
            const at = text.indexOf(" ".repeat(2) + "=");
            if (at !== -1) {
              context.report({
                loc: context.sourceCode.getLocFromIndex(at),
                message: "one space before `=`",
                fix: (fixer) => fixer.replaceTextRange([at, at + 2], " "),
              });
            }
          },
        };
      },
    },
  },
};
"#;

/// `uf` in `project` with the terminal report: the exit code and everything
/// it printed.
fn run(project: &Project, args: &[&str]) -> (i32, String) {
    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["--color", "never"])
        .args(args)
        .output()
        .expect("uf started");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr),
    )
}

fn read(project: &Project, name: &str) -> String {
    std::fs::read_to_string(project.path().join(name)).expect("the file is still there")
}

/// A `code` fix is the unsafe tier's: `--fix` leaves it and says which flag
/// would write it, and `--fix-unsafe` writes it.
#[test]
fn a_code_fix_waits_for_fix_unsafe_and_is_then_written() {
    let project = project("\"acme/no-foo\": \"error\"");

    let (_, output) = run(&project, &["lint", "--fix"]);

    assert_eq!(
        read(&project, "app.js"),
        APP,
        "`--fix` wrote a `code` fix\n{output}"
    );
    assert!(
        output.contains("would be fixed by `--fix-unsafe`"),
        "{output}"
    );

    let (code, output) = run(&project, &["lint", "--fix-unsafe"]);

    assert_eq!(
        read(&project, "app.js"),
        "// @flow\n\nexport const bar: number = 1;\n",
        "{output}"
    );
    assert_eq!(code, 0, "nothing is left once the fix is written\n{output}");
}

/// A rule that promises layout-only edits is in the safe tier.
#[test]
fn a_whitespace_fix_is_written_by_fix() {
    let project = Project::new(&[
        (
            "uf.config.js",
            "export default {\n  plugins: [\"./rules/layout.js\"],\n  lint: { rules: { \"layout/one-space\": \"error\" } },\n};\n",
        ),
        ("rules/layout.js", LAYOUT),
        ("app.js", "// @flow\n\nexport const foo: number  = 1;\n"),
    ]);

    let (code, output) = run(&project, &["lint", "--fix"]);

    assert_eq!(
        read(&project, "app.js"),
        "// @flow\n\nexport const foo: number = 1;\n",
        "{output}"
    );
    assert_eq!(code, 0, "{output}");
}

/// One message as the protocol frames it.
fn framed(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

/// The editor sees what `uf lint` sees: the finding, and its fix in the
/// lightbulb menu.
#[test]
fn the_language_server_reports_a_project_rule_and_offers_its_fix() {
    let project = project("\"acme/no-foo\": \"error\"");
    let uri = format!("file://{}/app.js", project.path().display());
    let text = serde_json::to_string(APP).expect("a JSON string");
    let input = [
        framed(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#),
        framed(&format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{uri}","languageId":"javascript","version":1,"text":{text}}}}}}}"#
        )),
        framed(&format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"textDocument/codeAction","params":{{"textDocument":{{"uri":"{uri}"}},"range":{{"start":{{"line":2,"character":13}},"end":{{"line":2,"character":16}}}},"context":{{"diagnostics":[]}}}}}}"#
        )),
        framed(r#"{"jsonrpc":"2.0","method":"exit"}"#),
    ]
    .concat();

    let output = uf()
        .arg("lsp")
        .arg("--cwd")
        .arg(project.path())
        .write_stdin(input)
        .output()
        .expect("uf started");

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        stdout.contains(r#""code":"acme/no-foo""#),
        "no project finding was published:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains(r#""newText":"bar""#),
        "no fix was offered:\n{stdout}\n{stderr}"
    );
}
