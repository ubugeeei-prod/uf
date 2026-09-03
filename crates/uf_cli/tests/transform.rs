//! `uf transform`, the service `@uniflowed/vite` and the host loaders pipe
//! modules through.
//!
//! The protocol is what a Vite build depends on, so these exercise it as a
//! caller does: write requests, read replies, in order.

mod support;

use std::io::Write;
use std::process::Stdio;

use support::uf_path;

/// Send `requests` through one `uf transform` process and collect the replies.
fn exchange(dir: &std::path::Path, requests: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut child = std::process::Command::new(uf_path())
        .arg("--cwd")
        .arg(dir)
        .arg("transform")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "uf transform failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("a reply is one JSON object"))
        .collect()
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("uf.config.js"), "export default {};\n").unwrap();
    dir
}

#[test]
fn a_flow_module_comes_back_as_javascript_with_a_map() {
    let dir = project();
    let replies = exchange(
        dir.path(),
        &[serde_json::json!({
            "id": "/app/main.js",
            "code": "// @flow\nexport function greet(who: string): string {\n  return who;\n}\n",
        })],
    );

    let code = replies[0]["code"].as_str().expect("transformed");
    assert!(code.contains("function greet(who)"), "{code}");
    assert!(!code.contains(": string"), "{code}");
    let map: serde_json::Value =
        serde_json::from_str(replies[0]["map"].as_str().expect("a map")).unwrap();
    assert_eq!(map["sources"][0], "/app/main.js");
}

/// Flow's own syntax — components, `match`, enums, JSX — comes out as the
/// JavaScript Flow specifies, memoised by the official React Compiler.
#[test]
fn modern_flow_syntax_is_lowered_and_compiled() {
    let dir = project();
    let replies = exchange(
        dir.path(),
        &[serde_json::json!({
            "id": "/app/Toggle.js",
            "code": "// @flow\nimport {useState} from 'react';\nenum Mode { On, Off }\nexport component Toggle(label: string) {\n  const [mode, setMode] = useState<Mode>(Mode.On);\n  const text = match (mode) { Mode.On => 'on', Mode.Off => 'off' };\n  return <button onClick={() => setMode(Mode.Off)}>{label}: {text}</button>;\n}\n",
        })],
    );

    let code = replies[0]["code"].as_str().expect("transformed");
    assert!(code.contains("function Toggle"), "{code}");
    assert!(code.contains("$$ufEnumMirrored"), "{code}");
    assert!(code.contains("react/compiler-runtime"), "{code}");
    assert!(code.contains("react/jsx-runtime"), "{code}");
    assert!(!code.contains("match ("), "{code}");
    assert!(!code.contains("component "), "{code}");
}

/// The order *is* the protocol: a caller pairs replies with requests by
/// position.
#[test]
fn replies_arrive_in_request_order() {
    let dir = project();
    let requests: Vec<serde_json::Value> = (0..8)
        .map(|index| {
            serde_json::json!({
                "id": format!("/app/m{index}.js"),
                "code": format!("export const v{index}: number = {index};\n"),
            })
        })
        .collect();

    let replies = exchange(dir.path(), &requests);

    assert_eq!(replies.len(), 8);
    for (index, reply) in replies.iter().enumerate() {
        assert_eq!(reply["id"], format!("/app/m{index}.js"));
        let code = reply["code"].as_str().expect("transformed");
        assert!(code.contains(&format!("v{index} = {index}")), "{code}");
    }
}

/// Development requests get readable output and Fast Refresh registrations.
#[test]
fn development_output_registers_for_fast_refresh() {
    let dir = project();
    let replies = exchange(
        dir.path(),
        &[serde_json::json!({
            "id": "/app/App.js",
            "code": "// @flow\nexport component App() { return <p>hi</p>; }\n",
            "options": { "development": true, "refresh": true },
        })],
    );

    let code = replies[0]["code"].as_str().expect("transformed");
    assert!(code.contains("$RefreshReg$"), "{code}");
    assert!(code.contains("jsxDEV"), "{code}");
}

/// uf's own packages ship Flow, so they are transformed even under
/// `node_modules`; a third-party package is already JavaScript and is not.
#[test]
fn only_uf_packages_are_transformed_under_node_modules() {
    let dir = project();
    let source = "// @flow\nexport const value: number = 1;\n";
    let replies = exchange(
        dir.path(),
        &[
            serde_json::json!({"id": "/app/node_modules/@uniflowed/react/index.js", "code": source}),
            serde_json::json!({"id": "/app/node_modules/other/index.js", "code": source}),
        ],
    );

    assert!(
        replies[0]["code"].as_str().is_some(),
        "uf package not transformed: {:?}",
        replies[0]
    );
    assert!(
        replies[1]["code"].is_null(),
        "third party transformed: {:?}",
        replies[1]
    );
}

/// A syntax error names its position, so a build can point at the line.
#[test]
fn a_syntax_error_is_reported_with_its_position() {
    let dir = project();
    let replies = exchange(
        dir.path(),
        &[serde_json::json!({"id": "/app/bad.js", "code": "// @flow\nconst a = ;\n"})],
    );

    assert!(replies[0]["error"].as_str().is_some(), "{:?}", replies[0]);
    assert_eq!(replies[0]["line"], 2);
}

/// A request that cannot be read is reported instead of silently dropped.
#[test]
fn a_malformed_request_is_reported_rather_than_ignored() {
    let dir = project();
    let mut child = std::process::Command::new(uf_path())
        .arg("--cwd")
        .arg(dir.path())
        .arg("transform")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.as_mut().unwrap(), "not json at all").unwrap();
    let output = child.wait_with_output().unwrap();

    let reply: serde_json::Value =
        serde_json::from_str(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
    assert!(
        reply["error"]
            .as_str()
            .unwrap_or_default()
            .contains("malformed"),
        "{reply:?}"
    );
}
