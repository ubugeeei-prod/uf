//! Awkward sources, and the invariants the pass holds on every one of them.

use super::*;

#[test]
fn a_directive_in_a_regular_expression_is_not_a_directive() {
    let scan = scan_directives("const re = /\"use client\";/;\n");
    assert_eq!(scan.environment, ModuleEnvironment::Server);
    assert!(scan.issues.is_empty());
}

#[test]
fn a_non_ascii_module_keeps_correct_positions() {
    let scan = scan_directives("// 日本語のコメント\nconst a = 1;\n\"use client\";\n");
    assert_eq!(scan.issues.len(), 1);
    assert_eq!(scan.issues[0].line(), 3);
}

#[test]
fn an_unterminated_string_is_not_a_directive() {
    assert_eq!(
        module_environment("\"use client\nconst a = 1;\n"),
        ModuleEnvironment::Server
    );
}

#[test]
fn directive_kind_round_trips_through_its_text() {
    for kind in [DirectiveKind::UseClient, DirectiveKind::UseServer] {
        assert_eq!(DirectiveKind::from_content(kind.as_str()), Some(kind));
        assert_eq!(kind.to_string(), kind.as_str());
    }
}

#[test]
fn environments_report_where_they_run() {
    assert!(ModuleEnvironment::Server.runs_on_server());
    assert!(ModuleEnvironment::ServerActions.runs_on_server());
    assert!(!ModuleEnvironment::Client.runs_on_server());
    assert!(ModuleEnvironment::Client.runs_on_client());
}

#[test]
fn scanning_is_idempotent() {
    let source = "\"use client\";\nexport const a = 1;\n";
    assert_eq!(scan_directives(source), scan_directives(source));
}

#[test]
fn a_very_large_module_still_finds_its_directive() {
    let mut source = String::from("\"use client\";\n");
    for index in 0..20_000 {
        source.push_str(&format!("const value{index} = {index};\n"));
    }
    assert_eq!(module_environment(&source), ModuleEnvironment::Client);
}
