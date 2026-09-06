//! Flow built-ins that judge a type annotation: types too vague to check, types
//! Flow has renamed, types that belong to Flow's own internals, and object types
//! whose exactness the author never said out loud.

use uf_config::UniflowedConfig;

use crate::flow_builtin::FlowBuiltinLint;
use crate::scan::{
    FileScan, find_words, identifier_len, is_word_byte, next_non_space, prev_non_space,
    previous_word, starts_word,
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
                // A sentence is not an annotation: `it("treats Object as any
                // non-null object", …)` names no type.
                if line.in_string(at) {
                    continue;
                }
                if names_a_value(code, at, needle.len()) {
                    continue;
                }
                push_in_code(diagnostics, scan, rule, severity, position, at, message);
            }
        }
    }
}

/// Whether the word at `at` is an expression rather than a type annotation.
///
/// None of the three names this rule looks for is reserved. `Object` and
/// `Function` are global constructors, and all three are legal property names,
/// so the same word is a type on one line and an ordinary value on the next:
///
/// ```js
/// type Handler = Function;          // a type
/// case Function:                    // the constructor, matched against
/// expect.any(Function)              // the constructor, passed
/// obj.constructor === Object        // the constructor, compared
/// { any: asymmetric.any }           // a property called `any`
/// ```
///
/// Flow's own lint walks an AST and never has to ask. This one reads source
/// text — which is what `RuleRequirement::SourceText` records — so it asks the
/// only question source text answers: what stands either side of the word.
/// Each arm below is a shape a *type* cannot have, and every one of them was a
/// finding this rule reported against code that was already right:
/// `@uniflowed/test`'s `expect.any` is Jest's, Vitest's and Sinon's name for
/// the matcher, and the module that implements it has to both name the
/// property and switch on the constructor.
///
/// # What it still cannot see
///
/// A `(` that follows a name opens a call's argument list, a declaration's
/// parameter list or an `if`'s condition, and all three hold expressions. The
/// one place that is not true is `declare function f(Object): void`, where a
/// bare name in a libdef's parameter list *is* the parameter's type — so an
/// `Object` written that way is no longer reported. uf emits no libdefs and
/// `flow/syntax` is the only rule that reads `.flow` sidecars at all, so the
/// shape does not occur here; it is the price of telling `expect.any(Object)`
/// from `(Object) => void` without a parser, and it is stated rather than
/// discovered.
fn names_a_value(code: &str, at: usize, len: usize) -> bool {
    let before = prev_non_space(code, at);
    let after = next_non_space(code, at + len);

    // `x.any` reads a property; `Object.keys(x)` and `new Function(src)` reach
    // for the global. A type is never on either side of a `.`, and never called.
    if before.is_some_and(|(_, byte)| byte == b'.') {
        return true;
    }
    if after.is_some_and(|(_, byte)| byte == b'.' || byte == b'(') {
        return true;
    }

    // `case Object:` — Flow has no syntax that puts a type after `case`.
    if previous_word(code, at).is_some_and(|(_, word)| word == "case") {
        return true;
    }

    // `obj.constructor === Object` — the operand of an equality test. The lone
    // `=` is deliberately not one of these: `type Handler = Function` is the
    // shape this rule exists for.
    if follows_an_equality_operator(code, at) {
        return true;
    }

    // `{ any: … }` — a property key, which is the one place a *type* named in
    // an object type is not what the colon introduces. The opener has to be
    // named rather than assumed from the colon alone, because a conditional
    // type's `? any : never` puts a real annotation in front of one too, and it
    // arrives with a `?` in front instead.
    if after.is_some_and(|(_, byte)| byte == b':')
        && before.is_none_or(|(_, byte)| matches!(byte, b'{' | b',' | b';'))
    {
        return true;
    }

    // `expect.any(Function)` — the whole of an argument, in a list that is
    // being called rather than one that describes a function type.
    is_a_bare_argument(code, at, len)
}

/// Whether the word at `at` is the right operand of `==`, `===`, `!=` or `!==`.
fn follows_an_equality_operator(code: &str, at: usize) -> bool {
    let Some((end, b'=')) = prev_non_space(code, at) else {
        return false;
    };
    end > 0 && matches!(code.as_bytes()[end - 1], b'=' | b'!')
}

/// Whether the word at `at` is an entire argument of a call.
///
/// `expect.any(Function)` passes the constructor; `type Sink = (Object) => void`
/// names a parameter's type, and the two are told apart by what stands before
/// the `(`. A `(` that follows a name, a `)` or a `]` belongs to something being
/// called or declared; a `(` that follows anything else opens a group or a
/// function type, which is where a bare type may stand.
///
/// The word has to be the *whole* argument, which is what keeps a type inside
/// one out: `Map<string, any>` is reached with a `<` or a `,` in front and a `>`
/// behind, and `(node: any)` with a `:` in front.
fn is_a_bare_argument(code: &str, at: usize, len: usize) -> bool {
    if !prev_non_space(code, at).is_some_and(|(_, byte)| matches!(byte, b'(' | b',')) {
        return false;
    }
    if !next_non_space(code, at + len).is_some_and(|(_, byte)| matches!(byte, b')' | b',')) {
        return false;
    }
    let Some(open) = enclosing_open_paren(code, at) else {
        return false;
    };
    prev_non_space(code, open)
        .is_some_and(|(_, byte)| is_word_byte(byte) || byte == b')' || byte == b']')
}

/// The `(` that opens the list the byte at `at` stands in, within this line.
///
/// `None` when the nearest unclosed opener is a `[` or a `{` — an array or an
/// object literal is not an argument list — and when the line holds no opener
/// at all, which is what a list continued from the line above looks like to a
/// scanner that reads one line at a time.
fn enclosing_open_paren(code: &str, at: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0usize;
    let mut index = at;
    while index > 0 {
        index -= 1;
        match bytes[index] {
            b')' | b']' | b'}' => depth += 1,
            b'[' | b'{' => depth = depth.checked_sub(1)?,
            b'(' => {
                if depth == 0 {
                    return Some(index);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
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
            if line.in_string(at) {
                continue;
            }
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
