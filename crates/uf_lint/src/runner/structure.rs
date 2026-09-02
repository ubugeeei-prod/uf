//! The two rules that need to know which scope a *declaration* sits in:
//! `flow/nested-component` and `flow/nested-hook`.
//!
//! They share one walk over the file with an explicit scope stack. Where a hook
//! may be *called* is `react/hooks-rules`, and that predicate lives in
//! `uf_react_compiler` next to the rest of the answer to "could this component
//! have been compiled?" — see [`super::react_compiler`].

use uf_config::UniflowedConfig;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{FileScan, identifier_len, is_hook_name, is_word_byte};
use crate::{Diagnostic, Severity, push_in_code, severity};

/// What kind of `{ ... }` a frame on the scope stack represents.
///
/// Only [`ScopeKind::allows_hooks`] is ever asked of a frame: these rules are
/// about where a declaration sits, and a `component` nested inside anything
/// that renders is the thing being reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeKind {
    /// A Flow `component` body.
    Component,
    /// A Flow `hook` body.
    Hook,
    /// A plain function whose name follows the `useSomething` convention.
    UseFunction,
    /// Any other function, class, or arrow body.
    Function,
    /// A block, object literal, JSX container, or anything else.
    Block,
}

impl ScopeKind {
    fn allows_hooks(self) -> bool {
        matches!(self, Self::Component | Self::Hook | Self::UseFunction)
    }
}

/// Walk the file once, tracking scopes, and report the two rules that need it.
pub(crate) fn run_structure_rules(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let nested_component_rule = FlowBuiltinLint::NestedComponent.as_rule_id();
    let nested_hook_rule = FlowBuiltinLint::NestedHook.as_rule_id();
    let nested_component = severity(config, nested_component_rule);
    let nested_hook = severity(config, nested_hook_rule);
    if nested_component.is_none() && nested_hook.is_none() {
        return;
    }

    let mut stack: Vec<ScopeKind> = Vec::new();
    let mut pending: Option<ScopeKind> = None;
    let mut paren_depth: u32 = 0;

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let bytes = code.as_bytes();
        let mut previous: Option<&str> = None;
        let mut at = 0usize;

        while at < bytes.len() {
            match bytes[at] {
                b'(' => {
                    paren_depth += 1;
                    previous = None;
                    at += 1;
                }
                b')' => {
                    paren_depth = paren_depth.saturating_sub(1);
                    previous = None;
                    at += 1;
                }
                b'{' => {
                    let kind = match paren_depth {
                        0 => pending.take().unwrap_or(ScopeKind::Block),
                        _ => ScopeKind::Block,
                    };
                    stack.push(kind);
                    previous = None;
                    at += 1;
                }
                b'}' => {
                    stack.pop();
                    pending = None;
                    previous = None;
                    at += 1;
                }
                b';' => {
                    pending = None;
                    previous = None;
                    at += 1;
                }
                b'=' if bytes.get(at + 1) == Some(&b'>') => {
                    set_pending(&mut pending, ScopeKind::Function);
                    previous = None;
                    at += 2;
                }
                b'\'' | b'"' | b'`' => {
                    // Strings are skipped so a `}` inside one cannot unbalance
                    // the scope stack.
                    let quote = bytes[at];
                    at = skip_string(bytes, at + 1, quote);
                    previous = None;
                }
                byte if is_word_byte(byte) => {
                    let len = identifier_len(code, at);
                    if len == 0 {
                        at += 1;
                        continue;
                    }
                    let word = &code[at..at + len];
                    handle_structure_word(
                        StructureWord {
                            scan,
                            position,
                            at,
                            word,
                            previous,
                        },
                        &mut pending,
                        &stack,
                        (
                            nested_component.map(|severity| (nested_component_rule, severity)),
                            nested_hook.map(|severity| (nested_hook_rule, severity)),
                        ),
                        diagnostics,
                    );
                    previous = Some(word);
                    at += len;
                }
                _ => at += 1,
            }
        }
    }
}

/// Everything `handle_structure_word` needs about the token it is looking at.
struct StructureWord<'a, 'b> {
    scan: &'a FileScan<'b>,
    position: usize,
    at: usize,
    word: &'a str,
    previous: Option<&'a str>,
}

type StructureSeverities = (
    Option<(&'static str, Severity)>,
    Option<(&'static str, Severity)>,
);

fn handle_structure_word(
    token: StructureWord<'_, '_>,
    pending: &mut Option<ScopeKind>,
    stack: &[ScopeKind],
    severities: StructureSeverities,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let StructureWord {
        scan,
        position,
        at,
        word,
        previous,
    } = token;
    let (nested_component, nested_hook) = severities;

    let declaration_context =
        previous.is_none() || matches!(previous, Some("export" | "declare" | "default"));

    match word {
        "component" if declaration_context => {
            set_pending(pending, ScopeKind::Component);
            if let Some((rule, severity)) = nested_component
                && stack.iter().copied().any(ScopeKind::allows_hooks)
            {
                push_in_code(
                    diagnostics,
                    scan,
                    rule,
                    severity,
                    position,
                    at,
                    "declare this component at module scope; nesting it remounts its subtree every render",
                );
            }
            return;
        }
        "hook" if declaration_context => {
            set_pending(pending, ScopeKind::Hook);
            if let Some((rule, severity)) = nested_hook
                && stack.iter().copied().any(ScopeKind::allows_hooks)
            {
                push_in_code(
                    diagnostics,
                    scan,
                    rule,
                    severity,
                    position,
                    at,
                    "declare this hook at module scope; a nested hook gets a new identity every render",
                );
            }
            return;
        }
        "function" => {
            set_pending(pending, ScopeKind::Function);
            return;
        }
        "class" => {
            set_pending(pending, ScopeKind::Function);
            return;
        }
        _ => {}
    }

    if matches!(previous, Some("function" | "const" | "let" | "var")) && is_hook_name(word) {
        set_pending(pending, ScopeKind::UseFunction);
    }
}

/// Remember what the next `{` opens, without letting a trailing `=>` downgrade a
/// hook-eligible declaration.
fn set_pending(pending: &mut Option<ScopeKind>, kind: ScopeKind) {
    match (*pending, kind) {
        (Some(existing), ScopeKind::Function) if existing.allows_hooks() => {}
        _ => *pending = Some(kind),
    }
}

/// Index just past the closing `quote`, or the end of the slice.
fn skip_string(bytes: &[u8], from: usize, quote: u8) -> usize {
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            byte if byte == quote => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}
