//! The generated native HTTP client is checked as an application's own module.
#![cfg(feature = "upstream-typecheck")]

mod support;

use camino::Utf8Path;
use serde_json::Value;
use std::fs;
use support::uf;

#[test]
fn generated_handler_client_checks_paths_and_required_parameters() {
    let directory = tempfile::tempdir().unwrap();
    let root = Utf8Path::from_path(directory.path()).unwrap();
    fs::create_dir_all(root.join("app/api/users/[id]")).unwrap();
    fs::write(
        root.join("app/api/users/[id]/$route.js"),
        "export function GET() { return new Response('ok'); }",
    )
    .unwrap();
    uf_router::write_router_manifest(root, &uf_config::UniflowedConfig::default()).unwrap();
    // A local stub gives the generator's external transport its real public
    // shape without requiring npm or checking unrelated package declarations.
    fs::create_dir_all(root.join("node_modules/@uniflowed/router")).unwrap();
    fs::write(root.join("node_modules/@uniflowed/router/package.json"), r#"{"name":"@uniflowed/router","exports":{"./http-client":"./http-client.js","./routing":"./routing.js"}}"#).unwrap();
    fs::write(root.join("node_modules/@uniflowed/router/http-client.js"), r#"
        // @flow
        export type RouteClientOptions = { origin: string };
        export type RouteRequest = { method?: string };
        export function createRouteClient(options: RouteClientOptions): (path: string, request?: RouteRequest) => Promise<Response> {
          return async () => new Response();
        }
    "#).unwrap();
    fs::write(root.join("node_modules/@uniflowed/router/routing.js"), "// @flow\nexport function buildRoute(path: string, params?: {readonly [string]: mixed}): string { return path; }\n").unwrap();
    fs::write(
        root.join("consumer.js"),
        concat!(
            "// @flow\n",
            "import { createClient } from './router.js';\n",
            "const api = createClient({ origin: 'https://app.test' });\n",
            "api.request('/api/users/:id', { method: 'GET' }, { id: '42' });\n",
            "api.request('/not-a-handler', { method: 'GET' });\n",
            "api.request('/api/users/:id', { method: 'GET' });\n",
            "api.request('/api/users/:id', { method: 'GET' }, { id: 42 });\n",
        ),
    )
    .unwrap();
    let output = uf()
        .arg("--cwd")
        .arg(root)
        .args(["check", "--json"])
        .output()
        .unwrap();
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let errors = report["typeCheck"]["diagnostics"].as_array().unwrap();
    let lines: Vec<_> = errors
        .iter()
        .filter(|error| {
            error["primary"]["path"]
                .as_str()
                .unwrap_or("")
                .ends_with("consumer.js")
        })
        .filter_map(|error| error["primary"]["start"]["line"].as_u64())
        .collect();
    assert!(
        !lines.contains(&4),
        "correct client call was rejected: {report}"
    );
    for line in [5, 6, 7] {
        assert!(
            lines.contains(&line),
            "misuse at line {line} was accepted: {report}"
        );
    }
}
