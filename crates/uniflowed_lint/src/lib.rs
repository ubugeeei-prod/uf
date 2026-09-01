use rayon::prelude::*;
use thiserror::Error;
use uniflowed_config::{RuleLevel, UniflowedConfig};
use uniflowed_flow::FlowParser;
use uniflowed_infra::{LineIndex, memchr_iter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: Severity,
    pub path: Option<String>,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintReport {
    pub diagnostics: Vec<Diagnostic>,
    pub files_checked: usize,
}

impl LintReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }
}

#[derive(Debug, Error)]
pub enum LintError {
    #[error(transparent)]
    Flow(#[from] uniflowed_flow::FlowError),
}

pub fn lint_sources(
    files: &[SourceFile],
    config: &UniflowedConfig,
) -> Result<LintReport, LintError> {
    let reports = files
        .par_iter()
        .map(|file| lint_source(file, config))
        .collect::<Result<Vec<_>, _>>()?;

    let mut diagnostics = reports
        .into_iter()
        .flat_map(|report| report.diagnostics)
        .collect::<Vec<_>>();
    diagnostics.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
            .then(a.rule.cmp(b.rule))
    });

    Ok(LintReport {
        diagnostics,
        files_checked: files.len(),
    })
}

pub fn lint_source(file: &SourceFile, config: &UniflowedConfig) -> Result<LintReport, LintError> {
    let mut diagnostics = Vec::new();

    run_flow_syntax(file, config, &mut diagnostics)?;
    run_flow_no_explicit_any(file, config, &mut diagnostics);
    run_no_tabs(file, config, &mut diagnostics);
    run_no_trailing_whitespace(file, config, &mut diagnostics);
    run_react_component_syntax(file, config, &mut diagnostics);
    run_react_hook_syntax(file, config, &mut diagnostics);
    run_react_no_render_side_effects(file, config, &mut diagnostics);
    run_react_native_platform_split(file, config, &mut diagnostics);
    run_server_no_client_secret(file, config, &mut diagnostics);
    run_server_use_server_actions(file, config, &mut diagnostics);
    run_router_reserved_files(file, config, &mut diagnostics);

    Ok(LintReport {
        diagnostics,
        files_checked: 1,
    })
}

fn severity(config: &UniflowedConfig, rule: &'static str) -> Option<Severity> {
    config
        .lint
        .rules
        .get(rule)
        .copied()
        .unwrap_or(RuleLevel::Off)
        .then_severity()
}

trait RuleLevelExt {
    fn then_severity(self) -> Option<Severity>;
}

impl RuleLevelExt for RuleLevel {
    fn then_severity(self) -> Option<Severity> {
        match self {
            RuleLevel::Off => None,
            RuleLevel::Warn => Some(Severity::Warn),
            RuleLevel::Error => Some(Severity::Error),
        }
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

fn run_flow_syntax(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), LintError> {
    let Some(severity) = severity(config, "flow/syntax") else {
        return Ok(());
    };

    let parser = FlowParser;
    let outcome = parser.validate_source(&file.source)?;
    for diagnostic in outcome.diagnostics {
        push(
            diagnostics,
            file,
            "flow/syntax",
            severity,
            diagnostic.line.unwrap_or(1) as usize,
            diagnostic.column.unwrap_or(0) as usize + 1,
            diagnostic.message,
        );
    }

    Ok(())
}

fn run_no_tabs(file: &SourceFile, config: &UniflowedConfig, diagnostics: &mut Vec<Diagnostic>) {
    let Some(severity) = severity(config, "uniflowed/no-tabs") else {
        return;
    };

    let index = LineIndex::new(&file.source);
    for offset in memchr_iter(b'\t', file.source.as_bytes()) {
        let position = index.line_col(offset);
        push(
            diagnostics,
            file,
            "uniflowed/no-tabs",
            severity,
            position.line,
            position.column,
            "replace tabs with spaces",
        );
    }
}

fn run_flow_no_explicit_any(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "flow/type-aware/no-explicit-any") else {
        return;
    };

    for (line_index, line) in file.source.lines().enumerate() {
        let Some(column) = line.find("any") else {
            continue;
        };
        let before = line[..column].chars().next_back();
        let after = line[column + 3..].chars().next();
        if before.is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric())
            || after.is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        {
            continue;
        }

        push(
            diagnostics,
            file,
            "flow/type-aware/no-explicit-any",
            severity,
            line_index + 1,
            column + 1,
            "avoid `any`; use `mixed`, opaque types, or generated router/action types",
        );
    }
}

