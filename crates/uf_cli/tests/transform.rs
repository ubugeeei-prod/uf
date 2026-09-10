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

/// Every component keeps the name its author gave it.
///
/// This is what React DevTools shows in the tree, and nothing else can supply
/// it: DevTools reads `type.name` off the function React is rendering, so a
/// component the pipeline renamed is a row in somebody's panel that says
/// nothing. The pipeline has three chances to lose it — Flow's `component`
/// lowering, the React Compiler's rewrite of the body, and the Fast Refresh
/// transform's registrations — and the compiler's is the one worth naming,
/// because it moves every value in a component into `t0`, `t1`, `$[0]` and
/// leaves the function it took them out of behind.
///
/// Three shapes, because they are lowered by three different paths: a declared
/// component, a default-exported one (the shape every `$page.js` uses), and
/// a plain arrow assigned to a `const`, whose name comes from JavaScript's own
/// inference rather than from a binding the transform wrote.
///
/// Asserted in the development configuration, because that is the only one
/// DevTools ever sees. See ubugeeei-prod/uf#503.
#[test]
fn a_component_keeps_its_name_through_the_compiler() {
    let dir = project();
    let development = serde_json::json!({ "development": true, "refresh": true });
    let replies = exchange(
        dir.path(),
        &[
            serde_json::json!({
                "id": "/app/Greeting.js",
                "code": "// @flow\nimport {useState} from 'react';\nexport component Greeting(name: string) {\n  const [seen, setSeen] = useState(0);\n  return <p onClick={() => setSeen(seen + 1)}>{name}{seen}</p>;\n}\n",
                "options": development,
            }),
            serde_json::json!({
                "id": "/app/$page.js",
                "code": "// @flow\nimport {useState} from 'react';\nexport default component Page() {\n  const [n] = useState(0);\n  return <main>{n}</main>;\n}\n",
                "options": development,
            }),
            serde_json::json!({
                "id": "/app/Row.js",
                "code": "// @flow\nconst Row = () => <li>row</li>;\nexport default Row;\n",
                "options": development,
            }),
        ],
    );

    let greeting = replies[0]["code"].as_str().expect("transformed");
    assert!(greeting.contains("function Greeting("), "{greeting}");
    // The compiler did run over it — otherwise this asserts that a name
    // survived a transform that never happened.
    assert!(greeting.contains("react/compiler-runtime"), "{greeting}");

    let page = replies[1]["code"].as_str().expect("transformed");
    assert!(page.contains("export default function Page("), "{page}");
    assert!(page.contains("react/compiler-runtime"), "{page}");

    // Not a declaration, so there is no name in the output to match on: what
    // gives this one its name is the assignment, and a transform that hoisted
    // the arrow into a temporary would take it away.
    let row = replies[2]["code"].as_str().expect("transformed");
    assert!(row.contains("const Row = () =>"), "{row}");
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

/// Entries a sweep should measure at their full size without a test machine
/// writing that much: `set_len` on an empty file is sparse, and the sweep
/// reads `metadata.len()`.
fn sparse_entry(path: &std::path::Path, bytes: u64, used_seconds_ago: u64) {
    let file = std::fs::File::create(path).unwrap();
    file.set_len(bytes).unwrap();
    // Backdated past the sweep's grace period, which is the rule that protects
    // a file a concurrent run may be writing. Without this every entry here
    // would be seconds old and correctly left alone.
    let when = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(used_seconds_ago))
        .unwrap();
    file.set_times(
        std::fs::FileTimes::new()
            .set_accessed(when)
            .set_modified(when),
    )
    .unwrap();
}

/// The transform cache is bounded, and starting the service is what bounds it.
///
/// `packages/host/internal/node-hooks.js` writes one `.mjs` per (compiler,
/// file, source) and has no way to take one back — a source edit orphans an
/// entry and a rebuild of `uf` orphans a generation, so `.uf/cache/transform`
/// grew without a ceiling until this. The sweep is here, at the door of the
/// only native process that knows the directory exists, and it runs when a
/// host starts one because compiling is the only thing that adds to it. See
/// ubugeeei-prod/uf#218.
#[test]
fn starting_the_service_brings_the_transform_cache_under_its_bound() {
    use uf_infra::cache::{MAX_CACHE_BYTES, SWEEP_TARGET_BYTES};

    let dir = project();
    let cache = dir.path().join(".uf").join("cache").join("transform");
    std::fs::create_dir_all(&cache).unwrap();

    // Ten entries over the cap, an hour apart: `age-0` is the coldest and
    // `age-9` the warmest. Named for their age rather than hashed, because
    // the sweep does not read a name and a reader of this test does.
    let each = MAX_CACHE_BYTES / 8;
    for index in 0..10u64 {
        sparse_entry(
            &cache.join(format!("age-{index}.mjs")),
            each,
            3_600 * (10 - index),
        );
    }
    assert!(each * 10 > MAX_CACHE_BYTES);

    exchange(
        dir.path(),
        &[serde_json::json!({ "id": "/app/main.js", "code": "// @flow\nexport const a = 1;\n" })],
    );

    let mut left = std::fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.metadata().unwrap().len(),
            )
        })
        .collect::<Vec<_>>();
    left.sort();
    let after: u64 = left.iter().map(|(_, bytes)| bytes).sum();

    assert!(
        after <= SWEEP_TARGET_BYTES,
        "the cache still holds {after} bytes, over the {SWEEP_TARGET_BYTES} the sweep targets"
    );
    // And it kept the *warmest* ones. An eviction that took the entries a
    // bisect is about to want would be a bound that made every build slower
    // rather than one that made the directory finite.
    let names = left.into_iter().map(|(name, _)| name).collect::<Vec<_>>();
    assert!(
        !names.contains(&String::from("age-0.mjs")),
        "the coldest entry survived: {names:?}"
    );
    assert!(
        names.contains(&String::from("age-9.mjs")),
        "the warmest entry was evicted: {names:?}"
    );
}
