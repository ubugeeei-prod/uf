#![deny(missing_docs)]
//! Native lint runner for Flow source files.
//!
//! `uf lint` is the **union** of two rule sets:
//!
//! 1. Flow's own built-in lints, under the `flow/` namespace — see
//!    [`flow_builtin`] for where that list is sourced from.
//! 2. uf's framework rules for the router, server/client boundary, React
//!    component and hook syntax, package layout, and a small security set.
//!
//! [`rules`] enumerates every rule with its category, default level, one-line
//! description, and whether it can run without a type checker. Rules that need
//! type inference are declared with [`RuleRequirement::TypeChecker`]; because uf
//! has no checker yet, enabling one of those puts it in
//! [`LintReport::unavailable`] instead of silently passing.

mod flow_builtin;
mod rules;
mod scan;
mod suppression;

use rayon::prelude::*;
use thiserror::Error;
use uf_config::{RuleLevel, UniflowedConfig};
use uf_flow::FlowParser;
use uf_infra::memchr_iter;

pub use crate::flow_builtin::{FLOW_NAMESPACE, FlowBuiltinLint, FlowLintParseError};
pub use crate::rules::{
    RuleCategory, RuleDescriptor, RuleRequirement, canonical_rule_id, rule, rules,
};

use crate::rules::deprecated_aliases_for;
use crate::scan::{
    FileScan, ends_word, find_all, find_words, identifier_len, is_hook_name, is_word_byte,
    next_non_space, prev_non_space, previous_word, starts_word,
};
use crate::suppression::UNKNOWN_SUPPRESSION_RULE;

/// How loudly a diagnostic is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Reported, but does not fail the run.
    Warn,
    /// Reported and fails the run.
    Error,
}

/// One rule violation at one place in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Canonical rule id, e.g. `"flow/unclear-type"`.
    pub rule: &'static str,
    /// Configured severity for the rule.
    pub severity: Severity,
    /// Path of the offending file, when the source came from disk.
    pub path: Option<String>,
    /// 1-based line number.
    pub line: usize,
    /// 1-based **byte** column within the line, matching [`uf_infra::LineIndex`].
    pub column: usize,
    /// Human-readable explanation.
    pub message: String,
}

/// A file handed to the linter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Path used for path-sensitive rules and for reporting.
    pub path: String,
    /// Full source text.
    pub source: String,
}

/// A rule that is enabled but cannot run yet.
///
/// This exists so that enabling, say, `flow/sketchy-null` is never a silent
/// no-op: the report says out loud that the rule was skipped and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnavailableRule {
    /// Canonical rule id.
    pub rule: &'static str,
    /// Level the config asked for.
    pub level: RuleLevel,
    /// What the rule is waiting on.
    pub requirement: RuleRequirement,
}

impl UnavailableRule {
    /// Why the rule did not run.
    pub fn reason(&self) -> &'static str {
        match self.requirement {
            RuleRequirement::TypeChecker => {
                "requires Flow type inference, which uf does not implement yet; the rule did not run"
            }
            RuleRequirement::SourceText => "available",
        }
    }
}

/// The outcome of a lint run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintReport {
    /// Violations found, sorted by path, line, column, then rule id.
    pub diagnostics: Vec<Diagnostic>,
    /// How many files were scanned.
    pub files_checked: usize,
    /// Enabled rules that could not run; see [`UnavailableRule`].
    pub unavailable: Vec<UnavailableRule>,
}

