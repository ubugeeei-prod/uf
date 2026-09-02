//! The `react/*` rules that read one construct at a time: preferring Flow's
//! `component` and `hook` declarations over the React idioms they replace, and
//! the default-export ban.
//!
//! The rules that ask whether a component *could have been compiled* live in
//! [`super::react_compiler`], which delegates to `uf_react_compiler`.

use uf_config::UniflowedConfig;

use crate::scan::{FileScan, is_hook_name, next_non_space};
use crate::{Diagnostic, push_at, push_in_code, severity};

pub(crate) fn run_react_component_syntax(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react/component-syntax") else {
        return;
    };

    if !(scan.file.path.ends_with(".jsx")
        || scan.file.path.ends_with(".tsx")
        || scan.facts.mentions_react)
    {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let trimmed = code.trim_start();
        let leading = line.code_offset() + (code.len() - trimmed.len());
        let name = trimmed
            .strip_prefix("function ")
            .and_then(|tail| tail.split(['(', '<']).next())
            .or_else(|| {
                trimmed
                    .strip_prefix("const ")
                    .and_then(|tail| tail.split([' ', '=']).next())
            });

        let Some(name) = name else {
            continue;
        };
        let Some(first) = name.chars().next() else {
            continue;
        };
        if first.is_ascii_uppercase() && !trimmed.starts_with("component ") {
            push_at(
                diagnostics,
                scan,
                "react/component-syntax",
                severity,
                position,
                leading,
                "prefer Flow `component` syntax for React components",
            );
        }
    }
}

pub(crate) fn run_react_hook_syntax(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react/hook-syntax") else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let trimmed = code.trim_start();
        let leading = line.code_offset() + (code.len() - trimmed.len());
        let Some(name) = trimmed
            .strip_prefix("function ")
            .and_then(|tail| tail.split(['(', '<']).next())
        else {
            continue;
        };

        if is_hook_name(name) {
            push_at(
                diagnostics,
                scan,
                "react/hook-syntax",
                severity,
                position,
                leading,
                "prefer Flow `hook` syntax for React hooks",
            );
        }
    }
}

pub(crate) fn run_react_no_default_export_component(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react/no-default-export-component") else {
        return;
    };
    if !(scan.facts.declares_component || is_router_module(&scan.file.path)) {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let Some((at, _)) = next_non_space(code, 0) else {
            continue;
        };
        if !code[at..].starts_with("export default") {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            "react/no-default-export-component",
            severity,
            position,
            at,
            "framework routes are wired by name; export components with a named export",
        );
    }
}

fn is_router_module(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("_uf."))
}
