//! uf's policy for each of Flow's built-in lints.
//!
//! Flow ships every lint defaulted to `off`; the table here is where uf takes a
//! position instead, and the comment on each row is the argument for the level it
//! picked. Names and rule ids are not repeated -- they live in
//! [`crate::flow_builtin`] and are joined to this table by discriminant.

use uf_config::RuleLevel;

use crate::flow_builtin::FlowBuiltinLint;
use crate::rules::RuleRequirement;

/// Per-lint metadata for the Flow built-ins, indexed by discriminant.
///
/// Ids and names live in [`crate::flow_builtin`]; only the uf-specific policy
/// (default level, requirement, blurb) lives here.
pub(crate) struct FlowLintMeta {
    pub(crate) default_level: RuleLevel,
    pub(crate) requirement: RuleRequirement,
    pub(crate) description: &'static str,
}

const fn meta(
    default_level: RuleLevel,
    requirement: RuleRequirement,
    description: &'static str,
) -> FlowLintMeta {
    FlowLintMeta {
        default_level,
        requirement,
        description,
    }
}

use RuleRequirement::{SourceText, TypeChecker};

/// uf's deliberate policy for each Flow built-in lint.
///
/// Flow itself defaults every lint to `off`. uf is opinionated instead: a rule is
/// `error` when the pattern it catches is a bug or an unsound escape hatch in a
/// large Flow codebase, `warn` when the pattern is merely suspicious or when uf's
/// current check only covers a syntactic subset, and `off` only when leaving it on
/// would double-report. Rationale per lint is on each row.
///
/// The order must match [`FlowBuiltinLint`]'s discriminant order.
pub(crate) static FLOW_META: [FlowLintMeta; FlowBuiltinLint::COUNT] = [
    // ambiguous-object-type: off, because the ambiguity it is named after no
    // longer exists. The rule dates from when a `.flowconfig` could set
    // `exact_by_default=false` and `{ a: b }` meant different things in
    // different projects. Flow has defaulted to exact since 2023 and now
    // rejects `exact_by_default=false` as deprecated, so `{ a: b }` is exact,
    // full stop — and the `{| |}` this rule asks for is the legacy spelling of
    // what the plain braces already say. Left on, it reported 152 errors
    // against uf's own packages for writing modern Flow.
    //
    // Still selectable: a codebase migrating from an older Flow may want every
    // object type marked while both spellings are in the tree.
    meta(
        RuleLevel::Off,
        SourceText,
        "object type annotations must state exactness explicitly",
    ),
    // default-import-access: reading named exports off a default import is almost
    // always a CommonJS interop mistake.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "do not read named exports off a default import",
    ),
    // deprecated-type: `bool` is a legacy alias; one-line fix, no downside.
    meta(
        RuleLevel::Error,
        SourceText,
        "the `bool` type alias is deprecated; write `boolean`",
    ),
    // export-renamed-default: legal but confusing; warn rather than block.
    meta(
        RuleLevel::Warn,
        SourceText,
        "avoid `export { value as default }`; use an explicit default export",
    ),
    // internal-type: Flow's internal types are unstable and change between
    // releases, so depending on them makes upgrades break silently.
    meta(
        RuleLevel::Error,
        SourceText,
        "do not reference Flow's internal types directly",
    ),
    // invalid-import-star-use: namespace objects are not values; misuse is a bug.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "namespace imports may only be used for member access",
    ),
    // invalid-this-arg: rebinding a method to a foreign receiver is unsound.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "do not rebind a method to an incompatible receiver",
    ),
    // libdef-override: silently shadowing a builtin libdef breaks every consumer.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "library definitions must not override built-in declarations",
    ),
    // mixed-import-and-require: uf ships ESM only; mixing the two module systems
    // in one file defeats static analysis and tree shaking.
    meta(
        RuleLevel::Error,
        SourceText,
        "do not mix `import` and `require` in one module",
    ),
    // nested-component: a component declared inside another is a fresh type every
    // render, so React remounts its whole subtree.
    meta(
        RuleLevel::Error,
        SourceText,
        "do not declare a component inside another component or hook",
    ),
    // nested-hook: same remount/identity problem as nested components.
    meta(
        RuleLevel::Error,
        SourceText,
        "do not declare a hook inside another component or hook",
    ),
    // non-const-var-export: a mutable export is a live binding consumers cannot
    // reason about, and it blocks tree shaking.
    meta(
        RuleLevel::Error,
        SourceText,
        "exported bindings must be `const`",
    ),
    // nonstrict-import: only actionable once `@flow strict` adoption is underway,
    // so warn instead of blocking a migration.
    meta(
        RuleLevel::Warn,
        TypeChecker,
        "`@flow strict` modules may only import other strict modules",
    ),
    // react-intrinsic-overlap: needs type inference, and uf's syntactic version
    // was a misreading.
    //
    // uf flagged any `const`/`let`/`var` whose name matched an HTML element,
    // on the grounds that it "silently changes what JSX means". It does not:
    // a lowercase tag is *always* an intrinsic, resolved to the string, never
    // from scope — `const body = 42; <body />` still compiles to
    // `_jsx("body")`. The rule reported 90 errors against uf's own packages for
    // naming variables `source`, `table`, `text` and `slot`.
    //
    // Flow's rule of this name is a different check entirely: a `mixed`-typed
    // base used where a component is expected. That needs inference, so it
    // belongs with the rules uf reports as unavailable rather than with the
    // ones it pretends to run.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "a value used as a component must not overlap a JSX intrinsic",
    ),
    // require-explicit-enum-checks: warn, because the explicit form is verbose and
    // the implicit form is not itself a bug.
    meta(
        RuleLevel::Warn,
        TypeChecker,
        "compare Flow enum values explicitly instead of testing truthiness",
    ),
    // require-explicit-enum-switch-cases: same reasoning as above.
    meta(
        RuleLevel::Warn,
        TypeChecker,
        "list Flow enum switch cases explicitly instead of relying on `default`",
    ),
    // sketchy-null (umbrella): `if (count)` skipping `0` and `""` is the single
    // most common Flow-catchable production bug, so it is on by default. The five
    // typed variants below stay `off` so one violation is not reported twice.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "existence check on a value that may be both nullish and falsey",
    ),
    // sketchy-null-bigint: covered by the `sketchy-null` umbrella above.
    meta(
        RuleLevel::Off,
        TypeChecker,
        "existence check on a `?bigint` (covered by `flow/sketchy-null`)",
    ),
    // sketchy-null-bool: covered by the `sketchy-null` umbrella above.
    meta(
        RuleLevel::Off,
        TypeChecker,
        "existence check on a `?boolean` (covered by `flow/sketchy-null`)",
    ),
    // sketchy-null-mixed: covered by the `sketchy-null` umbrella above.
    meta(
        RuleLevel::Off,
        TypeChecker,
        "existence check on a `mixed` value (covered by `flow/sketchy-null`)",
    ),
    // sketchy-null-number: covered by the `sketchy-null` umbrella above.
    meta(
        RuleLevel::Off,
        TypeChecker,
        "existence check on a `?number` (covered by `flow/sketchy-null`)",
    ),
    // sketchy-null-string: covered by the `sketchy-null` umbrella above.
    meta(
        RuleLevel::Off,
        TypeChecker,
        "existence check on a `?string` (covered by `flow/sketchy-null`)",
    ),
    // sketchy-number: `{count && <List />}` renders a literal `0` in React. This
    // ships user-visible bugs, so it is an error.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "a number in a boolean position renders or branches on `0`",
    ),
    // this-in-exported-function: legal in methods, so warn rather than block.
    meta(
        RuleLevel::Warn,
        TypeChecker,
        "avoid `this` inside an exported standalone function",
    ),
    // unclear-type: `any`, `Object` and `Function` switch the checker off. This is
    // the rule formerly known as `flow/type-aware/no-explicit-any`.
    meta(
        RuleLevel::Error,
        SourceText,
        "avoid `any`, `Object`, and `Function` type annotations",
    ),
    // uninitialized-instance-property: reading a field before the constructor
    // finishes yields `undefined` at runtime with no type error.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "do not read an instance property before the constructor initializes it",
    ),
    // unnecessary-invariant: dead code, not a bug; warn.
    meta(
        RuleLevel::Warn,
        TypeChecker,
        "`invariant` on a condition already known to be truthy",
    ),
    // unnecessary-optional-chain: uf can only see the syntactic subset today (a
    // base that is never nullable), so warn rather than block.
    meta(
        RuleLevel::Warn,
        SourceText,
        "`?.` applied to a base that can never be nullish",
    ),
    // unsafe-getters-setters: accessors hide side effects behind property syntax,
    // but they are legitimate in some UI code, so warn.
    meta(
        RuleLevel::Warn,
        SourceText,
        "avoid getters and setters; they hide side effects behind property access",
    ),
    // unsafe-object-assign: `Object.assign` mutates its target and is unsound in
    // Flow; object spread is both safer and faster.
    meta(
        RuleLevel::Error,
        SourceText,
        "prefer object spread over `Object.assign`",
    ),
    // untyped-import: an untyped dependency turns every value it exports into
    // `any`, which quietly disables checking far from the import site.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "importing from an untyped module produces `any`",
    ),
    // untyped-type-import: same silent-`any` problem, in type position.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "importing a type from an untyped module produces an `any` alias",
    ),
    // unused-promise: a floating promise swallows rejections and loses ordering.
    meta(
        RuleLevel::Error,
        TypeChecker,
        "do not ignore a `Promise`; await it or handle its rejection",
    ),
];
