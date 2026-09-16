//! `react-compiler/*`: the official React Compiler's diagnostics, one rule per
//! category.
//!
//! Nothing in this module decides whether code breaks a rule of React.
//! [`uf_transform::lint`] runs the official compiler, `react_compiler`, with the
//! options `eslint-plugin-react-hooks` runs it with and over the modules that
//! plugin hands it, and returns what the compiler reported. This module files
//! each diagnostic under the rule named for its category, at the level the
//! project set, with the compiler's message. [`super::react_tree`] runs it on
//! the tree [`super::module_tree`] parsed, so the module is read once for every
//! rule that needs it.
//!
//! A rule's name is the compiler's own name for its category — the name
//! `eslint-plugin-react-hooks` exposes that category under — so
//! `react-hooks/purity` in an ESLint config is `react-compiler/purity` here.

use uf_config::UniflowedConfig;
use uf_transform::ErrorCategory;

use crate::{Severity, severity};

/// `CapitalizedCalls`: a capitalized function called rather than rendered.
pub(crate) const CAPITALIZED_CALLS: &str = "react-compiler/capitalized-calls";
/// `ErrorBoundaries`: JSX built inside `try`.
pub(crate) const ERROR_BOUNDARIES: &str = "react-compiler/error-boundaries";
/// `EffectExhaustiveDependencies`: an effect's dependency array.
pub(crate) const EXHAUSTIVE_EFFECT_DEPENDENCIES: &str =
    "react-compiler/exhaustive-effect-dependencies";
/// `FBT`: Meta's `fbt` internationalization library.
pub(crate) const FBT: &str = "react-compiler/fbt";
/// `Globals`: a variable from outside the component or hook changed in render.
pub(crate) const GLOBALS: &str = "react-compiler/globals";
/// `Hooks`: the Rules of Hooks.
pub(crate) const HOOKS: &str = "react-compiler/hooks";
/// `Immutability`: a value React treats as immutable, mutated.
pub(crate) const IMMUTABILITY: &str = "react-compiler/immutability";
/// `IncompatibleLibrary`: an API known to break memoization.
pub(crate) const INCOMPATIBLE_LIBRARY: &str = "react-compiler/incompatible-library";
/// `Invariant`: an internal assumption of the compiler that did not hold.
pub(crate) const INVARIANT: &str = "react-compiler/invariant";
/// `MemoDependencies`: a `useMemo` or `useCallback` dependency array.
pub(crate) const MEMO_DEPENDENCIES: &str = "react-compiler/memo-dependencies";
/// `EffectDerivationsOfState`: state derived in an effect.
pub(crate) const NO_DERIVING_STATE_IN_EFFECTS: &str = "react-compiler/no-deriving-state-in-effects";
/// `PreserveManualMemo`: memoization the compiler cannot preserve.
pub(crate) const PRESERVE_MANUAL_MEMOIZATION: &str = "react-compiler/preserve-manual-memoization";
/// `Purity`: a known-impure function called during render.
pub(crate) const PURITY: &str = "react-compiler/purity";
/// `Refs`: a ref read or written during render.
pub(crate) const REFS: &str = "react-compiler/refs";
/// `EffectSetState`: `setState` called synchronously in an effect.
pub(crate) const SET_STATE_IN_EFFECT: &str = "react-compiler/set-state-in-effect";
/// `RenderSetState`: `setState` called during render.
pub(crate) const SET_STATE_IN_RENDER: &str = "react-compiler/set-state-in-render";
/// `StaticComponents`: a component created during render.
pub(crate) const STATIC_COMPONENTS: &str = "react-compiler/static-components";
/// `Syntax`: code the compiler rejects as invalid syntax.
pub(crate) const SYNTAX: &str = "react-compiler/syntax";
/// `Todo`: code the compiler does not compile yet.
pub(crate) const TODO: &str = "react-compiler/todo";
/// `UnsupportedSyntax`: syntax the compiler does not support.
pub(crate) const UNSUPPORTED_SYNTAX: &str = "react-compiler/unsupported-syntax";
/// `UseMemo`: a `useMemo` callback the hook cannot use.
pub(crate) const USE_MEMO: &str = "react-compiler/use-memo";
/// `VoidUseMemo`: a `useMemo` callback that returns nothing.
pub(crate) const VOID_USE_MEMO: &str = "react-compiler/void-use-memo";

