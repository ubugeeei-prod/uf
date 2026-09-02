//! How imports and exports are linked inside and between chunks.

use super::fixture::{Fixture, assert_chunks_parse};

#[test]
fn an_imported_module_is_linked_inside_the_chunk() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const helper = () => 1;\n");
    fixture.entry(
        "app.js",
        "import { helper } from \"./util.js\";\nexport const answer = helper();\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(
        code.contains("const helper = __uf_m0[\"helper\"];"),
        "{code}"
    );
    assert!(!code.contains("from \"./util.js\""), "{code}");
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn every_module_keeps_its_own_scope() {
    let mut fixture = Fixture::new();
    fixture.write("a.js", "const styles = 1;\nexport const a = styles;\n");
    fixture.write("b.js", "const styles = 2;\nexport const b = styles;\n");
    fixture.entry(
        "app.js",
        "import { a } from \"./a.js\";\nimport { b } from \"./b.js\";\nexport const sum = a + b;\n",
    );

    let output = fixture.bundle();

    assert_eq!(output.chunks[0].code.matches("const styles").count(), 2);
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_default_export_becomes_a_namespace_entry() {
    let mut fixture = Fixture::new();
    fixture.write(
        "page.js",
        "export default function Page() {\n  return 1;\n}\n",
    );
    fixture.entry(
        "app.js",
        "import Page from \"./page.js\";\nexport const render = () => Page();\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("\"default\": Page"), "{code}");
    assert!(
        code.contains("const Page = __uf_m0[\"default\"];"),
        "{code}"
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn an_anonymous_default_export_gets_a_binding() {
    let mut fixture = Fixture::new();
    fixture.write("value.js", "export default { name: \"uf\" };\n");
    fixture.entry(
        "app.js",
        "import value from \"./value.js\";\nexport const name = value.name;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(
        code.contains("const __uf_default = { name: \"uf\" };"),
        "{code}"
    );
    assert!(code.contains("\"default\": __uf_default"), "{code}");
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_namespace_import_binds_the_whole_namespace() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const a = 1;\nexport const b = 2;\n");
    fixture.entry(
        "app.js",
        "import * as util from \"./util.js\";\nexport const sum = util.a + util.b;\n",
    );

    let output = fixture.bundle();

    assert!(output.chunks[0].code.contains("const util = __uf_m0;"));
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_renamed_import_keeps_both_names() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const helper = 1;\n");
    fixture.entry(
        "app.js",
        "import { helper as tool } from \"./util.js\";\nexport const answer = tool;\n",
    );

    let output = fixture.bundle();

    assert!(
        output.chunks[0]
            .code
            .contains("const tool = __uf_m0[\"helper\"];")
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_re_export_reads_through_to_the_source_module() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const helper = 1;\n");
    fixture.write("index.js", "export { helper } from \"./util.js\";\n");
    fixture.entry(
        "app.js",
        "import { helper } from \"./index.js\";\nexport const answer = helper;\n",
    );

    let output = fixture.bundle();

    assert!(
        output.chunks[0]
            .code
            .contains("\"helper\": __uf_m0[\"helper\"]")
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_star_re_export_spreads_the_source_namespace() {
    let mut fixture = Fixture::new();
    fixture.write("util.js", "export const helper = 1;\n");
    fixture.write("index.js", "export * from \"./util.js\";\n");
    fixture.entry(
        "app.js",
        "import { helper } from \"./index.js\";\nexport const answer = helper;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(code.contains("__uf_star"), "{code}");
    assert!(code.contains("const __uf_star = (source)"), "{code}");
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn an_external_import_is_hoisted_to_the_top_of_the_chunk() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "import { useState } from \"react\";\nexport const use = () => useState(0);\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(
        code.contains("import { useState as __uf_x0 } from \"react\";"),
        "{code}"
    );
    assert!(code.contains("const useState = __uf_x0;"), "{code}");
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn an_external_default_import_keeps_default_semantics() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "import React from \"react\";\nexport const node = React;\n",
    );

    let output = fixture.bundle();

    assert!(
        output.chunks[0]
            .code
            .contains("import __uf_x0 from \"react\";")
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_bare_external_import_is_kept_for_its_side_effects() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "import \"polyfill\";\nexport const a = 1;\n");

    let output = fixture.bundle();

    assert!(output.chunks[0].code.contains("import \"polyfill\";"));
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn identical_external_imports_are_emitted_once() {
    let mut fixture = Fixture::new();
    fixture.write(
        "a.js",
        "import { useState } from \"react\";\nexport const a = useState;\n",
    );
    fixture.entry(
        "app.js",
        "import { useState } from \"react\";\nimport { a } from \"./a.js\";\nexport const b = [useState, a];\n",
    );

    let output = fixture.bundle();

    assert_eq!(output.chunks[0].code.matches("from \"react\";").count(), 1);
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn an_entry_chunk_re_exports_the_entry_module_names() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "export const answer = 42;\nexport default answer;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    assert!(
        code.contains("export const answer = __uf_m0[\"answer\"];"),
        "{code}"
    );
    assert!(
        code.contains("export default __uf_m0[\"default\"];"),
        "{code}"
    );
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_cycle_between_modules_still_emits() {
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

    assert_eq!(output.chunks.len(), 1);
    assert_eq!(output.chunks[0].modules.len(), 3);
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn an_empty_module_bundles_to_an_empty_namespace() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "");

    let output = fixture.bundle();

    assert!(output.chunks[0].code.contains("return {};"));
    assert_chunks_parse(&output);
    fixture.keep();
}

#[test]
fn a_dynamic_import_is_left_untouched() {
    let mut fixture = Fixture::new();
    fixture.entry(
        "app.js",
        "export const load = () => import(\"./late.js\");\n",
    );
    fixture.write("late.js", "export const late = 1;\n");

    let output = fixture.bundle();

    assert!(output.chunks[0].code.contains("import(\"./late.js\")"));
    assert_chunks_parse(&output);
    fixture.keep();
}