impl LintReport {
    /// Whether any diagnostic is an error.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

/// Anything that can stop a lint run before it produces a report.
#[derive(Debug, Error)]
pub enum LintError {
    /// The Flow parser failed to run.
    #[error(transparent)]
    Flow(#[from] uf_flow::FlowError),
}

/// Lint many files in parallel.
pub fn lint_sources(
    files: &[SourceFile],
    config: &UniflowedConfig,
) -> Result<LintReport, LintError> {
    // Give every worker its parser now, from this shallow frame. Rayon runs
    // later jobs several frames deeper on the same worker, and a parser created
    // down there starts with much of its stack budget already spent — see
    // `uf_flow::prepare_thread`.
    let prepared = rayon::broadcast(|_| uf_flow::prepare_thread());
    for outcome in prepared {
        outcome?;
    }

    let per_file = files
        .par_iter()
        .map(|file| lint_file(file, config))
        .collect::<Result<Vec<_>, _>>()?;

    let mut diagnostics = per_file.into_iter().flatten().collect::<Vec<_>>();
    sort_diagnostics(&mut diagnostics);

    Ok(LintReport {
        diagnostics,
        files_checked: files.len(),
        unavailable: unavailable_rules(config),
    })
}

/// Lint a single file.
pub fn lint_source(file: &SourceFile, config: &UniflowedConfig) -> Result<LintReport, LintError> {
    let mut diagnostics = lint_file(file, config)?;
    sort_diagnostics(&mut diagnostics);

    Ok(LintReport {
        diagnostics,
        files_checked: 1,
        unavailable: unavailable_rules(config),
    })
}

/// Every enabled rule that needs machinery uf has not built yet.
///
/// This is the single code path through which a rule is allowed to not run.
pub fn unavailable_rules(config: &UniflowedConfig) -> Vec<UnavailableRule> {
    rules()
        .iter()
        .filter(|descriptor| !descriptor.requirement.is_available())
        .filter_map(|descriptor| {
            let level = configured_level(config, descriptor.id)?;
            level.is_enabled().then_some(UnavailableRule {
                rule: descriptor.id,
                level,
                requirement: descriptor.requirement,
            })
        })
        .collect()
}

fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
            .then(a.rule.cmp(b.rule))
    });
}

fn lint_file(file: &SourceFile, config: &UniflowedConfig) -> Result<Vec<Diagnostic>, LintError> {
    let scan = FileScan::new(file);
    let mut diagnostics = Vec::new();

    let (suppressions, bad_suppressions) = suppression::collect(&scan);
    if let Some(severity) = severity(config, UNKNOWN_SUPPRESSION_RULE) {
        for bad in bad_suppressions {
            push(
                &mut diagnostics,
                file,
                UNKNOWN_SUPPRESSION_RULE,
                severity,
                bad.line,
                bad.column,
                bad.message,
            );
        }
    }

    run_flow_syntax(&scan, config, &mut diagnostics)?;
    run_no_tabs(&scan, config, &mut diagnostics);
    run_no_trailing_whitespace(&scan, config, &mut diagnostics);
    run_no_npm_script_invocation(&scan, config, &mut diagnostics);

    run_flow_unclear_type(&scan, config, &mut diagnostics);
    run_flow_deprecated_type(&scan, config, &mut diagnostics);
    run_flow_internal_type(&scan, config, &mut diagnostics);
    run_flow_ambiguous_object_type(&scan, config, &mut diagnostics);
    run_flow_unsafe_getters_setters(&scan, config, &mut diagnostics);
    run_flow_unsafe_object_assign(&scan, config, &mut diagnostics);
    run_flow_unnecessary_optional_chain(&scan, config, &mut diagnostics);
    run_flow_mixed_import_and_require(&scan, config, &mut diagnostics);
    run_flow_non_const_var_export(&scan, config, &mut diagnostics);
    run_flow_export_renamed_default(&scan, config, &mut diagnostics);
    run_flow_react_intrinsic_overlap(&scan, config, &mut diagnostics);

    run_react_component_syntax(&scan, config, &mut diagnostics);
    run_react_hook_syntax(&scan, config, &mut diagnostics);
    run_react_no_default_export_component(&scan, config, &mut diagnostics);
    run_react_no_render_side_effects(&scan, config, &mut diagnostics);
    run_react_native_platform_split(&scan, config, &mut diagnostics);
    run_structure_rules(&scan, config, &mut diagnostics);

    run_server_no_client_secret(&scan, config, &mut diagnostics);
    run_server_no_server_only_import_in_client(&scan, config, &mut diagnostics);
    run_server_use_client_directive_position(&scan, config, &mut diagnostics);
    run_server_use_server_actions(&scan, config, &mut diagnostics);

    run_router_reserved_files(&scan, config, &mut diagnostics);
    run_package_no_npm_scripts(&scan, config, &mut diagnostics);
    run_fetch_no_global_override(&scan, config, &mut diagnostics);

    run_security_no_dangerously_set_inner_html(&scan, config, &mut diagnostics);
    run_security_no_eval(&scan, config, &mut diagnostics);

    if !suppressions.is_empty() {
        diagnostics
            .retain(|diagnostic| !suppressions.is_suppressed(diagnostic.rule, diagnostic.line));
    }

    Ok(diagnostics)
}

