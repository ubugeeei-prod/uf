//! Tests for the React Compiler syntax-mode validator, one file per rule.

mod hooks;
mod mutation;
mod official;
mod plugin;
mod refs;
mod render;
mod robustness;

use crate::rule::{Finding, ReactDiagnostic};
use crate::validate;

/// Validate a module, failing the test if it could not be analysed at all.
fn check(source: &str) -> Vec<ReactDiagnostic> {
    validate(source).unwrap_or_else(|error| panic!("expected the module to validate: {error}"))
}

/// The findings of a module, in report order.
fn findings(source: &str) -> Vec<Finding> {
    check(source)
        .into_iter()
        .map(|entry| entry.finding)
        .collect()
}

/// Assert that a module produces no findings at all.
fn accepts(source: &str) {
    let found = findings(source);
    assert!(found.is_empty(), "expected no findings, got {found:?}");
}
