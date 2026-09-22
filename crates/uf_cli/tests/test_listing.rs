//! Machine-readable discovery lets runtime lanes retain uf's own file selection.
mod support;
use support::{Project, uf};

#[test]
fn json_listing_reports_tests_without_starting_a_host() {
    let project = Project::new(&[
        ("plain.js", "export const value = 1;\n"),
        (
            "one.test.js",
            "import { it } from '@uniflowed/test';\nit('listed only', () => { throw new Error('must not run'); });\n",
        ),
    ]);
    let output = uf()
        .arg("--cwd")
        .arg(project.path())
        .args(["test", "--list", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["list"], true);
    assert_eq!(report["files"], serde_json::json!(["one.test.js"]));
    assert_eq!(report["tests"][0]["name"], "listed only");
    assert_eq!(report["tests"][0]["selection"], "run");
}
