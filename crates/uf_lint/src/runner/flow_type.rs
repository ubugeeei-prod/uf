//! Flow built-ins that judge a type annotation: types too vague to check, types
//! Flow has renamed, types that belong to Flow's own internals, and object types
//! whose exactness the author never said out loud.

use uf_config::UniflowedConfig;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{
    FileScan, find_words, identifier_len, next_non_space, prev_non_space, starts_word,
};
use crate::{Diagnostic, Severity, push_at, push_in_code, severity};

/// Types Flow's `unclear-type` lint rejects, with the advice for each.
const UNCLEAR_TYPES: [(&str, &str); 3] = [
    (
        "any",
        "avoid `any`; use `mixed`, opaque types, or generated router/action types",
    ),
    ("Object", "avoid `Object`; describe the object's shape"),
    ("Function", "avoid `Function`; describe the call signature"),
];

pub(crate) fn run_flow_unclear_type(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::UnclearType.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for (needle, message) in UNCLEAR_TYPES {
            for at in find_words(code, needle) {
                // `Object.keys(x)`, `x.any`, and `new Function(src)` are value
                // positions, not type annotations.
                if prev_non_space(code, at).is_some_and(|(_, byte)| byte == b'.') {
                    continue;
                }
                if next_non_space(code, at + needle.len())
                    .is_some_and(|(_, byte)| byte == b'.' || byte == b'(')
                {
                    continue;
                }
                push_in_code(diagnostics, scan, rule, severity, position, at, message);
            }
        }
    }
}

pub(crate) fn run_flow_deprecated_type(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::DeprecatedType.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_words(code, "bool") {
            if prev_non_space(code, at).is_some_and(|(_, byte)| byte == b'.') {
                continue;
            }
            // `{ bool: true }` is a property name, not a type annotation.
            if next_non_space(code, at + 4).is_some_and(|(_, byte)| byte == b':' || byte == b'(') {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "the `bool` type alias is deprecated; write `boolean`",
            );
        }
    }
}

/// Flow types that exist only for the checker's own use.
///
/// Referencing them compiles today and breaks on the next Flow upgrade, which is
/// exactly what Flow's `internal-type` lint is for.
static INTERNAL_TYPES: phf::Set<&'static str> = phf::phf_set! {
    "$Flow$EnumProto",
    "$Flow$EnumValueRepresentationTypes",
    "$Flow$ModuleRef",
    "$TEMPORARY$array",
    "$TEMPORARY$bigint",
    "$TEMPORARY$number",
    "$TEMPORARY$object",
    "$TEMPORARY$string",
    "React$AbstractComponent",
    "React$Component",
    "React$ComponentType",
    "React$Context",
    "React$Element",
    "React$ElementConfig",
    "React$ElementProps",
    "React$ElementRef",
    "React$ElementType",
    "React$Key",
    "React$MixedElement",
    "React$Node",
    "React$Portal",
    "React$Ref",
    "React$StatelessFunctionalComponent",
};

pub(crate) fn run_flow_internal_type(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::InternalType.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let mut at = 0usize;
        while at < code.len() {
            let len = identifier_len(code, at);
            if len == 0 {
                at += 1;
                continue;
            }
            if starts_word(code, at) && INTERNAL_TYPES.contains(&code[at..at + len]) {
                push_in_code(
                    diagnostics,
                    scan,
                    rule,
                    severity,
                    position,
                    at,
                    "this is a Flow-internal type; use the public equivalent",
                );
            }
            at += len;
        }
    }
}

pub(crate) fn run_flow_ambiguous_object_type(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::AmbiguousObjectType.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let Some(brace_at) = type_alias_object_start(line.code()) else {
            continue;
        };
        report_ambiguous_object(scan, severity, rule, position, brace_at, diagnostics);
    }
}

/// Offset of the `{` that opens a `type X = { ... }` right-hand side.
///
/// Deliberately narrow: only type-alias right-hand sides are recognised, because
/// a bare `: {` is indistinguishable from an object literal or a ternary without
/// a real parser, and a linter that guesses is worse than one that under-reports.
fn type_alias_object_start(code: &str) -> Option<usize> {
    let mut at = next_non_space(code, 0)?.0;
    loop {
        let len = identifier_len(code, at);
        if len == 0 {
            return None;
        }
        match &code[at..at + len] {
            "export" | "declare" | "opaque" => at = next_non_space(code, at + len)?.0,
            "type" => {
                at += len;
                break;
            }
            _ => return None,
        }
    }

    let equals = at + code[at..].find('=')?;
    let (brace_at, byte) = next_non_space(code, equals + 1)?;
    (byte == b'{').then_some(brace_at)
}

/// Walk one object type from its opening `{` and report every nested object type
/// that states neither exactness (`{| |}`) nor inexactness (`...`).
fn report_ambiguous_object(
    scan: &FileScan<'_>,
    severity: Severity,
    rule: &'static str,
    start_line: usize,
    start_in_code: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    /// One open `{`: where it is, and what it has told us so far.
    struct Open {
        line: usize,
        column: usize,
        exact: bool,
        spread: bool,
    }

    let mut stack: Vec<Open> = Vec::new();
    for (position, line) in scan.lines.iter().enumerate().skip(start_line) {
        let code = line.code();
        let bytes = code.as_bytes();
        let mut at = if position == start_line {
            start_in_code
        } else {
            0
        };
        while at < bytes.len() {
            match bytes[at] {
                b'{' => {
                    let exact = bytes.get(at + 1) == Some(&b'|');
                    stack.push(Open {
                        line: position,
                        column: line.code_offset() + at,
                        exact,
                        spread: false,
                    });
                    at += if exact { 2 } else { 1 };
                }
                b'}' => {
                    let Some(open) = stack.pop() else {
                        return;
                    };
                    if !open.exact && !open.spread {
                        push_at(
                            diagnostics,
                            scan,
                            rule,
                            severity,
                            open.line,
                            open.column,
                            "object type is neither exact (`{| |}`) nor explicitly inexact (`...`)",
                        );
                    }
                    if stack.is_empty() {
                        return;
                    }
                    at += 1;
                }
                b'.' if bytes[at..].starts_with(b"...") => {
                    if let Some(open) = stack.last_mut() {
                        open.spread = true;
                    }
                    at += 3;
                }
                _ => at += 1,
            }
        }
    }
}
