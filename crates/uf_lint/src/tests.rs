//! Lint runner tests, one module per rule namespace so a rule's tests sit next to
//! the runner that owns it.
//!
//! Nearly every test goes through [`only`], which enables exactly one rule. That
//! is what keeps a fixture written for one rule from being scored by another rule
//! that happens to fire on the same line.

mod catalogue;
mod driver;
mod fetch;
mod flow_expression;
mod flow_module;
mod flow_syntax;
mod flow_type;
mod input;
mod package;
mod react;
mod react_native;
mod router;
mod security;
mod server;
mod structure;
mod suppression;
mod unavailable;
mod uniflowed;

use uf_config::{RuleLevel, UniflowedConfig};
use uf_infra::CompactString;

use super::*;

fn source(source: &str) -> SourceFile {
    SourceFile {
        path: "src/app/page.jsx".to_string(),
        source: source.to_string(),
    }
}

fn at(path: &str, source: &str) -> SourceFile {
    SourceFile {
        path: path.to_string(),
        source: source.to_string(),
    }
}

/// A config with exactly one rule enabled, so a rule's tests cannot be muddied by
/// another rule firing on the same fixture.
fn only(rule: &str) -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    config
        .lint
        .rules
        .insert(CompactString::from(rule), RuleLevel::Error);
    config
}

/// Diagnostics produced by `rule` alone for `path`/`text`.
fn lint_one(rule: &str, path: &str, text: &str) -> Vec<Diagnostic> {
    lint_source(&at(path, text), &only(rule))
        .expect("lint")
        .diagnostics
}

/// Diagnostics produced by `rule` alone for a default `.js` module.
fn lint_js(rule: &str, text: &str) -> Vec<Diagnostic> {
    lint_one(rule, "app/index.js", text)
}

fn fired(diagnostics: &[Diagnostic], rule: &str) -> bool {
    diagnostics.iter().any(|diagnostic| diagnostic.rule == rule)
}
