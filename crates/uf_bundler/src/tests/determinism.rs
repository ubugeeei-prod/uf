//! Building the same input twice must produce the same bytes.

use super::fixture::Fixture;

/// A project with enough shape that ordering could go wrong.
fn app() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.write("shared.js", "export const shared = () => 1;\n");
    fixture.write("zeta.js", "export const zeta = 26;\n");
    fixture.write("alpha.js", "export const alpha = 1;\n");
    fixture.write(
        "app/client/Counter.js",
        "\"use client\";\nimport { shared } from \"../../shared.js\";\nexport default function Counter() {\n  return shared();\n}\n",
    );
    fixture.entry(
        "app/_uf.page.js",
        "import { shared } from \"../shared.js\";\nimport { zeta } from \"../zeta.js\";\nimport { alpha } from \"../alpha.js\";\nimport Counter from \"./client/Counter.js\";\nexport default function Page() {\n  return [shared(), zeta, alpha, Counter];\n}\n",
    );
    fixture.entry(
        "app/about/_uf.page.js",
        "import { shared } from \"../../shared.js\";\nexport default function About() {\n  return shared();\n}\n",
    );
    fixture
}

#[test]
fn two_builds_of_the_same_input_are_byte_identical() {
    let fixture = app();

    let first = fixture.bundle();
    let second = fixture.bundle();

    assert_eq!(first.chunks.len(), second.chunks.len());
    for (left, right) in first.chunks.iter().zip(&second.chunks) {
        assert_eq!(left.file_name, right.file_name);
        assert_eq!(left.code, right.code, "chunk {} differs", left.file_name);
        assert_eq!(left.source_map, right.source_map);
        assert_eq!(left.modules, right.modules);
        assert_eq!(left.imports, right.imports);
    }
    fixture.keep();
}

#[test]
fn two_builds_with_source_maps_are_byte_identical() {
    let mut fixture = app();
    fixture.with_sourcemap();

    let first = fixture.bundle();
    let second = fixture.bundle();

    for (left, right) in first.chunks.iter().zip(&second.chunks) {
        assert_eq!(left.source_map, right.source_map);
    }
    fixture.keep();
}

#[test]
fn writing_a_bundle_twice_produces_the_same_files() {
    let fixture = app();
    let options = fixture.options();
    let container = fixture.container();

    let first = crate::write_bundle(&options, &fixture.bundle(), &container).expect("first write");
    let second =
        crate::write_bundle(&options, &fixture.bundle(), &container).expect("second write");

    assert_eq!(first, second);
    for path in &first {
        assert!(path.exists());
    }
    fixture.keep();
}

#[test]
fn declaring_the_same_imports_in_a_different_order_keeps_the_chunk_names() {
    let mut one = Fixture::new();
    one.write("a.js", "export const a = 1;\n");
    one.write("b.js", "export const b = 2;\n");
    one.entry(
        "app.js",
        "import { a } from \"./a.js\";\nimport { b } from \"./b.js\";\nexport const sum = a + b;\n",
    );

    let first = one.bundle();
    let second = one.bundle();

    assert_eq!(first.chunks[0].file_name, second.chunks[0].file_name);
    one.keep();
}

#[test]
fn changing_a_module_changes_the_chunk_hash() {
    let mut fixture = Fixture::new();
    fixture.entry("app.js", "export const a = 1;\n");
    let before = fixture.bundle().chunks[0].file_name.clone();

    fixture.write("app.js", "export const a = 2;\n");
    let after = fixture.bundle().chunks[0].file_name.clone();

    assert_ne!(before, after);
    fixture.keep();
}

#[test]
fn an_unrelated_change_keeps_a_chunk_hash() {
    let mut fixture = Fixture::new();
    fixture.write("unused.js", "export const unused = 1;\n");
    fixture.entry("app.js", "export const a = 1;\n");
    let before = fixture.bundle().chunks[0].file_name.clone();

    fixture.write("unused.js", "export const unused = 2;\n");
    let after = fixture.bundle().chunks[0].file_name.clone();

    assert_eq!(before, after);
    fixture.keep();
}

#[test]
fn namespace_entries_come_out_in_a_stable_order() {
    let mut fixture = Fixture::new();
    fixture.write(
        "util.js",
        "export const zeta = 1;\nexport const alpha = 2;\nexport const mid = 3;\n",
    );
    fixture.entry(
        "app.js",
        "import * as util from \"./util.js\";\nexport const all = util;\n",
    );

    let output = fixture.bundle();

    let code = &output.chunks[0].code;
    let alpha = code.find("\"alpha\"").expect("alpha");
    let mid = code.find("\"mid\"").expect("mid");
    let zeta = code.find("\"zeta\"").expect("zeta");
    assert!(alpha < mid && mid < zeta, "{code}");
    fixture.keep();
}

#[test]
fn a_module_symbol_is_derived_from_its_path() {
    let mut one = Fixture::new();
    one.write("lib/util.js", "export const a = 1;\n");
    one.write(
        "app/client/Counter.js",
        "\"use client\";\nimport { a } from \"../../lib/util.js\";\nexport default function Counter() {\n  return a;\n}\n",
    );
    one.entry(
        "app/_uf.page.js",
        "import Counter from \"./client/Counter.js\";\nexport default function Page() {\n  return Counter;\n}\n",
    );

    let first = one.bundle();
    let second = one.bundle();

    let left = first
        .chunk_of(camino::Utf8Path::new("lib/util.js"))
        .expect("chunk");
    let right = second
        .chunk_of(camino::Utf8Path::new("lib/util.js"))
        .expect("chunk");
    assert_eq!(left.code, right.code);
    assert!(left.code.contains(" as uf_"), "{}", left.code);
    one.keep();
}
