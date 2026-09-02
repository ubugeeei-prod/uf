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

use std::str::FromStr;

use thiserror::Error;
use uf_infra::CompactString;

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

/// One row of the name/id table backing [`FlowBuiltinLint`].
#[derive(Debug, Clone, Copy)]
struct LintEntry {
    lint: FlowBuiltinLint,
    name: &'static str,
    rule_id: &'static str,
    members: &'static [FlowBuiltinLint],
}

const fn leaf(lint: FlowBuiltinLint, name: &'static str, rule_id: &'static str) -> LintEntry {
    LintEntry {
        lint,
        name,
        rule_id,
        members: &[],
    }
}

/// Members of the `sketchy-null` umbrella, in the order Flow expands them.
const SKETCHY_NULL_MEMBERS: &[FlowBuiltinLint] = &[
    FlowBuiltinLint::SketchyNullBool,
    FlowBuiltinLint::SketchyNullString,
    FlowBuiltinLint::SketchyNullNumber,
    FlowBuiltinLint::SketchyNullBigInt,
    FlowBuiltinLint::SketchyNullMixed,
];

/// The single source of truth for Flow lint name ↔ uf rule id.
///
/// Indexed by `FlowBuiltinLint as usize`; `lint_table_is_indexed_by_discriminant`
/// pins that invariant down.
const LINTS: [LintEntry; FlowBuiltinLint::COUNT] = [
    leaf(
        FlowBuiltinLint::AmbiguousObjectType,
        "ambiguous-object-type",
        "flow/ambiguous-object-type",
    ),
    leaf(
        FlowBuiltinLint::DefaultImportAccess,
        "default-import-access",
        "flow/default-import-access",
    ),
    leaf(
        FlowBuiltinLint::DeprecatedType,
        "deprecated-type",
        "flow/deprecated-type",
    ),
    leaf(
        FlowBuiltinLint::ExportRenamedDefault,
        "export-renamed-default",
        "flow/export-renamed-default",
    ),
    leaf(
        FlowBuiltinLint::InternalType,
        "internal-type",
        "flow/internal-type",
    ),
    leaf(
        FlowBuiltinLint::InvalidImportStarUse,
        "invalid-import-star-use",
        "flow/invalid-import-star-use",
    ),
    leaf(
        FlowBuiltinLint::InvalidThisArg,
        "invalid-this-arg",
        "flow/invalid-this-arg",
    ),
    leaf(
        FlowBuiltinLint::LibdefOverride,
        "libdef-override",
        "flow/libdef-override",
    ),
    leaf(
        FlowBuiltinLint::MixedImportAndRequire,
        "mixed-import-and-require",
        "flow/mixed-import-and-require",
    ),
    leaf(
        FlowBuiltinLint::NestedComponent,
        "nested-component",
        "flow/nested-component",
    ),
    leaf(
        FlowBuiltinLint::NestedHook,
        "nested-hook",
        "flow/nested-hook",
    ),
    leaf(
        FlowBuiltinLint::NonConstVarExport,
        "non-const-var-export",
        "flow/non-const-var-export",
    ),
    leaf(
        FlowBuiltinLint::NonstrictImport,
        "nonstrict-import",
        "flow/nonstrict-import",
    ),
    leaf(
        FlowBuiltinLint::ReactIntrinsicOverlap,
        "react-intrinsic-overlap",
        "flow/react-intrinsic-overlap",
    ),
    leaf(
        FlowBuiltinLint::RequireExplicitEnumChecks,
        "require-explicit-enum-checks",
        "flow/require-explicit-enum-checks",
    ),
    leaf(
        FlowBuiltinLint::RequireExplicitEnumSwitchCases,
        "require-explicit-enum-switch-cases",
        "flow/require-explicit-enum-switch-cases",
    ),
    LintEntry {
        lint: FlowBuiltinLint::SketchyNull,
        name: "sketchy-null",
        rule_id: "flow/sketchy-null",
        members: SKETCHY_NULL_MEMBERS,
    },
    leaf(
        FlowBuiltinLint::SketchyNullBigInt,
        "sketchy-null-bigint",
        "flow/sketchy-null-bigint",
    ),
    leaf(
        FlowBuiltinLint::SketchyNullBool,
        "sketchy-null-bool",
        "flow/sketchy-null-bool",
    ),
    leaf(
        FlowBuiltinLint::SketchyNullMixed,
        "sketchy-null-mixed",
        "flow/sketchy-null-mixed",
    ),
    leaf(
        FlowBuiltinLint::SketchyNullNumber,
        "sketchy-null-number",
        "flow/sketchy-null-number",
    ),
    leaf(
        FlowBuiltinLint::SketchyNullString,
        "sketchy-null-string",
        "flow/sketchy-null-string",
    ),
    leaf(
        FlowBuiltinLint::SketchyNumber,
        "sketchy-number",
        "flow/sketchy-number",
    ),
    leaf(
        FlowBuiltinLint::ThisInExportedFunction,
        "this-in-exported-function",
        "flow/this-in-exported-function",
    ),
    leaf(
        FlowBuiltinLint::UnclearType,
        "unclear-type",
        "flow/unclear-type",
    ),
    leaf(
        FlowBuiltinLint::UninitializedInstanceProperty,
        "uninitialized-instance-property",
        "flow/uninitialized-instance-property",
    ),
    leaf(
        FlowBuiltinLint::UnnecessaryInvariant,
        "unnecessary-invariant",
        "flow/unnecessary-invariant",
    ),
    leaf(
        FlowBuiltinLint::UnnecessaryOptionalChain,
        "unnecessary-optional-chain",
        "flow/unnecessary-optional-chain",
    ),
    leaf(
        FlowBuiltinLint::UnsafeGettersSetters,
        "unsafe-getters-setters",
        "flow/unsafe-getters-setters",
    ),
    leaf(
        FlowBuiltinLint::UnsafeObjectAssign,
        "unsafe-object-assign",
        "flow/unsafe-object-assign",
    ),
    leaf(
        FlowBuiltinLint::UntypedImport,
        "untyped-import",
        "flow/untyped-import",
    ),
    leaf(
        FlowBuiltinLint::UntypedTypeImport,
        "untyped-type-import",
        "flow/untyped-type-import",
    ),
    leaf(
        FlowBuiltinLint::UnusedPromise,
        "unused-promise",
        "flow/unused-promise",
    ),
];

