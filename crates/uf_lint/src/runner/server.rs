//! The `server/*` rules, which police the server/client boundary: secrets read
//! from a client module, server-only imports crossing into one, and the
//! placement of the `'use client'` / `'use server'` directives that draw the
//! boundary in the first place.

use uf_config::UniflowedConfig;

use crate::scan::{FileScan, next_non_space};
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
        let Some(at) = code.find("SECRET").or_else(|| code.find("PRIVATE_")) else {
            continue;
        };
        push_in_code(
            diagnostics,
            scan,
            "server/no-client-secret",
            severity,
            position,
            at,
            "client modules must not read private server secrets",
        );
    }
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
        let Some(at) = SERVER_ONLY_SPECIFIERS
            .into_iter()
            .find_map(|specifier| code.find(specifier))
        else {
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
        if position == first_code_line {
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

    if !(scan.file.path.starts_with("server/") || scan.file.path.ends_with(".server.js"))
        || !scan.file.source.contains("serverAction")
    {
        return;
    }

    let first_code_line = scan
        .facts
        .first_code_line
        .map(|position| scan.lines[position].code().trim())
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
