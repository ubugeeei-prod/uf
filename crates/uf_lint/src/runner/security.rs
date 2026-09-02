//! The `security/*` rules: raw HTML injected into the DOM without a sanitizer in
//! the module, and the `eval` family, including the string form of `setTimeout`
//! that is `eval` wearing a hat.

use uf_config::UniflowedConfig;

use crate::scan::{FileScan, find_words, identifier_len, next_non_space, previous_word};
use crate::{Diagnostic, push_in_code, severity};

/// The sanitizing package whose helpers may feed `dangerouslySetInnerHTML`.
const MARKDOWN_PACKAGE: &str = "@uniflowed/markdown";

/// Defends against the stored/reflected XSS class that React's
/// `dangerouslySetInnerHTML` has produced repeatedly across the ecosystem
/// (CVE-2018-6341 and the long tail of markdown-renderer XSS advisories):
/// unsanitized HTML reaching the DOM. Only values produced by a
/// `@uniflowed/markdown` helper — which sanitizes — are allowed through.
pub(crate) fn run_security_no_dangerously_set_inner_html(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "security/no-dangerously-set-inner-html") else {
        return;
    };
    if !scan.file.source.contains("dangerouslySetInnerHTML") {
        return;
    }

    let sanitizers = markdown_sanitizers(scan);

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_words(code, "dangerouslySetInnerHTML") {
            // The `__html` value may wrap onto the next line, so both are checked.
            let next = scan
                .lines
                .get(position + 1)
                .map(|line| line.code())
                .unwrap_or("");
            if sanitizers
                .iter()
                .any(|&name| is_called_in(code, name) || is_called_in(next, name))
            {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                "security/no-dangerously-set-inner-html",
                severity,
                position,
                at,
                "unsanitized HTML is an XSS sink; render it through a @uniflowed/markdown helper",
            );
        }
    }
}

/// Names imported from `@uniflowed/markdown` in this file.
fn markdown_sanitizers<'a>(scan: &FileScan<'a>) -> Vec<&'a str> {
    let mut names = Vec::new();
    for line in &scan.lines {
        let code = line.code();
        if !code.contains(MARKDOWN_PACKAGE) || !code.contains("import") {
            continue;
        }
        let clause = code
            .split_once(" from ")
            .map(|(clause, _)| clause)
            .unwrap_or(code);
        let mut at = 0usize;
        while at < clause.len() {
            let len = identifier_len(clause, at);
            if len == 0 {
                at += 1;
                continue;
            }
            let word = &clause[at..at + len];
            if !matches!(word, "import" | "type" | "as" | "from") {
                names.push(word);
            }
            at += len;
        }
    }
    names
}

/// Whether `name` is used as a call or a namespace member in `code`.
fn is_called_in(code: &str, name: &str) -> bool {
    find_words(code, name).any(|at| {
        next_non_space(code, at + name.len()).is_some_and(|(_, byte)| byte == b'(' || byte == b'.')
    })
}

/// Timer APIs that accept a string body and `eval` it.
const TIMER_FUNCTIONS: [&str; 2] = ["setTimeout", "setInterval"];

/// Defends against the arbitrary-code-execution class that comes from turning
/// attacker-influenced strings into code (`eval`, `new Function`, and the
/// string form of `setTimeout`/`setInterval`).
pub(crate) fn run_security_no_eval(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "security/no-eval") else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();

        for at in find_words(code, "eval") {
            if !next_non_space(code, at + "eval".len()).is_some_and(|(_, byte)| byte == b'(') {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                "security/no-eval",
                severity,
                position,
                at,
                "`eval` executes arbitrary code; parse the data instead",
            );
        }

        for at in find_words(code, "Function") {
            if !next_non_space(code, at + "Function".len()).is_some_and(|(_, byte)| byte == b'(') {
                continue;
            }
            let Some((keyword_at, "new")) = previous_word(code, at) else {
                continue;
            };
            push_in_code(
                diagnostics,
                scan,
                "security/no-eval",
                severity,
                position,
                keyword_at,
                "`new Function` compiles a string into code; write the function directly",
            );
        }

        for timer in TIMER_FUNCTIONS {
            for at in find_words(code, timer) {
                let Some((paren_at, b'(')) = next_non_space(code, at + timer.len()) else {
                    continue;
                };
                if !next_non_space(code, paren_at + 1)
                    .is_some_and(|(_, byte)| matches!(byte, b'\'' | b'"' | b'`'))
                {
                    continue;
                }
                push_in_code(
                    diagnostics,
                    scan,
                    "security/no-eval",
                    severity,
                    position,
                    at,
                    "a string timer body is evaluated as code; pass a function instead",
                );
            }
        }
    }
}
