//! Native package operations do not need an external manager or a network.
mod support;
use std::fs;
use support::uf;

#[test]
fn local_native_graph_installs_freezes_removes_and_audits_without_a_host() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("lib")).unwrap();
    fs::write(
        root.join("package.json"),
        r#"{"name":"app","dependencies":{"lib":"file:lib"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("uf.config.js"),
        "export default { packageManager: 'uf', runtime: 'node@26' };\n",
    )
    .unwrap();
    fs::write(
        root.join("lib/package.json"),
        r#"{"name":"lib","version":"1.0.0"}"#,
    )
    .unwrap();
    let run = |args: &[&str]| {
        let output = uf()
            .arg("--cwd")
            .arg(root)
            .args(args)
            .env("PATH", root.join("no-external-programs"))
            .env("UF_PM_STORE", root.join("store"))
            .env("UF_ROOTS", root.join("roots"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    };
    run(&["install"]);
    assert!(root.join("node_modules/lib/package.json").exists());
    let lock = fs::read(root.join("uf.lock")).unwrap();
    run(&["install", "--frozen-lockfile"]);
    assert_eq!(fs::read(root.join("uf.lock")).unwrap(), lock);
    let report = run(&["audit", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    assert_eq!(report["checked"], 0);
    assert!(report["findings"].as_array().unwrap().is_empty());
    run(&["remove", "lib"]);
    assert!(!root.join("node_modules/lib").exists());
    run(&["add", "./lib"]);
    assert!(root.join("node_modules/lib/package.json").exists());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("package.json")).unwrap()).unwrap();
    assert_eq!(manifest["dependencies"]["lib"], "file:lib");
}
