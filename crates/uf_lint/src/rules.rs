//! The rule catalogue: one descriptor per lint `uf lint` knows about.
//!
//! `uf lint` is the union of Flow's built-in lint set (see
//! [`crate::flow_builtin`]) and uf's own framework rules, so the catalogue is the
//! place `uf inspect` and the docs read to answer "what can this linter check?".

use std::sync::LazyLock;

use serde::Serialize;
use uf_config::RuleLevel;

use crate::flow_builtin::FlowBuiltinLint;

/// Which part of the toolchain a rule belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleCategory {
    /// Flow's own built-in lints plus Flow parse diagnostics.
    Flow,
    /// Toolchain-wide hygiene rules.
    Uniflowed,
    /// React and Flow component/hook syntax rules.
    React,
    /// React Native specific rules.
    ReactNative,
    /// Server component, server action, and client/server boundary rules.
    Server,
    /// File-system router rules.
    Router,
    /// `package.json` rules.
    Package,
    /// `@uniflowed/fetch` rules.
    Fetch,
    /// Rules that exist to keep a known vulnerability class out of the codebase.
    Security,
}

/// What a rule needs in order to produce an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RuleRequirement {
    /// Decidable from the source text alone; the rule runs today.
    SourceText,
    /// Needs type inference. uf has no type checker yet, so an enabled rule of
    /// this kind is reported through [`crate::LintReport::unavailable`] rather
    /// than silently passing.
    TypeChecker,
}

impl RuleRequirement {
    /// Whether `uf lint` can evaluate rules with this requirement today.
    #[inline]
    pub fn is_available(self) -> bool {
        matches!(self, Self::SourceText)
    }
}

/// Everything `uf inspect` and the docs need to describe one rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDescriptor {
    /// Stable rule id, e.g. `"flow/unclear-type"`.
    pub id: &'static str,
    /// Which part of the toolchain owns the rule.
    pub category: RuleCategory,
    /// Level applied by [`uf_config::LintConfig::default`].
    pub default_level: RuleLevel,
    /// What the rule needs in order to run.
    pub requirement: RuleRequirement,
    /// One-line summary, suitable for `uf inspect` output.
    pub description: &'static str,
}

/// Rule ids that older configs may still use, mapped to their replacement.
///
/// `flow/type-aware/no-explicit-any` was uf's hand-rolled `any` check before the
/// Flow built-in set landed; Flow's own name for it is `unclear-type`.
static DEPRECATED_ALIASES: phf::Map<&'static str, &'static str> = phf::phf_map! {
    "flow/type-aware/no-explicit-any" => "flow/unclear-type",
};

