//! What counts as a file-level directive, and where it is allowed to sit.

use super::*;

#[test]
fn a_module_without_a_directive_is_a_server_component() {
    assert_eq!(
        module_environment("// @flow\nexport default function Page() {}\n"),
        ModuleEnvironment::Server
    );
}

#[test]
fn an_empty_module_is_a_server_component() {
    assert_eq!(module_environment(""), ModuleEnvironment::Server);
}

#[test]
fn double_quoted_use_client_is_a_client_module() {
    assert_eq!(
        module_environment("\"use client\";\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn single_quoted_use_client_is_a_client_module() {
    assert_eq!(
        module_environment("'use client';\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn use_server_marks_a_server_actions_module() {
    assert_eq!(
        module_environment("\"use server\";\n"),
        ModuleEnvironment::ServerActions
    );
}

#[test]
fn a_directive_without_a_semicolon_is_accepted() {
    assert_eq!(
        module_environment("\"use client\"\nexport const a = 1;\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn a_directive_at_the_end_of_file_without_a_terminator_is_accepted() {
    assert_eq!(
        module_environment("'use client'"),
        ModuleEnvironment::Client
    );
}

#[test]
fn a_byte_order_mark_does_not_hide_the_directive() {
    assert_eq!(
        module_environment("\u{feff}\"use client\";\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn a_shebang_does_not_hide_the_directive() {
    assert_eq!(
        module_environment("#!/usr/bin/env uf\n\"use client\";\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn a_byte_order_mark_and_a_shebang_together_do_not_hide_the_directive() {
    assert_eq!(
        module_environment("\u{feff}#!/usr/bin/env uf\n'use server'\n"),
        ModuleEnvironment::ServerActions
    );
}

#[test]
fn a_line_comment_may_precede_the_directive() {
    assert_eq!(
        module_environment("// @flow\n\"use client\";\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn a_block_comment_may_precede_the_directive_on_the_same_line() {
    assert_eq!(
        module_environment("/* c */ \"use client\";\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn a_multiline_block_comment_may_precede_the_directive() {
    assert_eq!(
        module_environment("/*\n * @flow\n */\n'use client';\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn blank_lines_and_odd_whitespace_may_precede_the_directive() {
    assert_eq!(
        module_environment("\n\n\t   'use client'   ;\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn crlf_line_endings_do_not_hide_the_directive() {
    assert_eq!(
        module_environment("// @flow\r\n\"use client\";\r\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn use_strict_before_the_directive_is_allowed() {
    assert_eq!(
        module_environment("\"use strict\";\n\"use client\";\n"),
        ModuleEnvironment::Client
    );
}

#[test]
fn two_spaces_inside_the_directive_is_not_a_directive() {
    let scan = scan_directives("\"use  client\";\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert!(scan.issues.is_empty());
}

#[test]
fn a_unicode_escape_inside_the_directive_is_not_a_directive() {
    assert_eq!(
        module_environment("\"use\\u0020client\";\n"),
        ModuleEnvironment::Server
    );
}

#[test]
fn padding_inside_the_quotes_is_not_a_directive() {
    assert_eq!(
        module_environment("\" use client \";\n"),
        ModuleEnvironment::Server
    );
}

#[test]
fn uppercase_directives_are_not_directives() {
    assert_eq!(
        module_environment("\"USE CLIENT\";\n"),
        ModuleEnvironment::Server
    );
}

#[test]
fn a_concatenated_directive_is_rejected_with_an_issue() {
    let scan = scan_directives("\"use client\" + \"\";\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert_eq!(
        scan.issues.as_slice(),
        &[DirectiveIssue::NotAStringLiteral {
            kind: DirectiveKind::UseClient,
            line: 1,
            column: 1,
        }]
    );
}

#[test]
fn a_concatenated_directive_split_over_lines_is_rejected() {
    let scan = scan_directives("\"use client\"\n  + \"\";\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert_eq!(scan.issues.len(), 1);
    assert_eq!(scan.issues[0].rule(), "rsc/directive-not-a-string-literal");
}

#[test]
fn a_template_literal_directive_is_rejected() {
    let scan = scan_directives("`use client`;\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert_eq!(
        scan.issues.as_slice(),
        &[DirectiveIssue::NotAStringLiteral {
            kind: DirectiveKind::UseClient,
            line: 1,
            column: 1,
        }]
    );
}

#[test]
fn a_directive_after_a_statement_is_rejected() {
    let scan = scan_directives("const a = 1;\n\"use client\";\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert_eq!(
        scan.issues.as_slice(),
        &[DirectiveIssue::NotInPrologue {
            kind: DirectiveKind::UseClient,
            line: 2,
            column: 1,
        }]
    );
}

#[test]
fn a_directive_after_an_import_is_rejected() {
    let scan = scan_directives("import \"./a.js\";\n\"use client\";\n");
    assert_eq!(scan.issues.len(), 1);
    assert_eq!(scan.issues[0].line(), 2);
}

#[test]
fn a_directive_inside_a_string_value_is_not_a_directive() {
    let scan = scan_directives("const marker = \"use client\";\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert!(scan.issues.is_empty());
}

#[test]
fn a_directive_inside_a_comment_is_not_a_directive() {
    let scan = scan_directives("// \"use client\";\nconst a = 1;\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert!(scan.issues.is_empty());
}

#[test]
fn a_directive_passed_as_an_argument_is_not_a_directive() {
    let scan = scan_directives("register(\"use server\");\n");
    assert!(scan.issues.is_empty());
    assert!(scan.function_directives.is_empty());
}

#[test]
fn conflicting_directives_keep_the_first_and_report() {
    let scan = scan_directives("\"use client\";\n\"use server\";\n");
    assert_eq!(scan.environment, ModuleEnvironment::Client);
    assert_eq!(
        scan.issues.as_slice(),
        &[DirectiveIssue::Conflicting { line: 2, column: 1 }]
    );
}

#[test]
fn a_repeated_identical_directive_is_not_a_conflict() {
    let scan = scan_directives("\"use client\";\n\"use client\";\n");
    assert_eq!(scan.environment, ModuleEnvironment::Client);
    assert!(scan.issues.is_empty());
}
