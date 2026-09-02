//! Flow's built-in lint set, mapped into uf's `flow/` rule-id namespace.
//!
//! # Where this list comes from
//!
//! The authoritative set is Flow's own `flow_lint_settings` crate — specifically
//! `LintKind::as_str` (canonical spellings) and `LintKind::parse_from_str` (every
//! spelling accepted in a `.flowconfig` `[lints]` section):
//! <https://github.com/facebook/flow/blob/main/rust_port/crates/flow_lint_settings/src/lints.rs>
//!
//! It was cross-checked against the user-facing reference at
//! <https://flow.org/en/docs/linting/rule-reference/>. Both were read on
//! 2026-09-02; the two agree on every name below.
//!
//! Four names that circulate in older Flow write-ups are deliberately **absent**
//! because current Flow does not accept them, and inventing them here would make
//! `uf lint` reject configs Flow itself accepts (and vice versa):
//!
//! - `implicit-inexact-object` — removed when Flow moved to exact-by-default;
//!   the surviving check is [`FlowBuiltinLint::AmbiguousObjectType`].
//! - `unused-promise-in-async-scope` — renamed to `unused-promise`.
//! - `require-explicit-import-type` — never a Flow lint.
//! - `deprecated-class-static-blocks` — never a Flow lint.
//!
//! # Umbrella names
//!
//! Flow lets one configured name cover several reportable lints. `sketchy-null`
//! is the only such umbrella that survives here (see
//! [`FlowBuiltinLint::members`]). `sketchy-number` and `deprecated-type` each
//! have exactly one member in Flow (`sketchy-number-and`, `deprecated-type-bool`),
//! so uf keeps the short documented spelling as canonical and accepts the long
//! one as an alias in [`FlowBuiltinLint::from_str`].

mod table;

#[cfg(test)]
mod tests;

use std::str::FromStr;

use thiserror::Error;
use uf_infra::CompactString;

use table::{BY_NAME, LINTS};

/// A single Flow built-in lint, identified by the name Flow accepts in `[lints]`.
///
/// Variants are ordered alphabetically by lint name, and that order is load
/// bearing: [`LINTS`] is indexed by `self as usize`, which is what keeps
/// [`FlowBuiltinLint::as_rule_id`] allocation free and branch free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum FlowBuiltinLint {
    /// `ambiguous-object-type`
    AmbiguousObjectType,
    /// `default-import-access`
    DefaultImportAccess,
    /// `deprecated-type`
    DeprecatedType,
    /// `export-renamed-default`
    ExportRenamedDefault,
    /// `internal-type`
    InternalType,
    /// `invalid-import-star-use`
    InvalidImportStarUse,
    /// `invalid-this-arg`
    InvalidThisArg,
    /// `libdef-override`
    LibdefOverride,
    /// `mixed-import-and-require`
    MixedImportAndRequire,
    /// `nested-component`
    NestedComponent,
    /// `nested-hook`
    NestedHook,
    /// `non-const-var-export`
    NonConstVarExport,
    /// `nonstrict-import`
    NonstrictImport,
    /// `react-intrinsic-overlap`
    ReactIntrinsicOverlap,
    /// `require-explicit-enum-checks`
    RequireExplicitEnumChecks,
    /// `require-explicit-enum-switch-cases`
    RequireExplicitEnumSwitchCases,
    /// `sketchy-null` — umbrella over the five typed variants.
    SketchyNull,
    /// `sketchy-null-bigint`
    SketchyNullBigInt,
    /// `sketchy-null-bool`
    SketchyNullBool,
    /// `sketchy-null-mixed`
    SketchyNullMixed,
    /// `sketchy-null-number`
    SketchyNullNumber,
    /// `sketchy-null-string`
    SketchyNullString,
    /// `sketchy-number` — Flow spells the single member `sketchy-number-and`.
    SketchyNumber,
    /// `this-in-exported-function`
    ThisInExportedFunction,
    /// `unclear-type`
    UnclearType,
    /// `uninitialized-instance-property`
    UninitializedInstanceProperty,
    /// `unnecessary-invariant`
    UnnecessaryInvariant,
    /// `unnecessary-optional-chain`
    UnnecessaryOptionalChain,
    /// `unsafe-getters-setters`
    UnsafeGettersSetters,
    /// `unsafe-object-assign`
    UnsafeObjectAssign,
    /// `untyped-import`
    UntypedImport,
    /// `untyped-type-import`
    UntypedTypeImport,
    /// `unused-promise`
    UnusedPromise,
}

/// The `flow/` prefix every Flow built-in rule id carries.
pub const FLOW_NAMESPACE: &str = "flow/";

impl FlowBuiltinLint {
    /// Number of Flow built-in lints uf models.
    pub const COUNT: usize = 33;

    /// Every Flow built-in lint, in ascending lint-name order.
    pub fn all() -> impl ExactSizeIterator<Item = FlowBuiltinLint> {
        LINTS.iter().map(|entry| entry.lint)
    }

    /// The bare Flow lint name, e.g. `"sketchy-null"`.
    #[inline]
    pub fn as_name(self) -> &'static str {
        LINTS[self as usize].name
    }

    /// The uf rule id, e.g. `"flow/sketchy-null"`.
    #[inline]
    pub fn as_rule_id(self) -> &'static str {
        LINTS[self as usize].rule_id
    }

    /// Lints this name expands to when Flow reports a violation.
    ///
    /// Empty for every leaf lint; only `sketchy-null` has members.
    #[inline]
    pub fn members(self) -> &'static [FlowBuiltinLint] {
        LINTS[self as usize].members
    }

    /// Whether this name only exists to configure other lints in bulk.
    #[inline]
    pub fn is_umbrella(self) -> bool {
        !self.members().is_empty()
    }

    /// Resolve a bare Flow lint name (no `flow/` prefix).
    #[inline]
    pub fn from_lint_name(name: &str) -> Option<Self> {
        BY_NAME.get(name).copied()
    }

    /// Resolve a fully qualified uf rule id such as `"flow/unclear-type"`.
    #[inline]
    pub fn from_rule_id(rule_id: &str) -> Option<Self> {
        rule_id
            .strip_prefix(FLOW_NAMESPACE)
            .and_then(Self::from_lint_name)
    }
}

/// Failure to resolve a Flow built-in lint name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FlowLintParseError {
    /// The name is not one Flow accepts in a `[lints]` section.
    #[error("`{name}` is not a Flow built-in lint")]
    UnknownLint {
        /// The rejected spelling, as written by the user.
        name: CompactString,
    },
}

impl FromStr for FlowBuiltinLint {
    type Err = FlowLintParseError;

    /// Accepts both `"unclear-type"` and `"flow/unclear-type"`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_rule_id(value)
            .or_else(|| Self::from_lint_name(value))
            .ok_or_else(|| FlowLintParseError::UnknownLint {
                name: CompactString::from(value),
            })
    }
}