fn run_no_trailing_whitespace(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "uniflowed/no-trailing-whitespace") else {
        return;
    };

    for (line_index, line) in file.source.lines().enumerate() {
        let trimmed = line.trim_end_matches([' ', '\t']);
        if trimmed.len() != line.len() {
            push(
                diagnostics,
                file,
                "uniflowed/no-trailing-whitespace",
                severity,
                line_index + 1,
                trimmed.len() + 1,
                "remove trailing whitespace",
            );
        }
    }
}

fn run_react_component_syntax(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react/component-syntax") else {
        return;
    };

    if !(file.path.ends_with(".jsx")
        || file.path.ends_with(".tsx")
        || file.source.contains("React"))
    {
        return;
    }

    for (line_index, line) in file.source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
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
            push(
                diagnostics,
                file,
                "react/component-syntax",
                severity,
                line_index + 1,
                leading + 1,
                "prefer Flow `component` syntax for React components",
            );
        }
    }
}

fn run_react_hook_syntax(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react/hook-syntax") else {
        return;
    };

    for (line_index, line) in file.source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        let Some(name) = trimmed
            .strip_prefix("function ")
            .and_then(|tail| tail.split(['(', '<']).next())
        else {
            continue;
        };

        if name.starts_with("use")
            && name
                .chars()
                .nth(3)
                .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            push(
                diagnostics,
                file,
                "react/hook-syntax",
                severity,
                line_index + 1,
                leading + 1,
                "prefer Flow `hook` syntax for React hooks",
            );
        }
    }
}

fn run_server_no_client_secret(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "server/no-client-secret") else {
        return;
    };

    if !file.source.contains("\"use client\"") && !file.source.contains("'use client'") {
        return;
    }

    for (line_index, line) in file.source.lines().enumerate() {
        if line.contains("SECRET") || line.contains("PRIVATE_") {
            push(
                diagnostics,
                file,
                "server/no-client-secret",
                severity,
                line_index + 1,
                line.find("SECRET")
                    .or_else(|| line.find("PRIVATE_"))
                    .map(|column| column + 1)
                    .unwrap_or(1),
                "client modules must not read private server secrets",
            );
        }
    }
}

fn run_react_no_render_side_effects(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react/no-render-side-effects") else {
        return;
    };

    if !file.source.contains("component ") {
        return;
    }

    for (line_index, line) in file.source.lines().enumerate() {
        let needle = [
            "Date.now(",
            "Math.random(",
            "localStorage.",
            "sessionStorage.",
        ]
        .into_iter()
        .find(|needle| line.contains(needle));
        let Some(needle) = needle else {
            continue;
        };
        push(
            diagnostics,
            file,
            "react/no-render-side-effects",
            severity,
            line_index + 1,
            line.find(needle).map(|column| column + 1).unwrap_or(1),
            "keep React render idempotent; move unstable reads into actions, effects, or loaders",
        );
    }
}

fn run_react_native_platform_split(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "react-native/platform-split") else {
        return;
    };

    if !file.source.contains("react-native")
        || !(file.source.contains("Platform.OS") || file.source.contains("Platform.select"))
        || file.path.contains(".ios.")
        || file.path.contains(".android.")
        || file.path.contains(".native.")
    {
        return;
    }

    for (line_index, line) in file.source.lines().enumerate() {
        if let Some(column) = line.find("Platform.") {
            push(
                diagnostics,
                file,
                "react-native/platform-split",
                severity,
                line_index + 1,
                column + 1,
                "prefer platform-specific files for React Native platform branches",
            );
        }
    }
}

fn run_server_use_server_actions(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "server/use-server-actions") else {
        return;
    };

    if !(file.path.starts_with("server/")
        || file.path.ends_with(".server.flow")
        || file.path.ends_with(".server.js"))
        || !file.source.contains("serverAction")
    {
        return;
    }

    let first_code_line = file
        .source
        .lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//")
        })
        .unwrap_or("");

    if first_code_line.trim() != r#""use server";"# && first_code_line.trim() != r#"'use server';"# {
        push(
            diagnostics,
            file,
            "server/use-server-actions",
            severity,
            1,
            1,
            r#"server action modules must start with "use server";"#,
        );
    }
}

