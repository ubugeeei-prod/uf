//! Import and export statements, and the type-only pieces of them.

use super::stripped_text;

#[test]
fn exported_type_aliases_are_erased_with_their_export_keyword() {
    let out = stripped_text("// @flow\nexport type Id = string;\nexport const a = 1;\n");

    assert!(!out.contains("export type"), "{out}");
    assert!(out.contains("export const a = 1;"), "{out}");
}

#[test]
fn import_type_statements_are_erased() {
    let out = stripped_text("// @flow\nimport type { Id } from \"./id.js\";\nconst a = 1;\n");

    assert!(!out.contains("import"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn import_typeof_statements_are_erased() {
    let out = stripped_text("// @flow\nimport typeof Thing from \"./thing.js\";\nconst a = 1;\n");

    assert!(!out.contains("import"), "{out}");
}

#[test]
fn a_default_import_named_type_survives() {
    let out = stripped_text("// @flow\nimport type from \"./type.js\";\nconst a = type;\n");

    assert!(out.contains("import type from"), "{out}");
}

#[test]
fn type_specifiers_are_removed_from_a_mixed_import() {
    let out = stripped_text("// @flow\nimport { type Id, make } from \"./id.js\";\nmake();\n");

    assert!(!out.contains("type Id"), "{out}");
    assert!(out.contains("make"), "{out}");
    assert!(out.contains("import {"), "{out}");
}

#[test]
fn a_trailing_type_specifier_takes_its_comma_with_it() {
    let out = stripped_text("// @flow\nimport { make, type Id } from \"./id.js\";\nmake();\n");

    assert!(!out.contains("Id"), "{out}");
    assert!(out.contains("make"), "{out}");
    assert!(!out.contains(",  }"), "{out}");
}

#[test]
fn an_import_of_only_types_is_erased_whole() {
    let out = stripped_text("// @flow\nimport { type A, type B } from \"./t.js\";\nconst a = 1;\n");

    assert!(!out.contains("import"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn type_specifiers_are_removed_from_a_default_and_named_import() {
    let source = "// @flow\nimport Thing, { type Id, make } from \"./id.js\";\nmake(Thing);\n";

    let out = stripped_text(source);

    assert!(!out.contains("Id"), "{out}");
    assert!(out.contains("import Thing, {"), "{out}");
}

#[test]
fn export_type_statements_are_erased() {
    let out = stripped_text("// @flow\nexport type { Id } from \"./id.js\";\nconst a = 1;\n");

    assert!(!out.contains("export type"), "{out}");
    assert!(out.contains("const a = 1;"), "{out}");
}

#[test]
fn type_specifiers_are_removed_from_a_named_export() {
    let out = stripped_text("// @flow\nconst make = 1;\nexport { type Id, make };\n");

    assert!(!out.contains("Id"), "{out}");
    assert!(out.contains("export {"), "{out}");
    assert!(out.contains("make"), "{out}");
}

#[test]
fn an_export_default_function_body_is_not_mistaken_for_a_specifier_list() {
    let source =
        "// @flow\nexport default function main() {\n  const type = 1;\n  return type;\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("const type = 1;"), "{out}");
    assert!(out.contains("return type;"), "{out}");
}

#[test]
fn an_exported_component_keeps_its_export_keyword() {
    let source = "// @flow\nexport component Page(a: number) renders Node {\n  return a;\n}\n";

    let out = stripped_text(source);

    assert!(out.contains("export function Page({a}) {"), "{out}");
}
