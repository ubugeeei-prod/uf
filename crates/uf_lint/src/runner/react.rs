//! The `react/*` rules that read one construct at a time: preferring Flow's
//! `component` and `hook` declarations over the React idioms they replace, and
//! the default-export ban.
//!
//! The rules of React — hooks, purity, what render may change — are the
//! official React Compiler's diagnostics, filed in [`super::react_compiler`].

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
const COMPONENT_WRAPPERS: &[&str] = &["memo(", "React.memo(", "forwardRef(", "React.forwardRef("];

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

/// The name a line defines as a plain function, when it defines one in a
/// shape `hook` syntax replaces.
///
/// `function useX(…)`, with or without `export` and `async` in front, and
/// `const useX = (…) => …`, `const useX = function (…)`, and `const useX = x =>`
/// with or without `export`. Those are the shapes a hook is written in when it
/// is not written with `hook`. Only matching the bare `function useX` let the
/// exported and arrow forms, which are the common ones, go unreported.
///
/// A `const` bound to anything else is left alone, even with a hook's name:
/// `const useStore = create((set) => …)` is a hook *made* by a factory, and
/// no `hook` declaration can say that.
fn hook_defined_as_function(trimmed: &str) -> Option<&str> {
    let rest = trimmed
        .strip_prefix("export ")
        .map_or(trimmed, str::trim_start);
    let rest = rest.strip_prefix("default ").map_or(rest, str::trim_start);
    let declared = rest.strip_prefix("async ").map_or(rest, str::trim_start);
    if let Some(tail) = declared.strip_prefix("function ") {
        return tail.trim_start().split(['(', '<', ' ']).next();
    }
    let tail = rest
        .strip_prefix("const ")
        .or_else(|| rest.strip_prefix("let "))?;
    let at = binding_equals(tail)?;
    let (name, value) = (&tail[..at], &tail[at + 1..]);
    // `const useX: Type = …` names its type after a colon; the name is before.
    let name = name.split(':').next().unwrap_or(name).trim();
    let value = value.trim_start();
    let value = value.strip_prefix("async ").map_or(value, str::trim_start);
    let arrow_after_identifier = value.split_once("=>").is_some_and(|(parameter, _)| {
        let parameter = parameter.trim();
        !parameter.is_empty() && parameter.bytes().all(crate::scan::is_word_byte)
    });
    (value.starts_with("function") || value.starts_with('(') || arrow_after_identifier)
        .then_some(name)
}

/// Where the `=` that binds a declaration is: the first one that is not part
/// of `=>`, `==`, `<=`, `>=` or `!=`. A type annotation such as
/// `const useX: () => number = …` holds an `=>` before it.
fn binding_equals(tail: &str) -> Option<usize> {
    let bytes = tail.as_bytes();
    (0..bytes.len()).find(|&at| {
        bytes[at] == b'='
            && !matches!(bytes.get(at + 1), Some(b'>' | b'='))
            && !(at > 0 && matches!(bytes[at - 1], b'=' | b'<' | b'>' | b'!'))
    })
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
        let Some(name) = hook_defined_as_function(trimmed) else {
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

/// `react/no-default-export-component`: a module that declares a component,
/// or a route module, exporting it as `default`.
///
/// A convention and not a defect, hence `warn`: `@uniflowed/router` reads a
/// page's `default` first and its named `Page` second, so a default export
/// works. The rule asks for the named form because it is what `uf create`
/// scaffolds, and because a named component has one name in every import, in
/// stack traces and in React DevTools, where a default export takes whatever
/// name each importer gives it. The message used to say routes were "wired by
/// name", which was not true of uf's own router and sent readers looking for
/// a wiring bug that did not exist.
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
        // Source that this module *generates* is not source that this module
        // *is*. `npm/vite/driver.js` builds a Cloudflare Worker entry as
        // text, and its `export default { fetch: … }` was reported against the
        // line of the file holding the template rather than against a file that
        // does not exist yet (ubugeeei-prod/uf#451). Every adapter does this,
        // and so does `internal/routes.js`.
        if line.in_string(at) {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            "react/no-default-export-component",
            severity,
            position,
            at,
            "export the component by name: the router reads a named `Page`, `Layout` and the rest \
             as well as `default`, and a named component keeps one name in every import, stack \
             trace and DevTools tree",
        );
    }
}

fn is_router_module(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("$"))
}
