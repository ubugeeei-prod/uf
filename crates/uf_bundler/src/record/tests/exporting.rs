//! Export statements, in every shape a module can declare one.

use super::super::*;
use super::{record, rewritten};

#[test]
fn a_module_with_nothing_in_it_records_nothing() {
    let record = record("");

    assert!(record.imports.is_empty());
    assert!(record.exports.is_empty());
    assert!(record.patches.is_empty());
    assert_eq!(record.side_effects, SideEffectKind::None);
}

#[test]
fn an_exported_const_records_its_name_and_loses_its_keyword() {
    let source = "export const answer = 42;\n";

    let record = record(source);

    assert_eq!(record.exports[0].exported, "answer");
    assert_eq!(
        record.exports[0].source,
        ExportSource::Local {
            local: "answer".into()
        }
    );
    assert_eq!(rewritten(source), "       const answer = 42;\n");
}

#[test]
fn an_exported_declarator_list_records_every_name() {
    let record = record("export const a = 1, b = 2, c = 3;\n");

    let names: Vec<&str> = record
        .exports
        .iter()
        .map(|export| export.exported.as_str())
        .collect();
    assert_eq!(names, vec!["a", "b", "c"]);
}

#[test]
fn an_exported_function_records_its_name() {
    let record = record("export function make() {}\n");

    assert_eq!(record.exports[0].exported, "make");
}

#[test]
fn an_exported_async_function_records_its_name() {
    let record = record("export async function make() {}\n");

    assert_eq!(record.exports[0].exported, "make");
}

#[test]
fn an_exported_class_records_its_name() {
    let record = record("export class Box {}\n");

    assert_eq!(record.exports[0].exported, "Box");
}

#[test]
fn a_named_export_clause_maps_exported_names_to_locals() {
    let record = record("const a = 1;\nexport { a as answer };\n");

    assert_eq!(record.exports[0].exported, "answer");
    assert_eq!(
        record.exports[0].source,
        ExportSource::Local { local: "a".into() }
    );
}

#[test]
fn a_named_export_clause_is_blanked() {
    let out = rewritten("const a = 1;\nexport { a };\n");

    assert_eq!(out, "const a = 1;\n             \n");
}

#[test]
fn a_re_export_records_the_source_module() {
    let record = record("export { a as b } from \"./x.js\";\n");

    assert_eq!(record.imports[0].form, ImportForm::ReExport);
    assert_eq!(record.exports[0].exported, "b");
    assert_eq!(
        record.exports[0].source,
        ExportSource::Reexport {
            import: 0,
            imported: "a".into()
        }
    );
}

#[test]
fn a_star_re_export_is_recorded_separately() {
    let record = record("export * from \"./x.js\";\n");

    assert_eq!(record.star_reexports, vec![0]);
    assert!(record.exports.is_empty());
}

#[test]
fn a_named_star_re_export_records_the_namespace_name() {
    let record = record("export * as ns from \"./x.js\";\n");

    assert!(record.star_reexports.is_empty());
    assert_eq!(record.exports[0].exported, "ns");
    assert_eq!(
        record.exports[0].source,
        ExportSource::Reexport {
            import: 0,
            imported: "*".into()
        }
    );
}

#[test]
fn exports_name_answers_what_a_module_publishes() {
    let record = record("export const a = 1;\n");

    assert!(record.exports_name("a"));
    assert!(!record.exports_name("b"));
}
