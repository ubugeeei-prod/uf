//! The three rules that need to know which scope a token sits in:
//! `flow/nested-component`, `flow/nested-hook` and `react/hooks-rules`.
//!
//! They share one walk over the file with an explicit scope stack, because
//! answering "is this hook call at the top level of a component?" costs the same
//! work for all three.

use uf_config::UniflowedConfig;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{
    FileScan, identifier_len, is_hook_name, is_word_byte, next_non_space, prev_non_space,
};
use crate::{Diagnostic, Severity, push_in_code, severity};

/// What kind of `{ ... }` a frame on the scope stack represents.
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
    /// A JSX expression container, which does not nest hook scope.
    Jsx,
    /// A block, object literal, or anything else.
    Block,
}

impl ScopeKind {
    fn is_function(self) -> bool {
        matches!(
            self,
            Self::Component | Self::Hook | Self::UseFunction | Self::Function
        )
    }

    fn allows_hooks(self) -> bool {
        matches!(self, Self::Component | Self::Hook | Self::UseFunction)
    }
}

/// One open `{` during the structure walk.
struct Frame {
    kind: ScopeKind,
    /// Hook nesting depth *inside* this frame.
    hook_depth: u32,
}

const HOOK_SCOPE_MESSAGE: &str =
    "call hooks only inside a `component`, a `hook`, or a `useX` function";
const HOOK_TOP_LEVEL_MESSAGE: &str =
    "call hooks at the top level; not inside conditions, loops, or callbacks";

/// Walk the file once, tracking scopes, and report the three rules that need it.
///
/// Known limitation: a hook call inside a JSX expression container is only
/// tolerated when the container opens on the same line as the `>` that precedes
/// it, because that is as much JSX structure as a lexer-free scan can recover.
pub(crate) fn run_structure_rules(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let nested_component_rule = FlowBuiltinLint::NestedComponent.as_rule_id();
    let nested_hook_rule = FlowBuiltinLint::NestedHook.as_rule_id();
    let nested_component = severity(config, nested_component_rule);
    let nested_hook = severity(config, nested_hook_rule);
    let hooks_rules = severity(config, "react/hooks-rules");
    if nested_component.is_none() && nested_hook.is_none() && hooks_rules.is_none() {
        return;
    }

    let mut stack: Vec<Frame> = Vec::new();
    let mut pending: Option<ScopeKind> = None;
    let mut hook_depth: u32 = 0;
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
                    let kind = if paren_depth == 0 {
                        pending.take().unwrap_or_else(|| classify_brace(code, at))
                    } else {
                        classify_brace(code, at)
                    };
                    if kind != ScopeKind::Jsx {
                        hook_depth += 1;
                    }
                    stack.push(Frame { kind, hook_depth });
                    previous = None;
                    at += 1;
                }
                b'}' => {
                    if let Some(frame) = stack.pop()
                        && frame.kind != ScopeKind::Jsx
                    {
                        hook_depth = hook_depth.saturating_sub(1);
                    }
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
                            code,
                            at,
                            word,
                            previous,
                        },
                        &mut pending,
                        &stack,
                        hook_depth,
                        (
                            nested_component.map(|severity| (nested_component_rule, severity)),
                            nested_hook.map(|severity| (nested_hook_rule, severity)),
                            hooks_rules,
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
    code: &'a str,
    at: usize,
    word: &'a str,
    previous: Option<&'a str>,
}

type StructureSeverities = (
    Option<(&'static str, Severity)>,
    Option<(&'static str, Severity)>,
    Option<Severity>,
);

fn handle_structure_word(
    token: StructureWord<'_, '_>,
    pending: &mut Option<ScopeKind>,
    stack: &[Frame],
    hook_depth: u32,
    severities: StructureSeverities,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let StructureWord {
        scan,
        position,
        code,
        at,
        word,
        previous,
    } = token;
    let (nested_component, nested_hook, hooks_rules) = severities;

    let declaration_context =
        previous.is_none() || matches!(previous, Some("export" | "declare" | "default"));

    match word {
        "component" if declaration_context => {
            set_pending(pending, ScopeKind::Component);
            if let Some((rule, severity)) = nested_component
                && stack.iter().any(|frame| frame.kind.allows_hooks())
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
                && stack.iter().any(|frame| frame.kind.allows_hooks())
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

    let is_binding_name = matches!(
        previous,
        Some("function" | "const" | "let" | "var" | "component" | "hook")
    );
    if is_binding_name {
        if is_hook_name(word) && matches!(previous, Some("function" | "const" | "let" | "var")) {
            set_pending(pending, ScopeKind::UseFunction);
        }
        return;
    }

    let Some(severity) = hooks_rules else {
        return;
    };
    if !is_hook_name(word) {
        return;
    }
    if !next_non_space(code, at + word.len()).is_some_and(|(_, byte)| byte == b'(') {
        return;
    }
    // `props.useThing` is a property read, not a hook call.
    if prev_non_space(code, at).is_some_and(|(_, byte)| byte == b'.') {
        return;
    }

    let message = match stack.iter().rev().find(|frame| frame.kind.is_function()) {
        None => Some(HOOK_SCOPE_MESSAGE),
        Some(frame) if !frame.kind.allows_hooks() => Some(HOOK_SCOPE_MESSAGE),
        Some(frame) if frame.hook_depth != hook_depth => Some(HOOK_TOP_LEVEL_MESSAGE),
        Some(_) => None,
    };
    if let Some(message) = message {
        push_in_code(
            diagnostics,
            scan,
            "react/hooks-rules",
            severity,
            position,
            at,
            message,
        );
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

/// Classify a `{` that no declaration head claimed.
fn classify_brace(code: &str, at: usize) -> ScopeKind {
    match prev_non_space(code, at) {
        // `<div>{...}` is a JSX container; `=>` and `->` are not.
        Some((position, b'>')) => {
            let before = position.checked_sub(1).map(|index| code.as_bytes()[index]);
            if matches!(before, Some(b'=') | Some(b'-')) {
                ScopeKind::Block
            } else {
                ScopeKind::Jsx
            }
        }
        _ => ScopeKind::Block,
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