/// Level configured for `rule`, honouring deprecated aliases.
fn configured_level(config: &UniflowedConfig, rule: &str) -> Option<RuleLevel> {
    config.lint.rules.get(rule).copied().or_else(|| {
        // Only reached when the canonical id is absent, so the default config
        // never pays for this lookup.
        deprecated_aliases_for(rule).find_map(|alias| config.lint.rules.get(alias).copied())
    })
}

fn severity(config: &UniflowedConfig, rule: &'static str) -> Option<Severity> {
    match configured_level(config, rule).unwrap_or(RuleLevel::Off) {
        RuleLevel::Off => None,
        RuleLevel::Warn => Some(Severity::Warn),
        RuleLevel::Error => Some(Severity::Error),
    }
}

fn push(
    diagnostics: &mut Vec<Diagnostic>,
    file: &SourceFile,
    rule: &'static str,
    severity: Severity,
    line: usize,
    column: usize,
    message: impl Into<String>,
) {
    diagnostics.push(Diagnostic {
        rule,
        severity,
        path: Some(file.path.clone()),
        line,
        column,
        message: message.into(),
    });
}

/// Push a diagnostic addressed by zero-based line index and zero-based byte
/// column within [`scan::Line::text`].
///
/// The 1-based position is resolved through the file's single [`uf_infra::LineIndex`],
/// so every rule reports positions the same way.
fn push_at(
    diagnostics: &mut Vec<Diagnostic>,
    scan: &FileScan<'_>,
    rule: &'static str,
    severity: Severity,
    line_index: usize,
    column_index: usize,
    message: impl Into<String>,
) {
    let offset = scan.lines[line_index].offset + column_index;
    let position = scan.index.line_col(offset);
    push(
        diagnostics,
        scan.file,
        rule,
        severity,
        position.line,
        position.column,
        message,
    );
}

/// Push a diagnostic addressed by an offset inside [`scan::Line::code`].
fn push_in_code(
    diagnostics: &mut Vec<Diagnostic>,
    scan: &FileScan<'_>,
    rule: &'static str,
    severity: Severity,
    line_index: usize,
    code_offset: usize,
    message: impl Into<String>,
) {
    let column_index = scan.lines[line_index].code_offset() + code_offset;
    push_at(
        diagnostics,
        scan,
        rule,
        severity,
        line_index,
        column_index,
        message,
    );
}

// ---------------------------------------------------------------------------
// Flow parser
// ---------------------------------------------------------------------------

fn run_flow_syntax(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), LintError> {
    let Some(severity) = severity(config, "flow/syntax") else {
        return Ok(());
    };
    if !is_flow_syntax_target(&scan.file.path) {
        return Ok(());
    }

    let parser = FlowParser;
    let outcome = parser.validate_source(&scan.file.source)?;
    for diagnostic in outcome.diagnostics {
        push(
            diagnostics,
            scan.file,
            "flow/syntax",
            severity,
            diagnostic.line.unwrap_or(1) as usize,
            diagnostic.column.unwrap_or(0) as usize + 1,
            diagnostic.message,
        );
    }

    Ok(())
}

