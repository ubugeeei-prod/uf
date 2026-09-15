//! `react-compiler/*`: the official React Compiler's diagnostics, one rule per
//! category.
//!
//! Nothing in this module decides whether code breaks a rule of React.
//! [`uf_transform::lint`] runs the official compiler, `react_compiler`, with the
//! options `eslint-plugin-react-hooks` runs it with and over the modules that
//! plugin hands it, and returns what the compiler reported. This module files
//! each diagnostic under the rule named for its category, at the level the
//! project set, with the compiler's message. [`super::react_tree`] runs it on
//! the tree [`super::module_tree`] parsed, so the module is read once for every
//! rule that needs it.
//!
//! A rule's name is the compiler's own name for its category — the name
//! `eslint-plugin-react-hooks` exposes that category under — so
//! `react-hooks/purity` in an ESLint config is `react-compiler/purity` here.

use uf_config::UniflowedConfig;
use uf_transform::ErrorCategory;

use crate::{Severity, severity};

/// The compiler's `Hooks` category: the Rules of Hooks.
pub(crate) const HOOKS: &str = "react-compiler/hooks";

/// The compiler's `Purity` category: a known-impure function called during
/// render.
pub(crate) const PURITY: &str = "react-compiler/purity";

/// The compiler's `Globals` category: a variable declared outside a component
/// or hook, reassigned or mutated during render.
pub(crate) const GLOBALS: &str = "react-compiler/globals";

/// Every `react-compiler/*` rule.
const RULES: [&str; 3] = [GLOBALS, HOOKS, PURITY];

/// The rule a category is filed under, or `None` for a category `uf lint` does
/// not report.
///
/// A `match` with no wildcard, so that a category a newer compiler adds does
/// not compile until somebody decides where it goes.
pub(super) const fn rule_for(category: ErrorCategory) -> Option<&'static str> {
    match category {
        ErrorCategory::Hooks => Some(HOOKS),
        ErrorCategory::Purity => Some(PURITY),
        ErrorCategory::Globals => Some(GLOBALS),
        ErrorCategory::CapitalizedCalls
        | ErrorCategory::StaticComponents
        | ErrorCategory::UseMemo
        | ErrorCategory::VoidUseMemo
        | ErrorCategory::PreserveManualMemo
        | ErrorCategory::MemoDependencies
        | ErrorCategory::IncompatibleLibrary
        | ErrorCategory::Immutability
        | ErrorCategory::Refs
        | ErrorCategory::EffectDependencies
        | ErrorCategory::EffectExhaustiveDependencies
        | ErrorCategory::EffectSetState
        | ErrorCategory::EffectDerivationsOfState
        | ErrorCategory::ErrorBoundaries
        | ErrorCategory::RenderSetState
        | ErrorCategory::Invariant
        | ErrorCategory::Todo
        | ErrorCategory::Syntax
        | ErrorCategory::UnsupportedSyntax
        | ErrorCategory::Config
        | ErrorCategory::Gating
        | ErrorCategory::Suppression
        | ErrorCategory::FBT => None,
    }
}

/// The level each `react-compiler/*` rule runs at in this project.
pub(super) struct CompilerWork {
    levels: [(&'static str, Option<Severity>); RULES.len()],
}

impl CompilerWork {
    /// The level `rule` runs at, or `None` when it is off.
    pub(super) fn level(&self, rule: &str) -> Option<Severity> {
        self.levels
            .iter()
            .find(|(id, _)| *id == rule)
            .and_then(|(_, level)| *level)
    }
}

/// The compiler's rules as this project configured them, or `None` when every
/// one of them is off and the compiler has nothing to report.
pub(super) fn wanted(config: &UniflowedConfig) -> Option<CompilerWork> {
    let levels = RULES.map(|rule| (rule, severity(config, rule)));
    levels
        .iter()
        .any(|(_, level)| level.is_some())
        .then_some(CompilerWork { levels })
}
