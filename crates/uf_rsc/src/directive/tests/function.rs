//! Function-level `"use server"` and the owner each closure is named after.

use super::*;

#[test]
fn a_function_level_use_server_is_collected_not_rejected() {
    let scan =
        scan_directives("export async function refresh() {\n  \"use server\";\n  return 1;\n}\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert!(scan.issues.is_empty());
    assert_eq!(scan.function_directives.len(), 1);
    assert_eq!(
        scan.function_directives[0].owner,
        FunctionOwner::Named(CompactString::const_new("refresh"))
    );
}

#[test]
fn a_function_level_use_server_in_an_arrow_is_named_after_its_binding() {
    let scan = scan_directives("const save = async () => {\n  \"use server\";\n};\n");
    assert_eq!(
        scan.function_directives[0].owner,
        FunctionOwner::Named(CompactString::const_new("save"))
    );
}

#[test]
fn an_inline_closure_action_gets_a_stable_ordinal() {
    let scan = scan_directives(
        "register(async () => {\n \"use server\";\n});\nregister(async () => {\n \"use server\";\n});\n",
    );
    assert_eq!(scan.function_directives.len(), 2);
    assert_eq!(
        scan.function_directives[0].owner,
        FunctionOwner::Anonymous { ordinal: 0 }
    );
    assert_eq!(
        scan.function_directives[1].owner,
        FunctionOwner::Anonymous { ordinal: 1 }
    );
}

#[test]
fn a_flow_return_type_does_not_hide_a_function_level_action() {
    let scan =
        scan_directives("async function refresh(): Promise<string> {\n \"use server\";\n}\n");
    assert_eq!(scan.function_directives.len(), 1);
    assert_eq!(
        scan.function_directives[0].owner,
        FunctionOwner::Named(CompactString::const_new("refresh"))
    );
}

#[test]
fn a_default_exported_function_action_is_named_default() {
    let scan = scan_directives("export default async function () {\n \"use server\";\n}\n");
    assert_eq!(
        scan.function_directives[0].owner,
        FunctionOwner::Named(CompactString::const_new("default"))
    );
}

#[test]
fn a_method_level_action_is_named_after_the_method() {
    let scan = scan_directives("const api = {\n async save() {\n \"use server\";\n }\n};\n");
    assert_eq!(
        scan.function_directives[0].owner,
        FunctionOwner::Named(CompactString::const_new("save"))
    );
}

#[test]
fn a_directive_at_the_top_of_an_if_block_is_not_a_function_action() {
    let scan = scan_directives("if (ready) {\n \"use server\";\n}\n");
    assert!(scan.function_directives.is_empty());
    assert_eq!(scan.issues.len(), 1);
    assert_eq!(scan.issues[0].rule(), "rsc/directive-not-in-prologue");
}

#[test]
fn a_directive_at_the_top_of_a_for_block_is_not_a_function_action() {
    let scan = scan_directives("for (const x of xs) {\n \"use server\";\n}\n");
    assert!(scan.function_directives.is_empty());
    assert_eq!(scan.issues.len(), 1);
}

#[test]
fn a_client_directive_inside_a_function_is_rejected() {
    let scan = scan_directives("function Widget() {\n \"use client\";\n}\n");
    assert!(scan.function_directives.is_empty());
    assert_eq!(
        scan.issues.as_slice(),
        &[DirectiveIssue::ClientDirectiveInFunction { line: 2, column: 2 }]
    );
}

#[test]
fn a_function_level_action_after_a_file_directive_is_still_collected() {
    let scan = scan_directives(
        "\"use client\";\nexport function Form() {\n const save = async () => {\n  \"use server\";\n };\n}\n",
    );
    assert_eq!(scan.environment, ModuleEnvironment::Client);
    assert_eq!(scan.function_directives.len(), 1);
}

#[test]
fn a_single_line_function_action_terminated_by_a_brace_is_collected() {
    let scan = scan_directives("const save = async () => { \"use server\" };\n");
    assert_eq!(scan.function_directives.len(), 1);
}
