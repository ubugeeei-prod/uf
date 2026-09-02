//! `uf transform`, the service `@uniflowed/vite` pipes modules through.
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
fn a_flow_module_comes_back_as_javascript() {
    let dir = project();
    let replies = exchange(
        dir.path(),
        &[serde_json::json!({
            "id": "/app/main.js",
            "code": "// @flow\nexport function greet(who: string): string {\n  return who;\n}\n",
        })],
    );

    assert_eq!(replies.len(), 1);
    let code = replies[0]["code"].as_str().expect("transformed");
    assert!(!code.contains(": string"), "annotation survived:\n{code}");
    assert!(code.contains("function greet(who"), "body lost:\n{code}");
}

/// Replies are paired with requests by order, which is what the plugin relies
/// on — it keeps a queue of resolvers, not a map of ids.
#[test]
fn replies_come_back_in_the_order_the_requests_were_sent() {
    let dir = project();
    let requests: Vec<_> = (0..8)
        .map(|index| {
            serde_json::json!({
                "id": format!("/app/m{index}.js"),
                "code": format!("// @flow\nexport const value: number = {index};\n"),
            })
        })
        .collect();

    let replies = exchange(dir.path(), &requests);

    assert_eq!(replies.len(), requests.len());
    for (index, reply) in replies.iter().enumerate() {
        assert_eq!(reply["id"], format!("/app/m{index}.js"));
        let code = reply["code"].as_str().expect("transformed");
        assert!(code.contains(&format!("= {index};")), "{index}: {code}");
    }
}

/// A module needing no stage reports no code, so the caller keeps the original
/// source and its source map rather than taking a copy.
#[test]
fn a_module_with_nothing_to_do_reports_no_code() {
    let dir = project();
    let replies = exchange(
        dir.path(),
        &[serde_json::json!({"id": "/app/plain.js", "code": "export const a = 1;\n"})],
    );

    assert!(replies[0]["code"].is_null(), "{:?}", replies[0]);
    assert!(replies[0]["error"].is_null(), "{:?}", replies[0]);
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

/// A module that cannot be transformed reports the failure instead of silently
/// handing back something the bundler will misread.
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
