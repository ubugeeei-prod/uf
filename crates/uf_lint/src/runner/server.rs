//! The `server/*` rules, which police the server/client boundary: secrets read
//! from a client module, server-only imports crossing into one, and the
//! placement of the `'use client'` / `'use server'` directives that draw the
//! boundary in the first place.

use uf_config::UniflowedConfig;

use crate::scan::{
    FileScan, find_all, find_words, identifier_len, next_non_space, prev_non_space, previous_word,
    starts_word, word_in_jsx_text,
};
use crate::{Diagnostic, push, push_in_code, severity};

pub(crate) fn run_server_no_client_secret(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "server/no-client-secret") else {
        return;
    };
    if !scan.facts.has_use_client {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for marker in ["import.meta.env", "process.env"] {
            for at in find_all(code, marker) {
                // `notprocess.env` is not the environment. The marker starts
                // on a word boundary, and a string that mentions it is text.
                if !starts_word(code, at)
                    || line.in_string(at)
                    || word_in_jsx_text(scan, position, at, marker.len())
                {
                    continue;
                }
                let Some((name_at, name)) = env_property(code, at + marker.len()) else {
                    continue;
                };
                if !names_a_secret(name) {
                    continue;
                }
                push_in_code(
                    diagnostics,
                    scan,
                    "server/no-client-secret",
                    severity,
                    position,
                    name_at,
                    "client modules must not read private server secrets",
                );
            }
        }
    }
}

/// The property read from `process.env` or `import.meta.env`.
///
/// `.NAME`, `?.NAME`, and `["NAME"]` / `['NAME']`. The offset is the name, so
/// an existing finding on `PRIVATE_TOKEN` stays on that column.
fn env_property(code: &str, after: usize) -> Option<(usize, &str)> {
    let (at, byte) = next_non_space(code, after)?;
    if byte == b'.' || (byte == b'?' && code.as_bytes().get(at + 1) == Some(&b'.')) {
        let dot = if byte == b'?' { at + 2 } else { at + 1 };
        let (name_at, _) = next_non_space(code, dot)?;
        let len = identifier_len(code, name_at);
        if len == 0 {
            return None;
        }
        return Some((name_at, &code[name_at..name_at + len]));
    }
    if byte == b'[' {
        let (quote_at, quote) = next_non_space(code, at + 1)?;
        if !matches!(quote, b'\'' | b'"') {
            return None;
        }
        let start = quote_at + 1;
        let end = code[start..].find(quote as char)?;
        return Some((start, &code[start..start + end]));
    }
    None
}

/// Whether the specifier at `at` is the module string of an import or require.
///
/// `from "…"`, `import "…"`, `require("…")`, and `import("…")`. The words
/// inside an ordinary string are not in front of its opening quote.
fn specifier_is_imported(code: &str, at: usize) -> bool {
    let bytes = code.as_bytes();
    let mut index = at;
    while index > 0 && !matches!(bytes[index - 1], b'\'' | b'"' | b'`') {
        index -= 1;
    }
    if index == 0 {
        return false;
    }
    let quote_at = index - 1;
    let before = match prev_non_space(code, quote_at) {
        Some((paren, b'(')) => paren,
        _ => quote_at,
    };
    matches!(
        previous_word(code, before).map(|(_, word)| word),
        Some("from" | "import" | "require")
    )
}

/// `API_SECRET` and `MY_SECRET_KEY` have a `SECRET` segment. `PRIVATE_TOKEN`
/// contains `PRIVATE_`. `SECRETARY` is a different word.
fn names_a_secret(name: &str) -> bool {
    name.split('_').any(|part| part == "SECRET") || name.contains("PRIVATE_")
}

/// Module specifiers a `"use client"` module must never import.
///
/// `.server.flow` is not among them: the product has no `.flow` files, so a
/// server module is `@uniflowed/server` or a `*.server.js` sibling.
const SERVER_ONLY_SPECIFIERS: [&str; 2] = ["@uniflowed/server", ".server.js"];

pub(crate) fn run_server_no_server_only_import_in_client(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "server/no-server-only-import-in-client") else {
        return;
    };
    if !scan.facts.has_use_client {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        if !(code.contains("import") || code.contains("require")) {
            continue;
        }
        // The specifier of a real import is a string too. What makes it an
        // import is `from`, `import`, or `require` in front of that string.
        // A sentence that contains both words is text.
        let Some(at) = SERVER_ONLY_SPECIFIERS.into_iter().find_map(|specifier| {
            find_all(code, specifier).find(|&at| {
                // `import { db } from "…"` puts a `{` before the specifier.
                // That brace closes JSX text for the specifier itself, so the
                // question is whether the `import` or `require` in front of it
                // is already the element's text.
                let drawn = find_words(code, "import")
                    .chain(find_words(code, "require"))
                    .any(|word| {
                        word < at
                            && word_in_jsx_text(scan, position, word, identifier_len(code, word))
                    });
                specifier_is_imported(code, at)
                    && !drawn
                    && !word_in_jsx_text(scan, position, at, specifier.len())
            })
        }) else {
            continue;
        };
        push_in_code(
            diagnostics,
            scan,
            "server/no-server-only-import-in-client",
            severity,
            position,
            at,
            "client modules must not import server-only modules; move the call behind a server action",
        );
    }
}

/// The directives that must lead a module.
const BOUNDARY_DIRECTIVES: [&str; 4] = [
    "\"use client\"",
    "'use client'",
    "\"use server\"",
    "'use server'",
];

pub(crate) fn run_server_use_client_directive_position(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "server/use-client-directive-position") else {
        return;
    };
    let Some(first_code_line) = scan.facts.first_code_line else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        // A `"use server"` inside a function body is an inline server action, a
        // different (valid) construct; only module-level directives are checked.
        if line.depth_at_start != 0 {
            continue;
        }
        let code = line.code();
        let Some((at, _)) = next_non_space(code, 0) else {
            continue;
        };
        if !BOUNDARY_DIRECTIVES
            .into_iter()
            .any(|directive| code[at..].starts_with(directive))
        {
            continue;
        }
        // A template line that reads `"use client";` is source this module
        // quotes, not a directive. The opening quote of a real directive is
        // not inside a string, and the same line drawn inside `<pre>` is not
        // a statement either.
        if line.in_string(at) || word_in_jsx_text(scan, position, at, 1) {
            continue;
        }
        if position == first_code_line.get() {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            "server/use-client-directive-position",
            severity,
            position,
            at,
            "a boundary directive is only honoured as the module's first statement",
        );
    }
}

pub(crate) fn run_server_use_server_actions(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "server/use-server-actions") else {
        return;
    };

    if !(scan.file.path.starts_with("server/") || scan.file.path.ends_with(".server.js")) {
        return;
    }
    // The letters inside a longer name, a comment, or a string are not the
    // `serverAction` helper. A module that only mentions it has nothing for
    // the bundler to turn into a reference.
    let defines_an_action = scan.lines.iter().enumerate().any(|(position, line)| {
        let code = line.code();
        find_words(code, "serverAction").any(|at| {
            !line.in_string(at) && !word_in_jsx_text(scan, position, at, "serverAction".len())
        })
    });
    if !defines_an_action {
        return;
    }

    let first_code_line = scan
        .facts
        .first_code_line
        .map(|position| scan.lines[position.get()].code().trim())
        .unwrap_or("");

    if first_code_line != r#""use server";"# && first_code_line != r#"'use server';"# {
        push(
            diagnostics,
            scan.file,
            "server/use-server-actions",
            severity,
            1,
            1,
            r#"server action modules must start with "use server";"#,
        );
    }
}