/// Whether `path` is Flow source uf should parse.
///
/// Flow lives in `.js` files here; `.flow` declaration sidecars are not part of
/// the product, so a file ending in `.flow` is someone else's convention and
/// parsing it would report syntax errors against declaration syntax uf never
/// emits.
fn is_flow_syntax_target(path: &str) -> bool {
    path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".mjs")
        || path.ends_with(".cjs")
}

// ---------------------------------------------------------------------------
// uniflowed/*
// ---------------------------------------------------------------------------

fn run_no_tabs(scan: &FileScan<'_>, config: &UniflowedConfig, diagnostics: &mut Vec<Diagnostic>) {
    let Some(severity) = severity(config, "uniflowed/no-tabs") else {
        return;
    };

    for offset in memchr_iter(b'\t', scan.file.source.as_bytes()) {
        let position = scan.index.line_col(offset);
        push(
            diagnostics,
            scan.file,
            "uniflowed/no-tabs",
            severity,
            position.line,
            position.column,
            "replace tabs with spaces",
        );
    }
}

fn run_no_trailing_whitespace(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "uniflowed/no-trailing-whitespace") else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        // The final `split` element is the text after the last `\n`; an empty one
        // is not a real line and must not be reported.
        if position + 1 == scan.lines.len() && line.text.is_empty() {
            continue;
        }
        let trimmed = line.text.trim_end_matches([' ', '\t']);
        if trimmed.len() != line.text.len() {
            push_at(
                diagnostics,
                scan,
                "uniflowed/no-trailing-whitespace",
                severity,
                position,
                trimmed.len(),
                "remove trailing whitespace",
            );
        }
    }
}

/// Package-manager invocations that belong in `uf.config.js` tasks instead.
const PACKAGE_MANAGER_WORDS: [&str; 4] = ["yarn", "pnpm", "bunx", "npx"];

