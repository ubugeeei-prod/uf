//! JSX must not survive a build.
//!
//! `uf build` once emitted chunks that its own suite called valid, because the
//! suite re-parsed them with a *Flow* parser and Flow's grammar includes JSX.
//! The output was unloadable and every test passed. These are the tests that
//! would have caught it: they ask whether a JSX token survives, which is a
//! property of the bytes rather than of a front end that shares the bug.

use super::fixture::{Fixture, assert_chunks_parse, assert_no_jsx, chunk_named};

/// A project shaped like the scaffolded app: Flow, a client boundary, JSX.
fn app() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.write(
        "app/client/Counter.js",
        "\"use client\";\n// @flow\nimport { useState } from \"@uniflowed/react\";\n\ncomponent Counter(initial: number) renders Node {\n  const [count, setCount] = useState(initial);\n  return <button onClick={() => setCount(count + 1)}>count: {count}</button>;\n}\n\nexport default Counter;\n",
    );
    fixture.entry(
        "app/_uf.page.js",
        "// @flow\nimport Counter from \"./client/Counter.js\";\n\ncomponent Page() renders Node {\n  return (\n    <main className=\"shell\">\n      <h1>title</h1>\n      <Counter initial={1} />\n    </main>\n  );\n}\n\nexport default Page;\n",
    );
    fixture
}

#[test]
fn no_chunk_holds_jsx() {
    let fixture = app();

    let output = fixture.bundle();

    assert!(!output.chunks.is_empty());
    for chunk in &output.chunks {
        assert_no_jsx(&chunk.file_name, &chunk.code);
    }
    fixture.keep();
}

#[test]
fn an_element_becomes_a_runtime_call() {
    let fixture = app();

    let output = fixture.bundle();

    let entry = chunk_named(&output, "entry-app");
    assert!(entry.code.contains("_jsx"), "{}", entry.code);
    assert!(!entry.code.contains("<main"), "{}", entry.code);
    fixture.keep();
}

#[test]
fn the_runtime_import_is_linked_like_any_other() {
    let fixture = app();

    let output = fixture.bundle();

    let entry = chunk_named(&output, "entry-app");
    assert!(
        entry.code.contains("from \"@uniflowed/jsx-runtime\""),
        "{}",
        entry.code
    );
    fixture.keep();
}

#[test]
fn a_client_boundary_chunk_is_lowered_too() {
    let fixture = app();

    let output = fixture.bundle();

    let client = output
        .chunk_of(camino::Utf8Path::new("app/client/Counter.js"))
        .expect("the client chunk");
    assert_no_jsx(&client.file_name, &client.code);
    assert!(client.code.contains("_jsx"), "{}", client.code);
    fixture.keep();
}

#[test]
fn lowering_keeps_the_module_line_count() {
    let fixture = app();

    let output = fixture.bundle();

    // Each module's body still occupies the lines it did in its source, which
    // is what the chunk's per-line source map depends on.
    let entry = chunk_named(&output, "entry-app");
    let source = std::fs::read_to_string(fixture.path("app/_uf.page.js")).expect("read");
    let body_lines = entry
        .code
        .lines()
        .skip_while(|line| !line.contains("__uf_init"))
        .count();
    assert!(body_lines >= source.lines().count(), "{}", entry.code);
    fixture.keep();
}

#[test]
fn a_project_with_no_jsx_still_builds() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "// @flow\nexport const answer: number = 42;\n");

    let output = fixture.bundle();

    assert!(!output.chunks[0].code.contains("_jsx"));
    assert!(
        !output.chunks[0].code.contains("jsx-runtime"),
        "a module with no JSX must not import the runtime"
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_fragment_survives_a_build() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "// @flow\ncomponent Page() renders Node {\n  return <><span>a</span><span>b</span></>;\n}\nexport default Page;\n",
    );

    let output = fixture.bundle();

    assert!(
        output.chunks[0].code.contains("_Fragment"),
        "{}",
        output.chunks[0].code
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_keyed_list_survives_a_build() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "// @flow\ncomponent List(items: Array<string>) renders Node {\n  return <ul>{items.map((i) => <li key={i}>{i}</li>)}</ul>;\n}\nexport default List;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("_jsx(\"li\""), "{code}");
    assert!(
        code.contains(", i)"),
        "the key must be the third argument\n{code}"
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn lowering_is_deterministic() {
    let fixture = app();

    let first = fixture.bundle();
    let second = fixture.bundle();

    for (left, right) in first.chunks.iter().zip(&second.chunks) {
        assert_eq!(left.file_name, right.file_name);
        assert_eq!(left.code, right.code);
    }
    fixture.keep();
}
