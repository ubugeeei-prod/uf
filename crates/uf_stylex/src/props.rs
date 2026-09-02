//! What `stylex.props(...)` does, modelled at compile time.
//!
//! `props` is the one piece of StyleX that stays at runtime, because its
//! arguments are usually conditional (`active && styles.on`) and a compiler
//! cannot fold them. What it does is small enough to state exactly:
//!
//! * arguments are merged left to right, and the **property** is the unit of
//!   merging — a later namespace that sets `color` replaces everything the
//!   earlier one said about `color`, its `:hover` value included. That is the
//!   same thing a later `color:` in a CSS rule does, and it is why a later
//!   namespace cannot leave a stray hover state behind;
//! * the surviving classes are emitted in sheet order, so the class attribute
//!   is a pure function of the merge and never of argument order.
//!
//! Modelling it here is what lets the ordering be tested: a merge that keeps
//! two classes for the same property would be a bug you could only see in a
//! browser.

use compact_str::CompactString;

use crate::compile::{CompiledProperty, CompiledStyle};

/// The result of merging namespaces, ready to become a `class` attribute.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergedProps {
    classes: Vec<CompactString>,
}

impl MergedProps {
    /// The surviving classes, in sheet order.
    pub fn classes(&self) -> &[CompactString] {
        &self.classes
    }

    /// How many classes survived.
    pub fn len(&self) -> usize {
        self.classes.len()
    }

    /// Whether nothing survived.
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    /// The space-separated `class` attribute.
    pub fn class_name(&self) -> String {
        let mut out = String::with_capacity(self.classes.len() * 15);
        for (index, class) in self.classes.iter().enumerate() {
            if index > 0 {
                out.push(' ');
            }
            out.push_str(class);
        }
        out
    }
}

/// Merge namespaces the way `stylex.props` does.
///
/// The `None` entries are the falsy arguments a call site writes as
/// `condition && styles.on`; they take part in the order but contribute
/// nothing, exactly as they do at runtime.
pub fn props<'a>(styles: impl IntoIterator<Item = Option<&'a CompiledStyle>>) -> MergedProps {
    // Held in argument order, one entry per property key, replaced in place so
    // a later namespace wins without disturbing the keys around it.
    let mut winners: Vec<(&CompactString, &CompiledProperty)> = Vec::new();
    for style in styles.into_iter().flatten() {
        for property in &style.properties {
            match winners.iter().position(|(key, _)| **key == property.key) {
                Some(at) => winners[at] = (&property.key, property),
                None => winners.push((&property.key, property)),
            }
        }
    }

    let mut classes: Vec<(u32, CompactString)> = Vec::new();
    for (_, property) in winners {
        for entry in &property.classes {
            classes.push((entry.priority.get(), entry.class.clone()));
        }
    }
    classes.sort();
    classes.dedup();

    MergedProps {
        classes: classes.into_iter().map(|(_, class)| class).collect(),
    }
}

/// Merge namespaces that are all present.
pub fn props_of<'a>(styles: impl IntoIterator<Item = &'a CompiledStyle>) -> MergedProps {
    props(styles.into_iter().map(Some))
}