/// Per-lint metadata for the Flow built-ins, indexed by discriminant.
///
/// Ids and names live in [`crate::flow_builtin`]; only the uf-specific policy
/// (default level, requirement, blurb) lives here.
struct FlowLintMeta {
    default_level: RuleLevel,
    requirement: RuleRequirement,
    description: &'static str,
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
static FLOW_META: [FlowLintMeta; FlowBuiltinLint::COUNT] = [
    // ambiguous-object-type: a large codebase cannot afford object types whose
    // exactness depends on a config flag; readers must see `{| |}` or `{ ... }`.
    meta(
        RuleLevel::Error,
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
    // react-intrinsic-overlap: shadowing `div`/`span` silently changes what JSX
    // means at the use site.
    meta(
        RuleLevel::Error,
        SourceText,
        "local bindings must not shadow a JSX intrinsic element name",
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

/// uf's own rules, on top of the Flow built-in set.
static OWN_RULES: &[RuleDescriptor] = &[
    RuleDescriptor {
        id: "flow/syntax",
        category: RuleCategory::Flow,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "the file must parse with the official Flow parser",
    },
    RuleDescriptor {
        id: "uniflowed/no-tabs",
        category: RuleCategory::Uniflowed,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "indent with spaces, never tabs",
    },
    RuleDescriptor {
        id: "uniflowed/no-trailing-whitespace",
        category: RuleCategory::Uniflowed,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "lines must not end in whitespace",
    },
    RuleDescriptor {
        id: "uniflowed/no-npm-script-invocation",
        category: RuleCategory::Uniflowed,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "shell out to uf tasks, not `npm run`/`yarn`/`pnpm`/`bunx`",
    },
    RuleDescriptor {
        id: "uniflowed/unknown-lint-suppression",
        category: RuleCategory::Uniflowed,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "`uf-lint-disable` comments must name a rule this linter knows",
    },
    RuleDescriptor {
        id: "react/component-syntax",
        category: RuleCategory::React,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "declare React components with Flow `component` syntax",
    },
    RuleDescriptor {
        id: "react/hook-syntax",
        category: RuleCategory::React,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "declare React hooks with Flow `hook` syntax",
    },
    RuleDescriptor {
        id: "react/hooks-rules",
        category: RuleCategory::React,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "call hooks only at the top level of a component, hook, or `useX` function",
    },
    // `warn`, not `error`, for the same reason as `react/component-syntax`: this
    // is a convention the ecosystem (and uf's own `uf create app` scaffold) is
    // still migrating to, and a linter must not fail a freshly created project.
    // It becomes an error once the scaffold ships named exports.
    RuleDescriptor {
        id: "react/no-default-export-component",
        category: RuleCategory::React,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "modules that declare components must use named exports",
    },
    RuleDescriptor {
        id: "react/no-render-side-effects",
        category: RuleCategory::React,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "keep render idempotent; no clocks, randomness, or storage reads",
    },
    RuleDescriptor {
        id: "react-native/platform-split",
        category: RuleCategory::ReactNative,
        default_level: RuleLevel::Warn,
        requirement: SourceText,
        description: "prefer platform-specific files over `Platform.OS` branches",
    },
    RuleDescriptor {
        id: "server/no-client-secret",
        category: RuleCategory::Server,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "client modules must not read server secrets",
    },
    RuleDescriptor {
        id: "server/no-server-only-import-in-client",
        category: RuleCategory::Server,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "client modules must not import server-only modules",
    },
    RuleDescriptor {
        id: "server/use-client-directive-position",
        category: RuleCategory::Server,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "`use client`/`use server` must be the module's first statement",
    },
    RuleDescriptor {
        id: "server/use-server-actions",
        category: RuleCategory::Server,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "server action modules must open with `\"use server\";`",
    },
    RuleDescriptor {
        id: "router/reserved-files",
        category: RuleCategory::Router,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "`_uf.*` file names are reserved for layout, page, and middleware",
    },
    RuleDescriptor {
        id: "package/no-npm-scripts",
        category: RuleCategory::Package,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "declare tasks in `uf.config.js`, not `package.json` scripts",
    },
    RuleDescriptor {
        id: "fetch/no-global-override",
        category: RuleCategory::Fetch,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "do not monkey-patch global `fetch`",
    },
    RuleDescriptor {
        id: "security/no-dangerously-set-inner-html",
        category: RuleCategory::Security,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "render HTML only through a sanitizing `@uniflowed/markdown` helper",
    },
    RuleDescriptor {
        id: "security/no-eval",
        category: RuleCategory::Security,
        default_level: RuleLevel::Error,
        requirement: SourceText,
        description: "never turn strings into code via `eval`, `new Function`, or timer strings",
    },
];

/// Every rule, sorted by id so [`rule`] can binary search.
static ALL_RULES: LazyLock<Box<[RuleDescriptor]>> = LazyLock::new(|| {
    let mut all = Vec::with_capacity(FlowBuiltinLint::COUNT + OWN_RULES.len());
    for lint in FlowBuiltinLint::all() {
        let meta = &FLOW_META[lint as usize];
        all.push(RuleDescriptor {
            id: lint.as_rule_id(),
            category: RuleCategory::Flow,
            default_level: meta.default_level,
            requirement: meta.requirement,
            description: meta.description,
        });
    }
    all.extend_from_slice(OWN_RULES);
    all.sort_unstable_by_key(|descriptor| descriptor.id);
    all.into_boxed_slice()
});

/// Every rule `uf lint` knows about, sorted by rule id.
///
/// This is what `uf inspect` and the documentation enumerate.
pub fn rules() -> &'static [RuleDescriptor] {
    &ALL_RULES
}

/// Look up one rule by its canonical id.
///
/// Deprecated aliases are **not** resolved here; call [`canonical_rule_id`] first
/// if the id may come from a user-written config or suppression comment.
pub fn rule(id: &str) -> Option<&'static RuleDescriptor> {
    rules()
        .binary_search_by_key(&id, |descriptor| descriptor.id)
        .ok()
        .map(|index| &rules()[index])
}

/// Resolve a user-written rule id to its canonical spelling.
///
/// Returns `None` when the id names no rule at all, which is what
/// `uniflowed/unknown-lint-suppression` reports on.
pub fn canonical_rule_id(id: &str) -> Option<&'static str> {
    if let Some(canonical) = DEPRECATED_ALIASES.get(id) {
        return Some(canonical);
    }
    rule(id).map(|descriptor| descriptor.id)
}

/// Iterate the deprecated aliases pointing at `canonical`.
pub(crate) fn deprecated_aliases_for(canonical: &str) -> impl Iterator<Item = &'static str> {
    DEPRECATED_ALIASES
        .entries()
        .filter(move |(_, target)| **target == canonical)
        .map(|(alias, _)| *alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_covers_flow_builtins_and_uf_rules() {
        assert_eq!(rules().len(), FlowBuiltinLint::COUNT + OWN_RULES.len());
    }

    #[test]
    fn catalogue_is_sorted_and_free_of_duplicate_ids() {
        let ids = rules()
            .iter()
            .map(|descriptor| descriptor.id)
            .collect::<Vec<_>>();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn every_flow_builtin_lint_has_a_descriptor() {
        for lint in FlowBuiltinLint::all() {
            let descriptor = rule(lint.as_rule_id())
                .unwrap_or_else(|| panic!("{} has no descriptor", lint.as_rule_id()));
            assert_eq!(descriptor.category, RuleCategory::Flow);
        }
    }

    #[test]
    fn every_flow_namespaced_descriptor_is_a_builtin_or_flow_syntax() {
        for descriptor in rules() {
            let Some(name) = descriptor.id.strip_prefix("flow/") else {
                continue;
            };
            if name == "syntax" {
                continue;
            }
            assert!(
                FlowBuiltinLint::from_rule_id(descriptor.id).is_some(),
                "{} is not a Flow built-in lint",
                descriptor.id
            );
        }
    }

    #[test]
    fn descriptions_are_one_line_and_non_empty() {
        for descriptor in rules() {
            assert!(!descriptor.description.is_empty(), "{}", descriptor.id);
            assert!(!descriptor.description.contains('\n'), "{}", descriptor.id);
        }
    }

    #[test]
    fn deprecated_alias_points_at_flow_unclear_type() {
        assert_eq!(
            canonical_rule_id("flow/type-aware/no-explicit-any"),
            Some(FlowBuiltinLint::UnclearType.as_rule_id())
        );
        assert_eq!(
            deprecated_aliases_for(FlowBuiltinLint::UnclearType.as_rule_id()).collect::<Vec<_>>(),
            vec!["flow/type-aware/no-explicit-any"]
        );
    }

    #[test]
    fn every_deprecated_alias_resolves_to_a_real_rule() {
        for (alias, target) in DEPRECATED_ALIASES.entries() {
            assert!(rule(target).is_some(), "{alias} points at unknown {target}");
            assert!(rule(alias).is_none(), "{alias} must not be a rule itself");
        }
    }

    #[test]
    fn canonical_rule_id_rejects_unknown_ids() {
        assert_eq!(canonical_rule_id("flow/does-not-exist"), None);
        assert_eq!(canonical_rule_id(""), None);
    }

    #[test]
    fn type_checker_rules_are_reported_as_unavailable() {
        assert!(!RuleRequirement::TypeChecker.is_available());
        assert!(RuleRequirement::SourceText.is_available());
    }

    #[test]
    fn sketchy_null_variants_default_off_so_violations_report_once() {
        for member in FlowBuiltinLint::SketchyNull.members() {
            let descriptor = rule(member.as_rule_id()).expect("descriptor");
            assert_eq!(descriptor.default_level, RuleLevel::Off);
        }
        let umbrella = rule(FlowBuiltinLint::SketchyNull.as_rule_id()).expect("descriptor");
        assert_eq!(umbrella.default_level, RuleLevel::Error);
    }

    #[test]
    fn security_rules_are_errors_by_default() {
        for descriptor in rules() {
            if descriptor.category == RuleCategory::Security {
                assert_eq!(
                    descriptor.default_level,
                    RuleLevel::Error,
                    "{}",
                    descriptor.id
                );
            }
        }
    }
}