/// Every `react-compiler/*` rule.
const RULES: [&str; 22] = [
    CAPITALIZED_CALLS,
    ERROR_BOUNDARIES,
    EXHAUSTIVE_EFFECT_DEPENDENCIES,
    FBT,
    GLOBALS,
    HOOKS,
    IMMUTABILITY,
    INCOMPATIBLE_LIBRARY,
    INVARIANT,
    MEMO_DEPENDENCIES,
    NO_DERIVING_STATE_IN_EFFECTS,
    PRESERVE_MANUAL_MEMOIZATION,
    PURITY,
    REFS,
    SET_STATE_IN_EFFECT,
    SET_STATE_IN_RENDER,
    STATIC_COMPONENTS,
    SYNTAX,
    TODO,
    UNSUPPORTED_SYNTAX,
    USE_MEMO,
    VOID_USE_MEMO,
];

/// The rule a category is filed under, or `None` for a category no module can
/// produce under the options [`uf_transform::lint`] runs the compiler with.
///
/// A `match` with no wildcard, so that a category a newer compiler adds does
/// not compile until somebody decides where it goes.
pub(super) const fn rule_for(category: ErrorCategory) -> Option<&'static str> {
    match category {
        ErrorCategory::CapitalizedCalls => Some(CAPITALIZED_CALLS),
        ErrorCategory::EffectDerivationsOfState => Some(NO_DERIVING_STATE_IN_EFFECTS),
        ErrorCategory::EffectExhaustiveDependencies => Some(EXHAUSTIVE_EFFECT_DEPENDENCIES),
        ErrorCategory::EffectSetState => Some(SET_STATE_IN_EFFECT),
        ErrorCategory::ErrorBoundaries => Some(ERROR_BOUNDARIES),
        ErrorCategory::FBT => Some(FBT),
        ErrorCategory::Globals => Some(GLOBALS),
        ErrorCategory::Hooks => Some(HOOKS),
        ErrorCategory::Immutability => Some(IMMUTABILITY),
        ErrorCategory::IncompatibleLibrary => Some(INCOMPATIBLE_LIBRARY),
        ErrorCategory::Invariant => Some(INVARIANT),
        ErrorCategory::MemoDependencies => Some(MEMO_DEPENDENCIES),
        ErrorCategory::PreserveManualMemo => Some(PRESERVE_MANUAL_MEMOIZATION),
        ErrorCategory::Purity => Some(PURITY),
        ErrorCategory::Refs => Some(REFS),
        ErrorCategory::RenderSetState => Some(SET_STATE_IN_RENDER),
        ErrorCategory::StaticComponents => Some(STATIC_COMPONENTS),
        ErrorCategory::Syntax => Some(SYNTAX),
        ErrorCategory::Todo => Some(TODO),
        ErrorCategory::UnsupportedSyntax => Some(UNSUPPORTED_SYNTAX),
        ErrorCategory::UseMemo => Some(USE_MEMO),
        ErrorCategory::VoidUseMemo => Some(VOID_USE_MEMO),
        // No validation in this compiler reports it.
        ErrorCategory::EffectDependencies => None,
        // Reported only for a malformed `use memo if(…)` directive, which the
        // compiler reads only when it is given `dynamicGating`. Neither the
        // plugin nor uf gives it that.
        ErrorCategory::Gating => None,
        // Reported only for a module type configuration that contradicts
        // itself. The compiler's own table is the only one it has: neither the
        // plugin nor uf supplies `moduleTypeProvider`.
        ErrorCategory::Config => None,
        // Reported only for a suppression comment the compiler looks for
        // itself. It looks for ESLint's only while hook usage or exhaustive
        // memoization dependencies go unvalidated, and both are validated; it
        // looks for Flow's only under `flowSuppressions`, which is `false`.
        ErrorCategory::Suppression => None,
    }
}

/// The level each `react-compiler/*` rule runs at in this project.
pub(super) struct CompilerWork {
    levels: [(&'static str, Option<Severity>); RULES.len()],
}

impl CompilerWork {
    /// The level `rule` runs at, or `None` when it is off.
    pub(super) fn level(&self, rule: &str) -> Option<Severity> {
        self.levels
            .iter()
            .find(|(id, _)| *id == rule)
            .and_then(|(_, level)| *level)
    }

    /// Whether the compiler has to be asked to check effect dependency
    /// arrays: `eslint-plugin-react-hooks` ships that validation switched off
    /// and switches it on through its rule options, and this rule is uf's
    /// spelling of those options.
    pub(super) fn checks_effect_dependencies(&self) -> bool {
        self.level(EXHAUSTIVE_EFFECT_DEPENDENCIES).is_some()
    }
}

/// The compiler's rules as this project configured them, or `None` when every
/// one of them is off and the compiler has nothing to report.
pub(super) fn wanted(config: &UniflowedConfig) -> Option<CompilerWork> {
    let levels = RULES.map(|rule| (rule, severity(config, rule)));
    levels
        .iter()
        .any(|(_, level)| level.is_some())
        .then_some(CompilerWork { levels })
}
