//! What the emitted JavaScript looks like, and that it is JavaScript.

use super::fixture::{Fixture, assert_chunks_parse, chunk_named};

#[test]
fn a_single_module_entry_becomes_one_chunk() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "export const answer = 42;\n");

    let output = fixture.bundle();

    assert_eq!(output.chunks.len(), 1);
    assert!(output.chunks[0].file_name.starts_with("assets/entry-app"));
    assert!(output.chunks[0].file_name.ends_with(".js"));
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn flow_types_never_reach_the_output() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "// @flow\nexport type Id = string;\nexport function make(raw: string): Id {\n  return raw;\n}\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(!code.contains("export type"), "{code}");
    assert!(!code.contains(": Id"), "{code}");
    assert!(code.contains("function make(raw"), "{code}");
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_component_declaration_is_rewritten_to_a_function() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "// @flow\ncomponent Page(name: string) renders Node {\n  return name;\n}\nexport default Page;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("Page({name"), "{code}");
    assert!(!code.contains("component "), "{code}");
    assert!(!code.contains("renders"), "{code}");
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_directive_prologue_is_blanked_out_of_the_chunk() {
    let mut fixture = Fixture::new();
    fixture.write("counter.js", "\"use client\";\nexport const count = 1;\n");
    fixture.entry(
        "app.js",
        "import { count } from \"./counter.js\";\nexport const total = count;\n",
    );

    let output = fixture.bundle();

    for chunk in &output.chunks {
        assert!(!chunk.code.contains("\"use client\""), "{}", chunk.code);
    }
    assert_chunks_parse(&output);
    fixture.keep();
}

/// JSX must be lowered by the time it reaches a chunk.
///
/// This test used to assert the opposite — that `<p>hello</p>` survived into
/// the output — and passed, because the only check on the emitted code was a
/// Flow parser, and Flow's grammar includes JSX. See `tests/lowering.rs`.
#[test]
fn jsx_is_lowered_before_it_reaches_a_chunk() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "// @flow\ncomponent Page() {\n  return <main><p>hello</p></main>;\n}\nexport default Page;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(!code.contains("<p>hello</p>"), "{code}");
    assert!(
        code.contains("_jsx(\"p\", {children: \"hello\"})"),
        "{code}"
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_crlf_module_bundles_without_losing_lines() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "// @flow\r\nexport const a: number = 1;\r\nexport const b = 2;\r\n",
    );

    let output = fixture.bundle();

    assert!(output.chunks[0].code.contains("export const a"));
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_module_with_a_byte_order_mark_bundles() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "\u{feff}export const a = 1;\n");

    let output = fixture.bundle();

    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn non_ascii_source_survives_bundling() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "export const café = \"caffè ☕\";\n");

    let output = fixture.bundle();

    assert!(output.chunks[0].code.contains("caffè ☕"));
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_module_with_only_comments_bundles() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "// nothing to see here\n/* or here */\n");

    let output = fixture.bundle();

    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn writing_a_bundle_puts_every_chunk_on_disk() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "export const a = 1;\n");

    let output = fixture.bundle();
    let written = crate::write_bundle(&fixture.options(), &output, &fixture.container())
        .expect("write succeeds");

    assert_eq!(written.len(), 1);
    assert!(written[0].exists());
    assert!(
        std::fs::read_to_string(&written[0])
            .expect("read chunk")
            .contains("export const a")
    );
    fixture.keep();
}

#[test]
fn the_stats_count_what_the_build_did() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const a = 1;\nexport const unused = 2;\n");
    fixture.entry(
        "app.js",
        "import { a } from \"./util.js\";\nexport const answer = a;\n",
    );

    let output = fixture.bundle();

    assert_eq!(output.stats.modules_loaded, 2);
    assert_eq!(output.stats.modules_kept, 2);
    assert_eq!(output.stats.chunks, 1);
    assert_eq!(output.stats.exports_dropped, 1);
    fixture.keep();
}

#[test]
fn an_entry_with_no_modules_produces_no_chunks() {
    let fixture = Fixture::new();

    let output = fixture.bundle();

    assert!(output.chunks.is_empty());
    assert_eq!(output.stats.modules_loaded, 0);
    fixture.keep();
}

#[test]
fn a_chunk_never_holds_the_same_module_twice() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const a = 1;\n");
    fixture.write("one.js", "export { a } from \"./util.js\";\n");
    fixture.write("two.js", "export { a as b } from \"./util.js\";\n");
    fixture.entry(
        "app.js",
        "import { a } from \"./one.js\";\nimport { b } from \"./two.js\";\nexport const sum = a + b;\n",
    );

    let output = fixture.bundle();

    let chunk = chunk_named(&output, "entry-app");
    let mut modules = chunk.modules.clone();
    modules.sort();
    modules.dedup();
    assert_eq!(modules.len(), chunk.modules.len());
    fixture.keep();
}
