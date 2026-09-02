//! The emitted stylesheet, and the order its rules go in.
//!
//! # Why the order is the whole design
//!
//! Every rule uf emits is one class selector, so every rule has identical
//! specificity and the cascade decides ties by document order alone. Emitting
//! in the order modules were discovered would therefore make `:hover` work or
//! not work depending on which file the bundler read first, and would make a
//! shorthand silently beat the longhand written next to it.
//!
//! So a rule's place in the sheet is a pure function of what the rule *is*:
//! [`RulePriority`] is [`PropertyRank`](crate::property::PropertyRank) plus
//! [`StyleCondition::weight`], and the class name breaks any remaining tie.
//! Nothing about where the rule came from takes part. Two machines that see the
//! same set of declarations in any order emit byte-identical CSS.

use std::collections::{BTreeMap, BTreeSet};

use compact_str::CompactString;
use serde::Serialize;

use crate::condition::StyleCondition;
use crate::property::PropertyRank;

/// Where one rule sits in the sheet. Smaller comes first.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RulePriority(pub u32);

impl RulePriority {
    /// Priority of a declaration of `property` in `condition`.
    pub fn of(property: &str, condition: &StyleCondition) -> Self {
        Self(PropertyRank::of(property).weight() + condition.weight())
    }

    /// The raw weight.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// One atomic rule: one class, one declaration, one state.
///
/// `Ord` is the sheet order, and it is a total order over the rule's own
/// content, so a `BTreeSet` of rules is both deduplicated and sorted with no
/// separate sorting pass and no dependence on insertion order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleRule {
    /// Where the rule sits in the sheet.
    pub priority: RulePriority,
    /// The generated class name, without its leading dot.
    pub class: CompactString,
    /// The CSS property.
    pub property: CompactString,
    /// The CSS value.
    pub value: CompactString,
    /// The state the rule applies in.
    pub condition: StyleCondition,
}

impl StyleRule {
    /// Build a rule and compute its priority.
    pub fn new(
        class: CompactString,
        property: CompactString,
        value: CompactString,
        condition: StyleCondition,
    ) -> Self {
        Self {
            priority: RulePriority::of(&property, &condition),
            class,
            property,
            value,
            condition,
        }
    }

    /// Append the rule's CSS text to `out`.
    pub fn write_css(&self, out: &mut String) {
        if let Some(at_rule) = self.condition.at_rule() {
            out.push_str(at_rule);
            out.push('{');
        }
        out.push('.');
        out.push_str(&self.class);
        out.push_str(self.condition.selector_suffix());
        out.push('{');
        out.push_str(&self.property);
        out.push(':');
        out.push_str(&self.value);
        out.push('}');
        if self.condition.at_rule().is_some() {
            out.push('}');
        }
    }
}

/// One CSS custom property declared by `stylex.defineVars`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableRule {
    /// The custom-property name, dashes included.
    pub name: CompactString,
    /// The CSS value.
    pub value: CompactString,
}

/// Two modules that declared the same variable with different values.
///
/// The variable name is derived from the binding a `defineVars` result is
/// declared under, so this is what a project gets for exporting `tokens` twice.
/// Reporting it beats letting one of the two win by sheet order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableConflict {
    /// The contested custom property.
    pub name: CompactString,
    /// Every value declared for it, sorted.
    pub values: Vec<CompactString>,
}

/// Every rule a build collected, ready to be written out.
///
/// Rules live in a `BTreeSet` and variables in a `BTreeMap`, so adding a module
/// twice changes nothing and adding modules in a different order changes
/// nothing either.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StyleSheet {
    rules: BTreeSet<StyleRule>,
    variables: BTreeMap<CompactString, BTreeSet<CompactString>>,
}

impl StyleSheet {
    /// An empty sheet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one rule. Adding the same rule again is a no-op.
    pub fn insert(&mut self, rule: StyleRule) {
        self.rules.insert(rule);
    }

    /// Add one variable declaration. Adding the same one again is a no-op.
    pub fn insert_variable(&mut self, name: CompactString, value: CompactString) {
        self.variables.entry(name).or_default().insert(value);
    }

    /// Fold another sheet into this one.
    pub fn extend(&mut self, other: &Self) {
        self.rules.extend(other.rules.iter().cloned());
        for (name, values) in &other.variables {
            self.variables
                .entry(name.clone())
                .or_default()
                .extend(values.iter().cloned());
        }
    }

    /// The rules, in sheet order.
    pub fn rules(&self) -> impl Iterator<Item = &StyleRule> {
        self.rules.iter()
    }

    /// How many rules the sheet holds.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether the sheet holds nothing at all.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.variables.is_empty()
    }

    /// The variable declarations, sorted by name.
    pub fn variables(&self) -> impl Iterator<Item = VariableRule> {
        self.variables.iter().flat_map(|(name, values)| {
            values.iter().map(move |value| VariableRule {
                name: name.clone(),
                value: value.clone(),
            })
        })
    }

    /// Variables declared with more than one value.
    pub fn variable_conflicts(&self) -> Vec<VariableConflict> {
        self.variables
            .iter()
            .filter(|(_, values)| values.len() > 1)
            .map(|(name, values)| VariableConflict {
                name: name.clone(),
                values: values.iter().cloned().collect(),
            })
            .collect()
    }

    /// The whole sheet as CSS text, one rule per line.
    ///
    /// Variables come first, because a custom property has to be declared for
    /// the `var()` calls in the rules below it to have anything to resolve in a
    /// browser that is still parsing the sheet.
    pub fn to_css(&self) -> String {
        let mut out = String::with_capacity(self.rules.len() * 48 + self.variables.len() * 32);
        if !self.variables.is_empty() {
            out.push_str(":root{");
            for variable in self.variables() {
                out.push_str(&variable.name);
                out.push(':');
                out.push_str(&variable.value);
                out.push(';');
            }
            out.push_str("}\n");
        }
        for rule in &self.rules {
            rule.write_css(&mut out);
            out.push('\n');
        }
        out
    }
}

impl FromIterator<StyleRule> for StyleSheet {
    fn from_iter<I: IntoIterator<Item = StyleRule>>(iter: I) -> Self {
        Self {
            rules: iter.into_iter().collect(),
            variables: BTreeMap::new(),
        }
    }
}
