//! Which exports and which modules survive.

use super::fixture::{Fixture, assert_chunks_parse};

#[test]
fn an_export_nothing_imports_is_dropped_from_the_namespace() {
    let mut fixture = Fixture::new();
    fixture.write(
        "util.js",
        "export const used = 1;\nexport const unused = 2;\n",
    );
    fixture.entry(
        "app.js",
        "import { used } from \"./util.js\";\nexport const answer = used;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("\"used\": used"), "{code}");
    assert!(!code.contains("\"unused\""), "{code}");
    fixture.keep();
}

#[test]
fn a_module_nothing_reaches_is_dropped_entirely() {
    let mut fixture = Fixture::new();
    fixture.write("orphan.js", "export const orphan = 1;\n");
    fixture.entry("app.js", "export const answer = 1;\n");

    let output = fixture.bundle();

    assert!(!output.chunks[0].code.contains("orphan"));
    assert_eq!(output.chunks[0].modules.len(), 1);
    fixture.keep();
}

#[test]
fn a_module_whose_exports_are_all_unused_is_dropped() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const helper = 1;\n");
    fixture.entry(
        "app.js",
        "import \"./util.js\";\nexport const answer = 1;\n",
    );

    let output = fixture.bundle();

    assert!(
        !output.chunks[0].code.contains("helper"),
        "{}",
        output.chunks[0].code
    );
    fixture.keep();
}

#[test]
fn a_module_with_a_top_level_side_effect_is_kept() {
    let mut fixture = Fixture::new();
    fixture.write("polyfill.js", "globalThis.patched = true;\n");
    fixture.entry(
        "app.js",
        "import \"./polyfill.js\";\nexport const answer = 1;\n",
    );

    let output = fixture.bundle();

    assert!(output.chunks[0].code.contains("globalThis.patched"));
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_side_effect_free_package_lets_a_bare_import_be_dropped() {
    let mut fixture = Fixture::new();
    fixture.write(
        "node_modules/lib/package.json",
        "{\"name\":\"lib\",\"sideEffects\":false}",
    );
    fixture.write("node_modules/lib/index.js", "register();\n");
    fixture.entry("app.js", "import \"lib\";\nexport const answer = 1;\n");

    let output = fixture.bundle();

    assert!(
        !output.chunks[0].code.contains("register()"),
        "{}",
        output.chunks[0].code
    );
    fixture.keep();
}

#[test]
fn a_package_without_a_side_effects_field_keeps_its_side_effects() {
    let mut fixture = Fixture::new();
    fixture.write("node_modules/lib/package.json", "{\"name\":\"lib\"}");
    fixture.write("node_modules/lib/index.js", "register();\n");
    fixture.entry("app.js", "import \"lib\";\nexport const answer = 1;\n");

    let output = fixture.bundle();

    assert!(output.chunks[0].code.contains("register()"));
    fixture.keep();
}

#[test]
fn a_namespace_import_keeps_every_export() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const a = 1;\nexport const b = 2;\n");
    fixture.entry(
        "app.js",
        "import * as util from \"./util.js\";\nexport const all = util;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("\"a\": a"), "{code}");
    assert!(code.contains("\"b\": b"), "{code}");
    fixture.keep();
}

#[test]
fn a_re_export_chain_pulls_only_the_name_that_is_used() {
    let mut fixture = Fixture::new();
    fixture.write(
        "util.js",
        "export const used = 1;\nexport const unused = 2;\n",
    );
    fixture.write("index.js", "export { used, unused } from \"./util.js\";\n");
    fixture.entry(
        "app.js",
        "import { used } from \"./index.js\";\nexport const answer = used;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("\"used\""), "{code}");
    assert!(!code.contains("\"unused\""), "{code}");
    fixture.keep();
}

#[test]
fn an_entry_module_keeps_every_export() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "export const a = 1;\nexport const b = 2;\n");

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("export const a ="), "{code}");
    assert!(code.contains("export const b ="), "{code}");
    fixture.keep();
}

#[test]
fn a_client_root_keeps_every_export() {
    let mut fixture = Fixture::new();
    fixture.write(
        "app/client/Counter.js",
        "\"use client\";\nexport const helper = 1;\nexport default function Counter() {\n  return 1;\n}\n",
    );
    fixture.entry(
        "app/_uf.page.js",
        "import Counter from \"./client/Counter.js\";\nexport default function Page() {\n  return Counter;\n}\n",
    );

    let output = fixture.bundle();

    let client = output
        .chunk_of(camino::Utf8Path::new("app/client/Counter.js"))
        .expect("client chunk");
    assert!(
        client.code.contains("\"helper\": helper"),
        "{}",
        client.code
    );
    fixture.keep();
}

#[test]
fn a_star_re_export_keeps_every_export_of_its_source() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const a = 1;\nexport const b = 2;\n");
    fixture.write("index.js", "export * from \"./util.js\";\n");
    fixture.entry(
        "app.js",
        "import { a } from \"./index.js\";\nexport const answer = a;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("\"a\": a"), "{code}");
    assert!(code.contains("\"b\": b"), "{code}");
    fixture.keep();
}

#[test]
fn a_deep_chain_of_unused_modules_is_dropped_whole() {
    let mut fixture = Fixture::new();
    fixture.write("c.js", "export const c = 1;\n");
    fixture.write("b.js", "export { c } from \"./c.js\";\n");
    fixture.write("a.js", "export { c } from \"./b.js\";\n");
    fixture.entry("app.js", "export const answer = 1;\n");

    let output = fixture.bundle();

    assert_eq!(output.stats.modules_loaded, 1);
    assert_eq!(output.stats.modules_kept, 1);
    fixture.keep();
}

#[test]
fn shaking_terminates_on_a_cyclic_graph() {
    let mut fixture = Fixture::new();
    fixture.write(
        "a.js",
        "import { b } from \"./b.js\";\nexport const a = () => b();\n",
    );
    fixture.write(
        "b.js",
        "import { a } from \"./a.js\";\nexport const b = () => a();\n",
    );
    fixture.entry(
        "app.js",
        "import { a } from \"./a.js\";\nexport const run = a;\n",
    );

    let output = fixture.bundle();

    assert_eq!(output.stats.modules_kept, 3);
    fixture.keep();
}

#[test]
fn a_default_import_only_keeps_the_default_export() {
    let mut fixture = Fixture::new();
    fixture.write(
        "page.js",
        "export const helper = 1;\nexport default function Page() {\n  return 1;\n}\n",
    );
    fixture.entry(
        "app.js",
        "import Page from \"./page.js\";\nexport const render = Page;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("\"default\": Page"), "{code}");
    assert!(!code.contains("\"helper\""), "{code}");
    fixture.keep();
}
