//! The `uniflowed/*` house rules: whitespace hygiene, and shelling out to a
//! package manager from source that should be declaring a task in `uf.config.js`.

use uf_config::UniflowedConfig;
use uf_infra::memchr_iter;

use crate::scan::{FileScan, find_all, find_words, heads_a_command, starts_word, word_in_jsx_text};
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
        // A tab inside a string is part of the string. The formatter reprints
        // from the syntax tree and keeps it, so reporting it left `uf fmt`
        // unable to make `uf lint` clean. A tab in a comment is still
        // reported: the formatter keeps that too, and it is the author's to
        // remove.
        if let Some(line) = scan.lines.get(position.line.saturating_sub(1))
            && let Some(at) = (offset + 1).checked_sub(line.offset + line.code_offset() + 1)
            && (line.in_string(at)
                || word_in_jsx_text(scan, position.line.saturating_sub(1), at, 1))
        {
            continue;
        }
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
        // Inside a template literal the spaces at the end of a line are part
        // of the string. The formatter reprints from the syntax tree and keeps
        // them, correctly — so reporting them here made `uf fmt` unable to
        // make `uf lint` clean, and the two disagreed for ever.
        if line.in_string(line.code().len()) {
            continue;
        }
        let trimmed = line.text.trim_end_matches([' ', '\t']);
        // Spaces at the end of an element's text are content. The formatter
        // reprints them, so reporting them left nothing that could clear the
        // diagnostic. A space after `</p>` is still the line's own trailing
        // space: the closing tag does not leave text open.
        let at = trimmed.len().saturating_sub(line.code_offset());
        let trailing_in_jsx = trimmed.len() != line.text.len()
            && trimmed.len() >= line.code_offset()
            && at < line.code().len()
            && word_in_jsx_text(scan, position, at, 1);
        if trailing_in_jsx {
            continue;
        }
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
///
/// A name here is only reported where it *heads a command* — see
/// [`heads_a_command`]. Matching the characters anywhere made a filename in an
/// array of format targets a shell invocation, and a test's own title another
/// one (ubugeeei-prod/uf#478).
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
        for at in find_all(code, "npm run")
            .filter(|&at| starts_word(code, at))
            .filter(|&at| heads_a_command(code, at, "npm".len()))
            .filter(|&at| !word_in_jsx_text(scan, position, at, "npm".len()))
        {
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
            // The name has to head a command, not merely appear. See
            // `scan::heads_a_command` for what that costs and what it bought.
            for at in find_words(code, word)
                .filter(|&at| heads_a_command(code, at, word.len()))
                .filter(|&at| !word_in_jsx_text(scan, position, at, word.len()))
            {
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
