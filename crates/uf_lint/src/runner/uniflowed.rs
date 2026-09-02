//! The `uniflowed/*` house rules: whitespace hygiene, and shelling out to a
//! package manager from source that should be declaring a task in `uf.config.js`.

use uf_config::UniflowedConfig;
use uf_infra::memchr_iter;

use crate::scan::{FileScan, find_all, find_words, starts_word};
use crate::{Diagnostic, push, push_at, push_in_code, severity};

pub(crate) fn run_no_tabs(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "uniflowed/no-tabs") else {
        return;
    };

    for offset in memchr_iter(b'\t', scan.file.source.as_bytes()) {
        let position = scan.index.line_col(offset);
        push(
            diagnostics,
            scan.file,
            "uniflowed/no-tabs",
            severity,
            position.line,
            position.column,
            "replace tabs with spaces",
        );
    }
}

pub(crate) fn run_no_trailing_whitespace(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "uniflowed/no-trailing-whitespace") else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        // The final `split` element is the text after the last `\n`; an empty one
        // is not a real line and must not be reported.
        if position + 1 == scan.lines.len() && line.text.is_empty() {
            continue;
        }
        let trimmed = line.text.trim_end_matches([' ', '\t']);
        if trimmed.len() != line.text.len() {
            push_at(
                diagnostics,
                scan,
                "uniflowed/no-trailing-whitespace",
                severity,
                position,
                trimmed.len(),
                "remove trailing whitespace",
            );
        }
    }
}

/// Package-manager invocations that belong in `uf.config.js` tasks instead.
const PACKAGE_MANAGER_WORDS: [&str; 4] = ["yarn", "pnpm", "bunx", "npx"];

pub(crate) fn run_no_npm_script_invocation(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "uniflowed/no-npm-script-invocation") else {
        return;
    };
    // `package.json` has its own, more specific rule.
    if scan.file.path.ends_with("package.json") {
        return;
    }

    const MESSAGE: &str =
        "declare the task in uf.config.js; uf projects do not shell out to npm/yarn/pnpm/bunx";

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_all(code, "npm run").filter(|&at| starts_word(code, at)) {
            push_in_code(
                diagnostics,
                scan,
                "uniflowed/no-npm-script-invocation",
                severity,
                position,
                at,
                MESSAGE,
            );
        }
        for word in PACKAGE_MANAGER_WORDS {
            for at in find_words(code, word) {
                push_in_code(
                    diagnostics,
                    scan,
                    "uniflowed/no-npm-script-invocation",
                    severity,
                    position,
                    at,
                    MESSAGE,
                );
            }
        }
    }
}
