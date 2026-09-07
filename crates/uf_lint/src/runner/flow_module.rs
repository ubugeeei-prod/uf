//! Flow built-ins about what a module declares and imports: mixing `require`
//! into an ES module, exporting a binding that can be reassigned, renaming a
//! binding to `default` on the way out, and top-level names that collide with a
//! JSX intrinsic element.

use uf_config::UniflowedConfig;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{
    FileScan, ends_word, find_all, find_words, identifier_len, next_non_space, prev_non_space,
    previous_word, starts_word,
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
    // `const require = createRequire(import.meta.url)` is how an ES module loads
    // a `.node` addon, and Node documents no other way. The binding is local and
    // its value came from `node:module`, so the module is not mixing module
    // systems — it is using the one bridge the platform provides, and the free
    // variable this rule is about is not in scope any more
    // (ubugeeei-prod/uf#479).
    if binds_require(scan) {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_words(code, "require") {
            // `node -e "… require('node:fs') …"` is a shell command this module
            // hands to a task runner, not a module system it mixes in. That is
            // `uf.config.js` in this repository, twice on one line.
            if line.in_string(at) {
                continue;
            }
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

/// Whether the file declares `require` as a binding of its own.
///
/// `const`, `let` or `var` — any of the three shadows the CommonJS free
/// variable, and a rule about mixing module systems has nothing to say about a
/// local function that happens to share its name. The declaration is looked for
/// rather than the `createRequire` call: a project that wraps it in a helper is
/// making the same statement, and the name is what decides whether the free
/// variable is reachable at all.
fn binds_require(scan: &FileScan<'_>) -> bool {
    scan.lines.iter().any(|line| {
        let code = line.code();
        find_words(code, "require").any(|at| {
            // `= …` after it, so a call is not read as a declaration, and a
            // keyword before it, so a property named `require` is not either.
            next_non_space(code, at + "require".len()).is_some_and(|(_, byte)| byte == b'=')
                && previous_word(code, at)
                    .is_some_and(|(_, word)| matches!(word, "const" | "let" | "var"))
        })
    })
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
        // A line of a template literal that happens to read `export let x = 1`
        // is a string this module builds, not a binding it exports — uf's own
        // code generators emit Flow source that way. Its sibling rules in this
        // runner guard the same search with the same test.
        if line.in_string(keyword_at) {
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
            // Prose about the rule is not the rule being broken: a message
            // saying `export it as default` renames nothing.
            .filter(|&at| !line.in_string(at))
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