fn run_no_npm_script_invocation(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "uniflowed/no-npm-script-invocation") else {
        return;
    };
    // `package.json` has its own, more specific rule.
    if scan.file.path.ends_with("package.json") {
        return;
    }

    const MESSAGE: &str =
        "declare the task in uf.config.js; uf projects do not shell out to npm/yarn/pnpm/bunx";

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_all(code, "npm run").filter(|&at| starts_word(code, at)) {
            push_in_code(
                diagnostics,
                scan,
                "uniflowed/no-npm-script-invocation",
                severity,
                position,
                at,
                MESSAGE,
            );
        }
        for word in PACKAGE_MANAGER_WORDS {
            for at in find_words(code, word) {
                push_in_code(
                    diagnostics,
                    scan,
                    "uniflowed/no-npm-script-invocation",
                    severity,
                    position,
                    at,
                    MESSAGE,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// flow/* built-ins that are decidable from source text
// ---------------------------------------------------------------------------

/// Types Flow's `unclear-type` lint rejects, with the advice for each.
const UNCLEAR_TYPES: [(&str, &str); 3] = [
    (
        "any",
        "avoid `any`; use `mixed`, opaque types, or generated router/action types",
    ),
    ("Object", "avoid `Object`; describe the object's shape"),
    ("Function", "avoid `Function`; describe the call signature"),
];

fn run_flow_unclear_type(
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

fn run_flow_deprecated_type(
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

fn run_flow_internal_type(
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

fn run_flow_ambiguous_object_type(
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

fn run_flow_unsafe_getters_setters(
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

fn run_flow_unsafe_object_assign(
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

fn run_flow_unnecessary_optional_chain(
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

fn run_flow_mixed_import_and_require(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::MixedImportAndRequire.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };
    if !scan.facts.has_esm_import {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_words(code, "require") {
            if prev_non_space(code, at).is_some_and(|(_, byte)| byte == b'.') {
                continue;
            }
            if !next_non_space(code, at + "require".len()).is_some_and(|(_, byte)| byte == b'(') {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "this module already uses `import`; do not mix in `require`",
            );
        }
    }
}

fn run_flow_non_const_var_export(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::NonConstVarExport.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let Some((at, _)) = next_non_space(code, 0) else {
            continue;
        };
        if identifier_len(code, at) != "export".len() || &code[at..at + 6] != "export" {
            continue;
        }
        let Some((keyword_at, _)) = next_non_space(code, at + 6) else {
            continue;
        };
        let len = identifier_len(code, keyword_at);
        if len == 0 || !matches!(&code[keyword_at..keyword_at + len], "var" | "let") {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            rule,
            severity,
            position,
            keyword_at,
            "exported bindings must be `const`; a mutable export is a live binding",
        );
    }
}

fn run_flow_export_renamed_default(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::ExportRenamedDefault.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_all(code, "as default")
            .filter(|&at| starts_word(code, at) && ends_word(code, at + "as default".len()))
        {
            push_in_code(
                diagnostics,
                scan,
                rule,
                severity,
                position,
                at,
                "renaming an export to `default` hides the real name; export it directly",
            );
        }
    }
}

/// Lowercase JSX intrinsic element names, which local bindings must not shadow.
static JSX_INTRINSICS: phf::Set<&'static str> = phf::phf_set! {
    "a", "abbr", "address", "area", "article", "aside", "audio",
    "b", "base", "bdi", "bdo", "big", "blockquote", "body", "br", "button",
    "canvas", "caption", "circle", "cite", "clipPath", "code", "col", "colgroup",
    "data", "datalist", "dd", "defs", "del", "details", "dfn", "dialog", "div",
    "dl", "dt", "ellipse", "em", "embed", "fieldset", "figcaption", "figure",
    "footer", "foreignObject", "form", "g", "h1", "h2", "h3", "h4", "h5", "h6",
    "head", "header", "hgroup", "hr", "html", "i", "iframe", "image", "img",
    "input", "ins", "kbd", "label", "legend", "li", "line", "linearGradient",
    "link", "main", "map", "mark", "marker", "mask", "menu", "meta", "meter",
    "nav", "noscript", "object", "ol", "optgroup", "option", "output", "p",
    "param", "path", "pattern", "picture", "polygon", "polyline", "pre",
    "progress", "q", "radialGradient", "rect", "rp", "rt", "ruby", "s", "samp",
    "script", "search", "section", "select", "slot", "small", "source", "span",
    "stop", "strong", "style", "sub", "summary", "sup", "svg", "table", "tbody",
    "td", "template", "text", "textarea", "tfoot", "th", "thead", "time",
    "title", "tr", "track", "tspan", "u", "ul", "use", "var", "video", "wbr",
};

/// Keywords that introduce a binding whose name follows.
const BINDING_KEYWORDS: [&str; 7] = [
    "const",
    "let",
    "var",
    "function",
    "component",
    "hook",
    "class",
];

fn run_flow_react_intrinsic_overlap(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let rule = FlowBuiltinLint::ReactIntrinsicOverlap.as_rule_id();
    let Some(severity) = severity(config, rule) else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let Some((mut at, _)) = next_non_space(code, 0) else {
            continue;
        };
        if code[at..].starts_with("export ") {
            let Some((next, _)) = next_non_space(code, at + "export ".len()) else {
                continue;
            };
            at = next;
        }
        let len = identifier_len(code, at);
        if len == 0 || !BINDING_KEYWORDS.contains(&&code[at..at + len]) {
            continue;
        }
        let Some((name_at, _)) = next_non_space(code, at + len) else {
            continue;
        };
        let name_len = identifier_len(code, name_at);
        if name_len == 0 || !JSX_INTRINSICS.contains(&code[name_at..name_at + name_len]) {
            continue;
        }
        push_in_code(
            diagnostics,
            scan,
            rule,
            severity,
            position,
            name_at,
            "this name shadows a JSX intrinsic element and silently changes what JSX means",
        );
    }
}

// ---------------------------------------------------------------------------
// react/*
// ---------------------------------------------------------------------------

fn run_react_component_syntax(
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

fn run_react_hook_syntax(
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

fn run_react_no_default_export_component(
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

fn run_react_no_render_side_effects(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react/no-render-side-effects") else {
        return;
    };
    if !scan.facts.declares_component {
        return;
    }

    const NEEDLES: [&str; 4] = [
        "Date.now(",
        "Math.random(",
        "localStorage.",
        "sessionStorage.",
    ];

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let Some(at) = NEEDLES.into_iter().find_map(|needle| code.find(needle)) else {
            continue;
        };
        push_in_code(
            diagnostics,
            scan,
            "react/no-render-side-effects",
            severity,
            position,
            at,
            "keep React render idempotent; move unstable reads into actions, effects, or loaders",
        );
    }
}

fn run_react_native_platform_split(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react-native/platform-split") else {
        return;
    };

    if !scan.facts.mentions_react_native
        || !(scan.file.source.contains("Platform.OS")
            || scan.file.source.contains("Platform.select"))
        || scan.file.path.contains(".ios.")
        || scan.file.path.contains(".android.")
        || scan.file.path.contains(".native.")
    {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        if let Some(at) = code.find("Platform.") {
            push_in_code(
                diagnostics,
                scan,
                "react-native/platform-split",
                severity,
                position,
                at,
                "prefer platform-specific files for React Native platform branches",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Structure rules: flow/nested-component, flow/nested-hook, react/hooks-rules
// ---------------------------------------------------------------------------

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
fn run_structure_rules(
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

// ---------------------------------------------------------------------------
// server/*
// ---------------------------------------------------------------------------

fn run_server_no_client_secret(
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

fn run_server_no_server_only_import_in_client(
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

fn run_server_use_client_directive_position(
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

fn run_server_use_server_actions(
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

// ---------------------------------------------------------------------------
// router/*, package/*, fetch/*
// ---------------------------------------------------------------------------

fn run_router_reserved_files(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "router/reserved-files") else {
        return;
    };

    let Some(file_name) = scan.file.path.rsplit('/').next() else {
        return;
    };
    if !file_name.starts_with("_uf.") {
        return;
    }
    if matches!(
        file_name,
        "_uf.layout.js" | "_uf.page.js" | "_uf.middleware.js"
    ) {
        return;
    }

    push(
        diagnostics,
        scan.file,
        "router/reserved-files",
        severity,
        1,
        1,
        "reserved router files must be _uf.layout.js, _uf.page.js, or _uf.middleware.js",
    );
}

fn run_package_no_npm_scripts(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "package/no-npm-scripts") else {
        return;
    };
    if !scan.file.path.ends_with("package.json") {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        if let Some(at) = line.text.find(r#""scripts""#) {
            push_at(
                diagnostics,
                scan,
                "package/no-npm-scripts",
                severity,
                position,
                at,
                "declare tasks in uf.config.js; npm scripts are not part of the uf toolchain",
            );
        }
    }
}

fn run_fetch_no_global_override(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "fetch/no-global-override") else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let at = code
            .find("globalThis.fetch")
            .or_else(|| code.find("window.fetch"))
            .or_else(|| code.find("global.fetch"));
        if let Some(at) = at {
            push_in_code(
                diagnostics,
                scan,
                "fetch/no-global-override",
                severity,
                position,
                at,
                "do not override global fetch; use @uniflowed/fetch explicit clients",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// security/*
// ---------------------------------------------------------------------------

/// The sanitizing package whose helpers may feed `dangerouslySetInnerHTML`.
const MARKDOWN_PACKAGE: &str = "@uniflowed/markdown";

/// Defends against the stored/reflected XSS class that React's
/// `dangerouslySetInnerHTML` has produced repeatedly across the ecosystem
/// (CVE-2018-6341 and the long tail of markdown-renderer XSS advisories):
/// unsanitized HTML reaching the DOM. Only values produced by a
/// `@uniflowed/markdown` helper — which sanitizes — are allowed through.
fn run_security_no_dangerously_set_inner_html(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "security/no-dangerously-set-inner-html") else {
        return;
    };
    if !scan.file.source.contains("dangerouslySetInnerHTML") {
        return;
    }

    let sanitizers = markdown_sanitizers(scan);

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        for at in find_words(code, "dangerouslySetInnerHTML") {
            // The `__html` value may wrap onto the next line, so both are checked.
            let next = scan
                .lines
                .get(position + 1)
                .map(|line| line.code())
                .unwrap_or("");
            if sanitizers
                .iter()
                .any(|&name| is_called_in(code, name) || is_called_in(next, name))
            {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                "security/no-dangerously-set-inner-html",
                severity,
                position,
                at,
                "unsanitized HTML is an XSS sink; render it through a @uniflowed/markdown helper",
            );
        }
    }
}

/// Names imported from `@uniflowed/markdown` in this file.
fn markdown_sanitizers<'a>(scan: &FileScan<'a>) -> Vec<&'a str> {
    let mut names = Vec::new();
    for line in &scan.lines {
        let code = line.code();
        if !code.contains(MARKDOWN_PACKAGE) || !code.contains("import") {
            continue;
        }
        let clause = code
            .split_once(" from ")
            .map(|(clause, _)| clause)
            .unwrap_or(code);
        let mut at = 0usize;
        while at < clause.len() {
            let len = identifier_len(clause, at);
            if len == 0 {
                at += 1;
                continue;
            }
            let word = &clause[at..at + len];
            if !matches!(word, "import" | "type" | "as" | "from") {
                names.push(word);
            }
            at += len;
        }
    }
    names
}

/// Whether `name` is used as a call or a namespace member in `code`.
fn is_called_in(code: &str, name: &str) -> bool {
    find_words(code, name).any(|at| {
        next_non_space(code, at + name.len()).is_some_and(|(_, byte)| byte == b'(' || byte == b'.')
    })
}

/// Timer APIs that accept a string body and `eval` it.
const TIMER_FUNCTIONS: [&str; 2] = ["setTimeout", "setInterval"];

/// Defends against the arbitrary-code-execution class that comes from turning
/// attacker-influenced strings into code (`eval`, `new Function`, and the
/// string form of `setTimeout`/`setInterval`).
fn run_security_no_eval(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "security/no-eval") else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();

        for at in find_words(code, "eval") {
            if !next_non_space(code, at + "eval".len()).is_some_and(|(_, byte)| byte == b'(') {
                continue;
            }
            push_in_code(
                diagnostics,
                scan,
                "security/no-eval",
                severity,
                position,
                at,
                "`eval` executes arbitrary code; parse the data instead",
            );
        }

        for at in find_words(code, "Function") {
            if !next_non_space(code, at + "Function".len()).is_some_and(|(_, byte)| byte == b'(') {
                continue;
            }
            let Some((keyword_at, "new")) = previous_word(code, at) else {
                continue;
            };
            push_in_code(
                diagnostics,
                scan,
                "security/no-eval",
                severity,
                position,
                keyword_at,
                "`new Function` compiles a string into code; write the function directly",
            );
        }

        for timer in TIMER_FUNCTIONS {
            for at in find_words(code, timer) {
                let Some((paren_at, b'(')) = next_non_space(code, at + timer.len()) else {
                    continue;
                };
                if !next_non_space(code, paren_at + 1)
                    .is_some_and(|(_, byte)| matches!(byte, b'\'' | b'"' | b'`'))
                {
                    continue;
                }
                push_in_code(
                    diagnostics,
                    scan,
                    "security/no-eval",
                    severity,
                    position,
                    at,
                    "a string timer body is evaluated as code; pass a function instead",
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
