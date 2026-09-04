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

        if declared_component(strip_export(trimmed)).is_none() {
            continue;
        }
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

/// The declaration itself, with any `export` in front of it removed.
///
/// `export const Button = () => …` is the same declaration as `const Button =
/// () => …`, and the rule used to see only the second — every exported
/// component in a codebase went unreported.
fn strip_export(trimmed: &str) -> &str {
    let after = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    after.strip_prefix("default ").unwrap_or(after)
}

/// The name of the React component this line declares, if it declares one.
///
/// The question this answers is narrower than it looks. A name beginning with
/// a capital is *not* enough on its own: `UNITS`, `ROOT_ID` and
/// `ThemeContext` all begin with one, and none of them is a component. So the
/// name has to be PascalCase rather than a constant, and — for a `const` —
/// the thing being bound has to be a function rather than a value.
fn declared_component(declaration: &str) -> Option<&str> {
    if declaration.starts_with("component ") {
        // Already the syntax this rule asks for.
        return None;
    }

    if let Some(tail) = declaration.strip_prefix("function ") {
        let name = tail.split(['(', '<']).next()?;
        return is_component_name(name).then_some(name);
    }

    let tail = declaration.strip_prefix("const ")?;
    let name = tail.split([' ', '=', ':']).next()?;
    if !is_component_name(name) {
        return None;
    }
    let (_, value) = tail.split_once('=')?;
    binds_a_function(value).then_some(name)
}

/// Whether `name` is PascalCase, as a component's name is.
///
/// The lowercase letter is what separates a component from a constant.
/// `SERVER_COOKIES` and `MAX_DEPTH` begin with a capital and are neither
/// components nor ever mistaken for one by a reader; requiring a lowercase
/// letter somewhere is the whole of the distinction, and it is the same one
/// every style guide in React draws.
fn is_component_name(name: &str) -> bool {
    name.starts_with(|first: char| first.is_ascii_uppercase())
        && name.chars().any(|character| character.is_ascii_lowercase())
}

/// The ways of wrapping a component that leave it a component.
const COMPONENT_WRAPPERS: &[&str] = &[
    "memo(",
    "React.memo(",
    "forwardRef(",
    "React.forwardRef(",
];

/// Whether the right-hand side of a `const` binds a function.
///
/// `(` counts because an arrow's parameter list can be long enough to wrap
/// before the `=>` — `const Card = ({\n  title,\n}) => …` — and the rule reads
/// one line at a time.
fn binds_a_function(value: &str) -> bool {
    let value = value.trim_start();
    value.starts_with("function")
        || value.starts_with('(')
        || value.contains("=>")
        || COMPONENT_WRAPPERS
            .iter()
            .any(|wrapper| value.starts_with(wrapper))
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
