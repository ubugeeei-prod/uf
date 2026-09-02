//! The rule catalogue: one descriptor per lint `uf lint` knows about.
//!
//! `uf lint` is the union of Flow's built-in lint set (see
//! [`crate::flow_builtin`]) and uf's own framework rules, so the catalogue is the
//! place `uf inspect` and the docs read to answer "what can this linter check?".

mod flow;
mod framework;

#[cfg(test)]
mod tests;

use std::sync::LazyLock;

use serde::Serialize;
use uf_config::RuleLevel;

use crate::flow_builtin::FlowBuiltinLint;
use flow::FLOW_META;
use framework::OWN_RULES;

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
