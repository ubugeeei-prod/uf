//! Flow built-ins that judge an expression rather than a type: property
//! accessors, `Object.assign`, and an optional chain on a base that cannot be
//! null.

use uf_config::UniflowedConfig;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{FileScan, find_all, identifier_len, next_non_space, starts_word};
use crate::{Diagnostic, push_in_code, severity};

pub(crate) fn run_flow_unsafe_getters_setters(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::UnsafeGettersSetters.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let Some((mut at, _)) = next_non_space(code, 0) else {
            continue;
        };
        if code[at..].starts_with("static ") {
            let Some((next, _)) = next_non_space(code, at + "static ".len()) else {
                continue;
            };
            at = next;
        }
        let len = identifier_len(code, at);
        if len == 0 || !matches!(&code[at..at + len], "get" | "set") {
            continue;
        }
        let Some((name_at, _)) = next_non_space(code, at + len) else {
            continue;
        };
        let name_len = identifier_len(code, name_at);
        if name_len == 0 {
            continue;
        }
        if !next_non_space(code, name_at + name_len).is_some_and(|(_, byte)| byte == b'(') {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            rule,
            severity,
            position,
            at,
            "avoid getters and setters; they hide side effects behind property access",
        );
    }
}

pub(crate) fn run_flow_unsafe_object_assign(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::UnsafeObjectAssign.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_all(code, "Object.assign").filter(|&at| starts_word(code, at)) {
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "prefer object spread over `Object.assign`, which mutates its target",
            );
        }
    }
}

pub(crate) fn run_flow_unnecessary_optional_chain(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::UnnecessaryOptionalChain.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    // Only the syntactic subset is decidable without types: a base that is
    // literally `this` can never be nullish.
    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_all(code, "this?.").filter(|&at| starts_word(code, at)) {
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "`this` is never nullish; drop the `?.`",
            );
        }
    }
}
