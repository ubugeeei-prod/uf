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
//!
//! Two of the React rules — `react/hooks-rules` and
//! `react/no-render-side-effects` — are decided by `uf_react_compiler` rather
//! than here. They are the same question the compiler's syntax mode asks, so
//! the predicate lives there and this crate reports what it found: one
//! predicate, one home, and no way for `uf lint` and `uf build` to disagree
//! about whether a component is compilable.

mod flow_builtin;
mod rules;
mod runner;
mod scan;
mod suppression;

use thiserror::Error;
use uf_config::{RuleLevel, UniflowedConfig};

pub use crate::flow_builtin::{FLOW_NAMESPACE, FlowBuiltinLint, FlowLintParseError};
pub use crate::rules::{
    RuleCategory, RuleDescriptor, RuleRequirement, canonical_rule_id, rule, rules,
};

use crate::rules::deprecated_aliases_for;
use crate::runner::{
    run_fetch_no_global_override, run_flow_ambiguous_object_type, run_flow_deprecated_type,
    run_flow_export_renamed_default, run_flow_internal_type, run_flow_mixed_import_and_require,
    run_flow_non_const_var_export, run_flow_syntax, run_flow_unclear_type,
    run_flow_unnecessary_optional_chain, run_flow_unsafe_getters_setters,
    run_flow_unsafe_object_assign, run_no_npm_script_invocation, run_no_tabs,
    run_no_trailing_whitespace, run_package_no_npm_scripts, run_react_compiler_rules,
    run_react_component_syntax, run_react_hook_syntax, run_react_native_platform_split,
    run_react_no_default_export_component, run_router_reserved_files,
    run_security_no_dangerously_set_inner_html, run_security_no_eval, run_server_no_client_secret,
    run_server_no_server_only_import_in_client, run_server_use_client_directive_position,
    run_server_use_server_actions, run_structure_rules,
};
use crate::scan::FileScan;
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
    let per_file = uf_infra::parallel::map(files, |file| lint_file(file, config))?;

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

    run_react_component_syntax(&scan, config, &mut diagnostics);
    run_react_hook_syntax(&scan, config, &mut diagnostics);
    run_react_no_default_export_component(&scan, config, &mut diagnostics);
    run_react_compiler_rules(&scan, config, &mut diagnostics);
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

pub(crate) fn severity(config: &UniflowedConfig, rule: &'static str) -> Option<Severity> {
    match configured_level(config, rule).unwrap_or(RuleLevel::Off) {
        RuleLevel::Off => None,
        RuleLevel::Warn => Some(Severity::Warn),
        RuleLevel::Error => Some(Severity::Error),
    }
}

pub(crate) fn push(
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
pub(crate) fn push_at(
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
pub(crate) fn push_in_code(
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

#[cfg(test)]
mod tests;
