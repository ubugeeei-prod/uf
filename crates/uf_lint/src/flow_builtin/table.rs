//! The name and rule-id tables behind [`FlowBuiltinLint`].
//!
//! One row per lint, in the same order as the enum, so a lookup by variant is an
//! index rather than a search; plus a perfect hash for the reverse direction, so
//! resolving a name out of a config file or a suppression comment allocates
//! nothing and scans nothing.

use super::FlowBuiltinLint;

/// One row of the name/id table backing [`FlowBuiltinLint`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct LintEntry {
    pub(crate) lint: FlowBuiltinLint,
    pub(crate) name: &'static str,
    pub(crate) rule_id: &'static str,
    pub(crate) members: &'static [FlowBuiltinLint],
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
pub(crate) const LINTS: [LintEntry; FlowBuiltinLint::COUNT] = [
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
pub(crate) static BY_NAME: phf::Map<&'static str, FlowBuiltinLint> = phf::phf_map! {
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