/// Name → lint lookup, including the two long spellings Flow also accepts.
///
/// A perfect hash keeps config resolution and suppression-comment parsing free
/// of both allocation and linear scans.
static BY_NAME: phf::Map<&'static str, FlowBuiltinLint> = phf::phf_map! {
    "ambiguous-object-type" => FlowBuiltinLint::AmbiguousObjectType,
    "default-import-access" => FlowBuiltinLint::DefaultImportAccess,
    "deprecated-type" => FlowBuiltinLint::DeprecatedType,
    "deprecated-type-bool" => FlowBuiltinLint::DeprecatedType,
    "export-renamed-default" => FlowBuiltinLint::ExportRenamedDefault,
    "internal-type" => FlowBuiltinLint::InternalType,
    "invalid-import-star-use" => FlowBuiltinLint::InvalidImportStarUse,
    "invalid-this-arg" => FlowBuiltinLint::InvalidThisArg,
    "libdef-override" => FlowBuiltinLint::LibdefOverride,
    "mixed-import-and-require" => FlowBuiltinLint::MixedImportAndRequire,
    "nested-component" => FlowBuiltinLint::NestedComponent,
    "nested-hook" => FlowBuiltinLint::NestedHook,
    "non-const-var-export" => FlowBuiltinLint::NonConstVarExport,
    "nonstrict-import" => FlowBuiltinLint::NonstrictImport,
    "react-intrinsic-overlap" => FlowBuiltinLint::ReactIntrinsicOverlap,
    "require-explicit-enum-checks" => FlowBuiltinLint::RequireExplicitEnumChecks,
    "require-explicit-enum-switch-cases" => FlowBuiltinLint::RequireExplicitEnumSwitchCases,
    "sketchy-null" => FlowBuiltinLint::SketchyNull,
    "sketchy-null-bigint" => FlowBuiltinLint::SketchyNullBigInt,
    "sketchy-null-bool" => FlowBuiltinLint::SketchyNullBool,
    "sketchy-null-mixed" => FlowBuiltinLint::SketchyNullMixed,
    "sketchy-null-number" => FlowBuiltinLint::SketchyNullNumber,
    "sketchy-null-string" => FlowBuiltinLint::SketchyNullString,
    "sketchy-number" => FlowBuiltinLint::SketchyNumber,
    "sketchy-number-and" => FlowBuiltinLint::SketchyNumber,
    "this-in-exported-function" => FlowBuiltinLint::ThisInExportedFunction,
    "unclear-type" => FlowBuiltinLint::UnclearType,
    "uninitialized-instance-property" => FlowBuiltinLint::UninitializedInstanceProperty,
    "unnecessary-invariant" => FlowBuiltinLint::UnnecessaryInvariant,
    "unnecessary-optional-chain" => FlowBuiltinLint::UnnecessaryOptionalChain,
    "unsafe-getters-setters" => FlowBuiltinLint::UnsafeGettersSetters,
    "unsafe-object-assign" => FlowBuiltinLint::UnsafeObjectAssign,
    "untyped-import" => FlowBuiltinLint::UntypedImport,
    "untyped-type-import" => FlowBuiltinLint::UntypedTypeImport,
    "unused-promise" => FlowBuiltinLint::UnusedPromise,
};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_table_is_indexed_by_discriminant() {
        for (index, entry) in LINTS.iter().enumerate() {
            assert_eq!(
                entry.lint as usize, index,
                "{} is stored at the wrong index",
                entry.name
            );
        }
    }

    #[test]
    fn every_lint_is_reachable_from_all() {
        assert_eq!(FlowBuiltinLint::all().len(), FlowBuiltinLint::COUNT);
        assert_eq!(LINTS.len(), FlowBuiltinLint::COUNT);
    }

    #[test]
    fn rule_ids_are_the_namespaced_lint_names() {
        for lint in FlowBuiltinLint::all() {
            assert_eq!(
                lint.as_rule_id(),
                format!("{FLOW_NAMESPACE}{}", lint.as_name()),
                "{} has a mismatched rule id",
                lint.as_name()
            );
        }
    }

    #[test]
    fn lint_names_are_sorted_and_unique() {
        let names = LINTS.iter().map(|entry| entry.name).collect::<Vec<_>>();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn name_table_round_trips_through_the_perfect_hash() {
        for lint in FlowBuiltinLint::all() {
            assert_eq!(FlowBuiltinLint::from_lint_name(lint.as_name()), Some(lint));
            assert_eq!(FlowBuiltinLint::from_rule_id(lint.as_rule_id()), Some(lint));
        }
    }

    #[test]
    fn perfect_hash_only_adds_the_two_flow_long_spellings() {
        assert_eq!(BY_NAME.len(), FlowBuiltinLint::COUNT + 2);
        assert_eq!(
            FlowBuiltinLint::from_lint_name("sketchy-number-and"),
            Some(FlowBuiltinLint::SketchyNumber)
        );
        assert_eq!(
            FlowBuiltinLint::from_lint_name("deprecated-type-bool"),
            Some(FlowBuiltinLint::DeprecatedType)
        );
    }

    #[test]
    fn sketchy_null_is_the_only_umbrella() {
        let umbrellas = FlowBuiltinLint::all()
            .filter(|lint| lint.is_umbrella())
            .collect::<Vec<_>>();
        assert_eq!(umbrellas, vec![FlowBuiltinLint::SketchyNull]);
        assert_eq!(FlowBuiltinLint::SketchyNull.members().len(), 5);
    }

    #[test]
    fn umbrella_members_are_themselves_leaves() {
        for member in FlowBuiltinLint::SketchyNull.members() {
            assert!(!member.is_umbrella());
            assert!(member.as_name().starts_with("sketchy-null-"));
        }
    }

    #[test]
    fn from_str_accepts_bare_names_and_rule_ids() {
        assert_eq!(
            "unclear-type".parse::<FlowBuiltinLint>(),
            Ok(FlowBuiltinLint::UnclearType)
        );
        assert_eq!(
            "flow/unclear-type".parse::<FlowBuiltinLint>(),
            Ok(FlowBuiltinLint::UnclearType)
        );
    }

    #[test]
    fn from_str_rejects_names_flow_does_not_ship() {
        for name in [
            "implicit-inexact-object",
            "unused-promise-in-async-scope",
            "require-explicit-import-type",
            "deprecated-class-static-blocks",
            "",
            "flow/",
            "sketchy",
            "FLOW/UNCLEAR-TYPE",
        ] {
            assert_eq!(
                name.parse::<FlowBuiltinLint>(),
                Err(FlowLintParseError::UnknownLint {
                    name: CompactString::from(name),
                }),
                "{name} should not resolve"
            );
        }
    }

    #[test]
    fn parse_error_message_names_the_offending_spelling() {
        let error = "implicit-inexact-object"
            .parse::<FlowBuiltinLint>()
            .expect_err("unknown lint");
        assert_eq!(
            error.to_string(),
            "`implicit-inexact-object` is not a Flow built-in lint"
        );
    }
}
