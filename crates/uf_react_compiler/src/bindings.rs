//! What the validator knows about each name in a module.
//!
//! Deliberately a flat table rather than a scope chain. What the checks need is
//! "is this name a prop", "is this name a ref", "has a hook seen this name" —
//! facts that a redeclaration resets, and that a scope chain would only make
//! more precise in cases a person does not write. Where that costs precision it
//! is documented at the check that pays for it, and the walk clears a
//! component's parameters when its body closes so two components cannot share
//! one another's props.

use compact_str::CompactString;
use uf_infra::FxHashMap;

use crate::error::MAX_TRACKED_BINDINGS;

/// What is known about one name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BindingFacts {
    /// The module declares this name somewhere.
    pub declared: bool,
    /// The declaration is at module scope, so writing to it is module state.
    pub module_scope: bool,
    /// A `component` parameter, or a name aliased from one.
    pub props: bool,
    /// Initialized from `useRef(...)`.
    pub ref_object: bool,
    /// Handed to a hook earlier in the module.
    pub passed_to_hook: bool,
}

/// Every name the validator is tracking.
#[derive(Debug, Default)]
pub struct Bindings {
    facts: FxHashMap<CompactString, BindingFacts>,
}

impl Bindings {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// What is known about `name`.
    pub fn get(&self, name: &str) -> BindingFacts {
        self.facts.get(name).copied().unwrap_or_default()
    }

    /// Declare `name`, discarding anything previously known about it.
    ///
    /// A redeclaration shadows: a `const items = []` inside one component must
    /// not inherit the "is a prop" fact from a component above it that happened
    /// to take a parameter of the same name.
    pub fn declare(&mut self, name: &str, module_scope: bool) {
        self.set(
            name,
            BindingFacts {
                declared: true,
                module_scope,
                ..BindingFacts::default()
            },
        );
    }

    /// Record that `name` is a prop, or an alias of one.
    pub fn mark_props(&mut self, name: &str) {
        self.update(name, |facts| {
            facts.declared = true;
            facts.props = true;
        });
    }

    /// Stop treating `name` as a prop, when the body that declared it closes.
    pub fn clear_props(&mut self, name: &str) {
        if let Some(facts) = self.facts.get_mut(name) {
            facts.props = false;
        }
    }

    /// Record that `name` holds the result of `useRef(...)`.
    pub fn mark_ref(&mut self, name: &str) {
        self.update(name, |facts| facts.ref_object = true);
    }

    /// Record that `name` was handed to a hook.
    pub fn mark_passed_to_hook(&mut self, name: &str) {
        self.update(name, |facts| facts.passed_to_hook = true);
    }

    /// How many names are tracked.
    pub fn len(&self) -> usize {
        self.facts.len()
    }

    /// Whether nothing is tracked.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    fn set(&mut self, name: &str, facts: BindingFacts) {
        if let Some(existing) = self.facts.get_mut(name) {
            *existing = facts;
        } else if self.facts.len() < MAX_TRACKED_BINDINGS {
            self.facts.insert(CompactString::new(name), facts);
        }
    }

    fn update(&mut self, name: &str, edit: impl FnOnce(&mut BindingFacts)) {
        if let Some(existing) = self.facts.get_mut(name) {
            edit(existing);
            return;
        }
        if self.facts.len() < MAX_TRACKED_BINDINGS {
            let mut facts = BindingFacts::default();
            edit(&mut facts);
            self.facts.insert(CompactString::new(name), facts);
        }
    }
}

/// Methods that write to the object they are called on.
///
/// Sorted for binary search. Calling one of these on a prop is the mutation a
/// component most often makes by accident, and it is the one an assignment
/// check alone would miss.
pub const MUTATING_METHODS: &[&str] = &[
    "add",
    "clear",
    "copyWithin",
    "delete",
    "fill",
    "pop",
    "push",
    "reverse",
    "set",
    "shift",
    "sort",
    "splice",
    "unshift",
];

/// Whether `name` is a method that writes to its receiver.
pub fn is_mutating_method(name: &str) -> bool {
    MUTATING_METHODS.binary_search(&name).is_ok()
}

/// Words that are never a reference to a value a hook was handed.
///
/// Sorted for binary search.
const NON_REFERENCES: &[&str] = &[
    "async",
    "await",
    "case",
    "catch",
    "class",
    "const",
    "default",
    "delete",
    "do",
    "else",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "yield",
];

/// Whether `name` could name a value at all.
pub fn is_reference(name: &str) -> bool {
    NON_REFERENCES.binary_search(&name).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mutating_method_table_is_sorted_for_binary_search() {
        assert!(MUTATING_METHODS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn the_keyword_table_is_sorted_for_binary_search() {
        assert!(NON_REFERENCES.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_redeclaration_forgets_that_a_name_was_a_prop() {
        let mut bindings = Bindings::new();
        bindings.mark_props("items");
        assert!(bindings.get("items").props);
        bindings.declare("items", false);
        assert!(!bindings.get("items").props);
    }

    #[test]
    fn an_unknown_name_is_known_to_be_nothing() {
        let bindings = Bindings::new();
        assert_eq!(bindings.get("nope"), BindingFacts::default());
        assert!(bindings.is_empty());
    }

    #[test]
    fn the_table_stops_growing_at_the_ceiling() {
        let mut bindings = Bindings::new();
        for index in 0..MAX_TRACKED_BINDINGS + 16 {
            bindings.declare(&format!("name{index}"), false);
        }
        assert_eq!(bindings.len(), MAX_TRACKED_BINDINGS);
    }

    #[test]
    fn a_mutating_method_is_recognized() {
        assert!(is_mutating_method("push"));
        assert!(is_mutating_method("sort"));
        assert!(!is_mutating_method("map"));
    }

    #[test]
    fn a_keyword_is_not_a_reference() {
        assert!(!is_reference("return"));
        assert!(!is_reference("true"));
        assert!(is_reference("items"));
    }
}