fn run_router_reserved_files(
    file: &SourceFile,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "router/reserved-files") else {
        return;
    };

    let Some(file_name) = file.path.rsplit('/').next() else {
        return;
    };
    if !file_name.starts_with("_uf.") {
        return;
    }
    if matches!(
        file_name,
        "_uf.layout.flow" | "_uf.page.flow" | "_uf.middleware.flow"
    ) {
        return;
    }

    push(
        diagnostics,
        file,
        "router/reserved-files",
        severity,
        1,
        1,
        "reserved router files must be _uf.layout.flow, _uf.page.flow, or _uf.middleware.flow",
    );
}

#[cfg(test)]
mod tests {
    use uniflowed_config::{RuleLevel, UniflowedConfig};
    use uniflowed_infra::CompactString;

    use super::*;

    fn source(source: &str) -> SourceFile {
        SourceFile {
            path: "src/app/page.jsx".to_string(),
            source: source.to_string(),
        }
    }

    #[test]
    fn reports_tabs_and_trailing_whitespace() {
        let report = lint_source(
            &source("// @flow\n\tconst x: number = 1;  \n"),
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(report.has_errors());
        assert_eq!(report.diagnostics.len(), 2);
        assert_eq!(report.diagnostics[0].rule, "uniflowed/no-tabs");
        assert_eq!(
            report.diagnostics[1].rule,
            "uniflowed/no-trailing-whitespace"
        );
    }

    #[test]
    fn rule_levels_can_disable_builtin_rules() {
        let mut config = UniflowedConfig::default();
        config.lint.rules.insert(
            CompactString::const_new("uniflowed/no-tabs"),
            RuleLevel::Off,
        );

        let report =
            lint_source(&source("// @flow\n\tconst x: number = 1;\n"), &config).expect("lint");

        assert!(!report.has_errors());
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn reports_flow_parse_errors() {
        let report = lint_source(&source("// @flow\ntype = ;\n"), &UniflowedConfig::default())
            .expect("lint");

        assert!(report.has_errors());
        assert_eq!(report.diagnostics[0].rule, "flow/syntax");
    }

    #[test]
    fn type_aware_rule_blocks_explicit_any() {
        let report = lint_source(
            &source("// @flow\ntype Props = { value: any };\n"),
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "flow/type-aware/no-explicit-any")
        );
    }

    #[test]
    fn framework_rule_prefers_component_syntax() {
        let report = lint_source(
            &source("// @flow\nimport * as React from '@uniflowed/react';\nfunction Button(): React.Node { return null; }\n"),
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "react/component-syntax")
        );
    }

    #[test]
    fn hook_rule_prefers_flow_hook_syntax() {
        let report = lint_source(
            &source("// @flow\nfunction useThing(): number { return 1; }\n"),
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "react/hook-syntax")
        );
    }

    #[test]
    fn server_rule_rejects_secret_reads_in_client_modules() {
        let report = lint_source(
            &source("// @flow\n'use client';\nconst token = process.env.PRIVATE_TOKEN;\n"),
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "server/no-client-secret")
        );
    }

    #[test]
    fn server_actions_require_use_server_directive() {
        let report = lint_source(
            &SourceFile {
                path: "server/actions.flow".to_string(),
                source: "// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n"
                    .to_string(),
            },
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "server/use-server-actions")
        );
    }

    #[test]
    fn server_actions_accept_use_server_directive() {
        let report = lint_source(
            &SourceFile {
                path: "server/actions.flow".to_string(),
                source: "\"use server\";\n// @flow\nimport { serverAction } from '@uniflowed/server';\nexport const save = serverAction(() => {});\n"
                    .to_string(),
            },
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(
            !report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "server/use-server-actions")
        );
    }

    #[test]
    fn router_reserved_files_are_constrained() {
        let report = lint_source(
            &SourceFile {
                path: "app/_uf.route.flow".to_string(),
                source: "// @flow\n".to_string(),
            },
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "router/reserved-files")
        );
    }

    #[test]
    fn render_side_effects_are_errors_by_default() {
        let report = lint_source(
            &source("// @flow\ncomponent Clock() { return <p>{Date.now()}</p>; }\n"),
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "react/no-render-side-effects")
        );
    }

    #[test]
    fn react_native_rule_prefers_platform_files() {
        let report = lint_source(
            &SourceFile {
                path: "src/app/Button.jsx".to_string(),
                source: "// @flow\nimport { Platform } from '@uniflowed/react-native';\nconst name = Platform.OS;\n"
                    .to_string(),
            },
            &UniflowedConfig::default(),
        )
        .expect("lint");

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "react-native/platform-split")
        );
    }
}
