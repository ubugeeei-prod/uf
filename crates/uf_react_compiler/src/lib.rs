#![deny(missing_docs)]
//! React Compiler syntax mode for uniflowed.
//!
//! `reactCompiler: { mode: "syntax" }` means uf does **not** run the memoizing
//! transform. It runs the half of the compiler that catches bugs: the checks
//! that decide whether a component *could* have been compiled. A component that
//! passes them is one whose renders are repeatable, whose hooks are called in
//! the same order every time, and whose values do not change behind React's
//! back — which is what makes memoization sound in the first place, and what
//! makes the component correct in Strict Mode whether or not it is memoized.
//!
//! # What is checked
//!
//! | Rule | What it decides |
//! | --- | --- |
//! | [`react/hooks-rules`](ReactCompilerRule::HooksRules) | hooks are called unconditionally, at the top level of a `component`, a `hook`, or a `useX` function — not in a condition, a loop, a callback, or after an early return |
//! | [`react/no-props-mutation`](ReactCompilerRule::NoPropsMutation) | neither props nor a value aliased from them is written to |
//! | [`react/no-mutation-after-hook`](ReactCompilerRule::NoMutationAfterHook) | a value handed to a hook is not written to afterwards |
//! | [`react/no-ref-read-in-render`](ReactCompilerRule::NoRefReadInRender) | `ref.current` is not read while rendering |
//! | [`react/no-render-side-effects`](ReactCompilerRule::NoRenderSideEffects) | render writes no module state, logs nothing, and reaches for neither the DOM nor a value that changes on its own |
//!
//! # What is deliberately not checked
//!
//! Every check above is decidable from source text alone. Three neighbouring
//! checks are not, and are left out rather than approximated:
//!
//! * **Whether a plain function is a component.** A `component` declaration says
//!   so in the syntax; `function Card(props)` does not, and calling every
//!   capitalized function a component would attach props rules to ordinary
//!   code. Only `component` and `hook` declarations and the `useX` naming
//!   convention are treated as React.
//! * **Whether a closure runs during render.** `useEffect(() => …)` and
//!   `onClick={() => …}` are the same syntax; which one runs during render
//!   depends on what the closure is passed to. So the render checks report only
//!   code that is certainly in render, and say nothing about closures.
//! * **Whether a value derived from props is a copy.** `const a = props.a`
//!   aliases; `const a = {...props.a}` copies, and only the first may not be
//!   written to. uf follows aliases through member chains, where the shape
//!   settles it, and treats anything that constructs a value as fresh.
//!
//! # One rule, one home
//!
//! `react/hooks-rules` and `react/no-render-side-effects` are also rules
//! `uf lint` reports. The predicate for both lives *here*, and `uf_lint`
//! delegates to [`validate`]: the linter maps a [`ReactDiagnostic`] onto its own
//! severity and suppression handling and reports it, so the two tools can never
//! disagree about whether a component is compilable.
//!
//! ```
//! use uf_react_compiler::{Finding, validate};
//!
//! let diagnostics = validate(
//!     "component Page(items: Array<string>) {\n\
//!      \x20 if (items.length) { const [n] = useState(0); }\n\
//!      \x20 items.push(\"x\");\n\
//!      \x20 return null;\n\
//!      }\n",
//! )
//! .expect("a module that validates");
//!
//! let findings: Vec<Finding> = diagnostics.iter().map(|entry| entry.finding).collect();
//! assert_eq!(findings, [Finding::HookNotAtTopLevel, Finding::PropsMutated]);
//! ```

pub mod analyze;
pub mod bindings;
pub mod error;
pub mod official;
pub mod plugin;
pub mod rule;
pub mod scope;
pub mod syntax;

pub use crate::analyze::validate;
pub use crate::bindings::{BindingFacts, Bindings, MUTATING_METHODS};
pub use crate::error::{
    MAX_DIAGNOSTICS, MAX_SCOPE_DEPTH, MAX_SOURCE_BYTES, MAX_TRACKED_BINDINGS, ReactCompilerError,
};
pub use crate::official::{
    OfficialCompileOutput, OfficialReactCompilerCrate, OfficialReactCompilerError,
    compile_babel_ast, compile_babel_ast_json, official_compiler_crate,
};
pub use crate::plugin::{FindingsSink, ModuleFindings, OnFinding, plugin};
pub use crate::rule::{Finding, ReactCompilerRule, ReactDiagnostic};
pub use crate::scope::{ScopeKind, is_hook_name};

#[cfg(test)]
mod tests;
