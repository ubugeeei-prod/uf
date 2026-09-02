//! Side-effect detection, rewrite spans, and awkward encodings.

use super::super::*;
use super::{record, rewritten};

#[test]
fn a_default_function_export_keeps_its_declaration() {
    let source = "export default function Page() {}\n";

    let record = record(source);

    assert_eq!(record.exports[0].exported, "default");
    assert_eq!(
        record.exports[0].source,
        ExportSource::Local {
            local: "Page".into()
        }
    );
    assert_eq!(rewritten(source), "               function Page() {}\n");
}

#[test]
fn a_default_class_export_keeps_its_declaration() {
    let record = record("export default class Box {}\n");

    assert_eq!(
        record.exports[0].source,
        ExportSource::Local {
            local: "Box".into()
        }
    );
}

#[test]
fn a_default_async_function_export_keeps_its_declaration() {
    let record = record("export default async function load() {}\n");

    assert_eq!(
        record.exports[0].source,
        ExportSource::Local {
            local: "load".into()
        }
    );
}

#[test]
fn a_top_level_call_is_a_side_effect() {
    assert_eq!(
        record("register();\n").side_effects,
        SideEffectKind::Present
    );
}

#[test]
fn a_top_level_assignment_is_a_side_effect() {
    assert_eq!(
        record("globalThis.patched = true;\n").side_effects,
        SideEffectKind::Present
    );
}

#[test]
fn a_top_level_if_is_a_side_effect() {
    assert_eq!(
        record("if (window) { patch(); }\n").side_effects,
        SideEffectKind::Present
    );
}

#[test]
fn only_declarations_are_not_a_side_effect() {
    let source = "import { a } from \"./x.js\";\nconst b = a;\nfunction c() { d(); }\nclass E {}\nexport { b, c, E };\n";

    assert_eq!(record(source).side_effects, SideEffectKind::None);
}

#[test]
fn a_call_inside_a_function_is_not_a_top_level_side_effect() {
    assert_eq!(
        record("function f() {\n  register();\n}\n").side_effects,
        SideEffectKind::None
    );
}

#[test]
fn a_call_in_an_initializer_is_not_a_top_level_statement() {
    assert_eq!(
        record("const a = compute();\n").side_effects,
        SideEffectKind::None
    );
}

#[test]
fn a_directive_prologue_is_not_a_side_effect() {
    assert_eq!(
        record("\"use client\";\nexport const a = 1;\n").side_effects,
        SideEffectKind::None
    );
}

#[test]
fn a_module_with_crlf_line_endings_keeps_them_after_rewriting() {
    let source = "import { a } from \"./x.js\";\r\nconst b = a;\r\n";

    let out = rewritten(source);

    assert_eq!(out.matches("\r\n").count(), 2);
    assert!(out.ends_with("const b = a;\r\n"));
}

#[test]
fn patches_come_back_in_source_order() {
    let record = record("import \"./a.js\";\nexport const b = 1;\nimport \"./c.js\";\n");

    assert!(
        record
            .patches
            .windows(2)
            .all(|pair| pair[0].start <= pair[1].start)
    );
}

#[test]
fn applying_patches_never_changes_the_line_count() {
    let source = "import {\n  a,\n} from \"./x.js\";\nexport const b = a;\nexport {\n  b,\n};\n";

    assert_eq!(rewritten(source).lines().count(), source.lines().count());
}

#[test]
fn an_out_of_range_patch_is_ignored() {
    let patches = [Patch {
        start: 100,
        end: 200,
        text: PatchText::Blank,
    }];

    assert_eq!(
        crate::emit::apply_patches("const a = 1;\n", &patches),
        "const a = 1;\n"
    );
}
