#![deny(missing_docs)]
//! Compile-time StyleX for uniflowed.
//!
//! A `*.stylex.js` module declares styles, and a component calls
//! `stylex.create({...})` to name them. This crate turns both into three
//! things, at build time, so that nothing about styling is computed in a
//! browser:
//!
//! * an **atomic stylesheet** — one class per `(namespace, property, value,
//!   state)` — emitted in a deterministic, specificity-correct order;
//! * a **rewritten module**, where the `stylex.create` call has become a plain
//!   object literal of class names;
//! * CSS **custom properties** for every `stylex.defineVars` entry, with
//!   `tokens.canvas` resolved to the `var(--…)` that names it.
//!
//! # The two properties everything else rests on
//!
//! **Determinism.** Class names come from SHA-256 over length-framed
//! components, never from a `Hasher` whose output is explicitly not stable
//! across Rust releases — see [`class`]. The same declaration produces the same
//! class on every machine and every run, so a build is reproducible and a CDN
//! cache entry stays valid.
//!
//! **Order.** Every emitted rule is one class selector, so all of them have
//! identical specificity and the cascade breaks ties by document order alone.
//! [`sheet`] therefore places a rule by what it *is* — how broadly the property
//! writes, and what state it applies in — and never by which module it came
//! from. Emitting in discovery order instead would make a `:hover` rule work or
//! not work depending on which file the bundler happened to read first.
//!
//! ```
//! use uf_stylex::{compile_module, props_of};
//!
//! let module = compile_module(
//!     "import { stylex } from \"@uniflowed/stylex\";\n\
//!      const styles = stylex.create({\n\
//!        base: { color: \"black\", marginTop: 4 },\n\
//!        loud: { color: \"red\" },\n\
//!      });\n",
//! )
//! .expect("a module that compiles");
//!
//! // The call is gone; only class names are left for the runtime.
//! assert!(!module.code.contains("stylex.create"));
//!
//! // `props` merges by property, last argument wins.
//! let merged = props_of(&module.styles);
//! assert_eq!(merged.len(), 2, "one surviving colour, one margin");
//! ```
//!
//! # Bounds
//!
//! A dependency in `node_modules` can ship a hostile `.stylex.js`, so every
//! limit is explicit and lives in [`error`]: a file-size ceiling, an object
//! nesting cap that removes recursion from the extractor entirely, a value
//! length cap, a declaration count cap, and a refusal of any value or selector
//! that could close its own CSS rule or the `<style>` element around it.

pub mod class;
pub mod compile;
pub mod condition;
pub mod error;
pub mod parse;
pub mod plugin;
pub mod property;
pub mod props;
pub mod sheet;
pub mod value;

pub use crate::class::{CLASS_PREFIX, NAME_DIGITS, VARIABLE_PREFIX, class_name, variable_name};
pub use crate::compile::{
    COMPILED_MARKER, CompiledModule, CompiledProperty, CompiledStyle, ConditionalClass,
    compile_module,
};
pub use crate::condition::StyleCondition;
pub use crate::error::{
    MAX_DECLARATIONS, MAX_OBJECT_DEPTH, MAX_SOURCE_BYTES, MAX_VALUE_BYTES, SourcePosition,
    StyleXError,
};
pub use crate::parse::bindings::{STYLEX_PACKAGE, VARIABLES_SUFFIX};
pub use crate::parse::{
    CreateCall, Declaration, DefineVarsCall, Namespace, ParsedModule, Variable, parse_module,
};
pub use crate::plugin::{FORBIDDEN_KEY_RULE, SheetSink, UNSAFE_VALUE_RULE, plugin};
pub use crate::property::PropertyRank;
pub use crate::props::{MergedProps, props, props_of};
pub use crate::sheet::{RulePriority, StyleRule, StyleSheet, VariableConflict, VariableRule};
pub use crate::value::StyleValue;

#[cfg(test)]
mod tests;
