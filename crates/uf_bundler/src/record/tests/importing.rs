//! Import statements: their bindings, their forms, their spans.

use super::super::*;
use super::{record, rewritten};

#[test]
fn a_side_effect_import_records_no_bindings() {
    let record = record("import \"./polyfill.js\";\n");

    assert_eq!(record.imports.len(), 1);
    assert_eq!(record.imports[0].specifier, "./polyfill.js");
    assert_eq!(record.imports[0].form, ImportForm::Static);
    assert!(record.imports[0].bindings.is_empty());
}

#[test]
fn a_default_import_records_its_local_name() {
    let record = record("import Page from \"./page.js\";\n");

    assert_eq!(
        record.imports[0].bindings[0],
        ImportBinding::Default {
            local: "Page".into()
        }
    );
}

#[test]
fn a_named_import_records_both_names() {
    let record = record("import { a, b as c } from \"./x.js\";\n");

    assert_eq!(
        record.imports[0].bindings.as_slice(),
        &[
            ImportBinding::Named {
                imported: "a".into(),
                local: "a".into()
            },
            ImportBinding::Named {
                imported: "b".into(),
                local: "c".into()
            },
        ]
    );
}

#[test]
fn a_namespace_import_records_its_local_name() {
    let record = record("import * as ns from \"./x.js\";\n");

    assert_eq!(
        record.imports[0].bindings[0],
        ImportBinding::Namespace { local: "ns".into() }
    );
}

#[test]
fn a_default_and_named_import_records_both() {
    let record = record("import Page, { a } from \"./x.js\";\n");

    assert_eq!(record.imports[0].bindings.len(), 2);
}

#[test]
fn a_default_and_namespace_import_records_both() {
    let record = record("import Page, * as ns from \"./x.js\";\n");

    assert_eq!(record.imports[0].bindings.len(), 2);
}

#[test]
fn an_import_statement_is_blanked() {
    let out = rewritten("import { a } from \"./x.js\";\nconst b = a;\n");

    assert_eq!(out, "                           \nconst b = a;\n");
}

#[test]
fn a_multi_line_import_keeps_its_line_count() {
    let source = "import {\n  a,\n  b,\n} from \"./x.js\";\nconst c = a + b;\n";

    let out = rewritten(source);

    assert_eq!(out.lines().count(), source.lines().count());
    assert!(out.contains("const c = a + b;"));
}

#[test]
fn a_dynamic_import_is_recorded_but_not_blanked() {
    let source = "const load = () => import(\"./late.js\");\n";

    let record = record(source);

    assert_eq!(record.imports[0].form, ImportForm::Dynamic);
    assert!(record.patches.is_empty());
    assert_eq!(rewritten(source), source);
}

#[test]
fn a_dynamic_import_at_statement_position_is_recorded() {
    let record = record("import(\"./late.js\");\n");

    assert_eq!(record.imports[0].form, ImportForm::Dynamic);
    assert_eq!(record.imports[0].specifier, "./late.js");
}

#[test]
fn a_require_call_is_recorded() {
    let record = record("const x = require(\"./cjs.js\");\n");

    assert_eq!(record.imports[0].form, ImportForm::Require);
    assert_eq!(record.imports[0].specifier, "./cjs.js");
}

#[test]
fn a_dynamic_form_is_never_linked() {
    assert!(ImportForm::Static.is_linked());
    assert!(ImportForm::ReExport.is_linked());
    assert!(!ImportForm::Dynamic.is_linked());
    assert!(!ImportForm::Require.is_linked());
}

#[test]
fn an_exported_object_pattern_records_its_bindings() {
    let record = record("export const { a, b: c } = source;\n");

    let names: Vec<&str> = record
        .exports
        .iter()
        .map(|export| export.exported.as_str())
        .collect();
    assert_eq!(names, vec!["a", "c"]);
}

#[test]
fn an_exported_array_pattern_records_its_bindings() {
    let record = record("export const [a, b] = source;\n");

    let names: Vec<&str> = record
        .exports
        .iter()
        .map(|export| export.exported.as_str())
        .collect();
    assert_eq!(names, vec!["a", "b"]);
}

#[test]
fn an_anonymous_default_export_becomes_a_binding() {
    let source = "export default { name: \"uf\" };\n";

    let record = record(source);

    assert_eq!(
        record.exports[0].source,
        ExportSource::Local {
            local: DEFAULT_LOCAL.into()
        }
    );
    assert_eq!(
        rewritten(source),
        "const __uf_default = { name: \"uf\" };\n"
    );
}

#[test]
fn an_anonymous_default_function_becomes_a_binding() {
    let record = record("export default function () {}\n");

    assert_eq!(
        record.exports[0].source,
        ExportSource::Local {
            local: DEFAULT_LOCAL.into()
        }
    );
}

#[test]
fn an_import_inside_a_string_is_not_recorded() {
    let record = record("const text = \"import { a } from './x.js';\";\n");

    assert!(record.imports.is_empty());
}

#[test]
fn an_import_inside_a_comment_is_not_recorded() {
    let record = record("// import { a } from \"./x.js\";\nconst b = 1;\n");

    assert!(record.imports.is_empty());
}

#[test]
fn an_import_with_a_string_specifier_name_records_the_name() {
    let record = record("import { \"a-b\" as ab } from \"./x.js\";\n");

    assert_eq!(
        record.imports[0].bindings[0],
        ImportBinding::Named {
            imported: "a-b".into(),
            local: "ab".into()
        }
    );
}

#[test]
fn a_binding_reports_its_local_name() {
    assert_eq!(ImportBinding::Default { local: "a".into() }.local(), "a");
    assert_eq!(ImportBinding::Namespace { local: "b".into() }.local(), "b");
    assert_eq!(
        ImportBinding::Named {
            imported: "c".into(),
            local: "d".into()
        }
        .local(),
        "d"
    );
}
