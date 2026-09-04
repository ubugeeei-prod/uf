//! Flow built-ins about what a module declares and imports: mixing `require`
//! into an ES module, exporting a binding that can be reassigned, renaming a
//! binding to `default` on the way out, and top-level names that collide with a
//! JSX intrinsic element.

use uf_config::UniflowedConfig;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{
    FileScan, ends_word, find_all, find_words, identifier_len, next_non_space, prev_non_space,
    starts_word,
};
use crate::{Diagnostic, push_in_code, severity};

pub(crate) fn run_flow_mixed_import_and_require(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::MixedImportAndRequire.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };
    if !scan.facts.has_esm_import {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_words(code, "require") {
            if prev_non_space(code, at).is_some_and(|(_, byte)| byte == b'.') {
                continue;
            }
            if !next_non_space(code, at + "require".len()).is_some_and(|(_, byte)| byte == b'(') {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "this module already uses `import`; do not mix in `require`",
            );
        }
    }
}

pub(crate) fn run_flow_non_const_var_export(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::NonConstVarExport.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let Some((at, _)) = next_non_space(code, 0) else {
            continue;
        };
        if identifier_len(code, at) != "export".len() || &code[at..at + 6] != "export" {
            continue;
        }
        let Some((keyword_at, _)) = next_non_space(code, at + 6) else {
            continue;
        };
        let len = identifier_len(code, keyword_at);
        if len == 0 || !matches!(&code[keyword_at..keyword_at + len], "var" | "let") {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            rule,
            severity,
            position,
            keyword_at,
            "exported bindings must be `const`; a mutable export is a live binding",
        );
    }
}

pub(crate) fn run_flow_export_renamed_default(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::ExportRenamedDefault.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_all(code, "as default")
            .filter(|&at| starts_word(code, at) && ends_word(code, at + "as default".len()))
        {
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "renaming an export to `default` hides the real name; export it directly",
            );
        }
    }
}
