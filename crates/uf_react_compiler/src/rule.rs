//! The rules the validator enforces, and the shape it reports them in.
//!
//! A finding is an enum, not a string: the rule id, the message and the
//! severity a linter would give it are all derived from the variant, so there
//! is exactly one place where the wording of a diagnostic lives and no way to
//! report a rule that does not exist.

use compact_str::CompactString;
use serde::Serialize;

/// A rule the React compiler's syntax mode checks.
///
/// The ids are in the `namespace/name` shape `uf_lint` uses, so a diagnostic
/// from here renders through the same code frame as a lint diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactCompilerRule {
    /// Hooks are called unconditionally, at the top level of a hook-eligible
    /// function.
    HooksRules,
    /// Props, and values aliased from props, are not written to.
    NoPropsMutation,
    /// A value handed to a hook is not written to afterwards.
    NoMutationAfterHook,
    /// A ref is not read while rendering.
    NoRefReadInRender,
    /// Render produces no effect a re-render could not repeat.
    NoRenderSideEffects,
}

impl ReactCompilerRule {
    /// Every rule, in id order.
    pub const ALL: [Self; 5] = [
        Self::HooksRules,
        Self::NoMutationAfterHook,
        Self::NoPropsMutation,
        Self::NoRefReadInRender,
        Self::NoRenderSideEffects,
    ];

    /// The canonical rule id.
    pub const fn id(self) -> &'static str {
        match self {
            Self::HooksRules => "react/hooks-rules",
            Self::NoPropsMutation => "react/no-props-mutation",
            Self::NoMutationAfterHook => "react/no-mutation-after-hook",
            Self::NoRefReadInRender => "react/no-ref-read-in-render",
            Self::NoRenderSideEffects => "react/no-render-side-effects",
        }
    }

    /// One line saying what the rule is for.
    pub const fn description(self) -> &'static str {
        match self {
            Self::HooksRules => {
                "hooks run in call order, so every render has to make the same calls"
            }
            Self::NoPropsMutation => {
                "props belong to the caller; a component may not write to them"
            }
            Self::NoMutationAfterHook => {
                "a value a hook has seen is memoized, so writing to it later is invisible"
            }
            Self::NoRefReadInRender => {
                "a ref is state React does not track; reading it makes render impure"
            }
            Self::NoRenderSideEffects => "render may be run twice, and must be safe to repeat",
        }
    }
}

/// Exactly what the validator found.
///
/// Each variant is one sentence a developer can act on, and each one maps to a
/// single rule — so the rule a diagnostic is filed under is never a judgement
/// call at the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Finding {
    /// A hook was called outside any component, hook, or `useX` function.
    HookOutsideComponent,
    /// A hook was called inside a condition, a loop, or a callback.
    HookNotAtTopLevel,
    /// A hook was called after a `return` in the same function.
    HookAfterEarlyReturn,
    /// A prop, or a value aliased from one, was written to.
    PropsMutated,
    /// A value was written to after it had been handed to a hook.
    MutationAfterHook,
    /// `ref.current` was read while rendering.
    RefReadDuringRender,
    /// A module-level binding was assigned to while rendering.
    ModuleBindingAssigned,
    /// `console.*` was called while rendering.
    ConsoleDuringRender,
    /// A browser global was reached for while rendering.
    DomAccessDuringRender,
    /// Render read a value that changes on its own.
    UnstableReadDuringRender,
}

impl Finding {
    /// The rule this finding is filed under.
    pub const fn rule(self) -> ReactCompilerRule {
        match self {
            Self::HookOutsideComponent | Self::HookNotAtTopLevel | Self::HookAfterEarlyReturn => {
                ReactCompilerRule::HooksRules
            }
            Self::PropsMutated => ReactCompilerRule::NoPropsMutation,
            Self::MutationAfterHook => ReactCompilerRule::NoMutationAfterHook,
            Self::RefReadDuringRender => ReactCompilerRule::NoRefReadInRender,
            Self::ModuleBindingAssigned
            | Self::ConsoleDuringRender
            | Self::DomAccessDuringRender
            | Self::UnstableReadDuringRender => ReactCompilerRule::NoRenderSideEffects,
        }
    }

    /// The message shown to the developer.
    ///
    /// `&'static str` rather than a formatted `String`: a validator runs over
    /// every module of every build, and a finding is rare, so nothing here
    /// should allocate on the path that produces one.
    pub const fn message(self) -> &'static str {
        match self {
            Self::HookOutsideComponent => {
                "call hooks only inside a `component`, a `hook`, or a `useX` function"
            }
            Self::HookNotAtTopLevel => {
                "call hooks at the top level; not inside conditions, loops, or callbacks"
            }
            Self::HookAfterEarlyReturn => {
                "call hooks before any early return; a return above them makes the call conditional"
            }
            Self::PropsMutated => {
                "props belong to the caller; derive a new value instead of writing to this one"
            }
            Self::MutationAfterHook => {
                "this value was handed to a hook; writing to it afterwards is a change React cannot see"
            }
            Self::RefReadDuringRender => {
                "read a ref in an effect or an event handler; during render it makes output depend on untracked state"
            }
            Self::ModuleBindingAssigned => {
                "render must not write to module state; move the write into an effect or an action"
            }
            Self::ConsoleDuringRender => {
                "render may run twice; move logging into an effect or an event handler"
            }
            Self::DomAccessDuringRender => {
                "the DOM does not exist while a component renders; read it from an effect"
            }
            Self::UnstableReadDuringRender => {
                "keep render idempotent; move unstable reads into actions, effects, or loaders"
            }
        }
    }
}

/// One finding, at one place in one module.
///
/// The field names and the 1-based byte positions match `uf_lint::Diagnostic`,
/// so `uf_term`'s code-frame renderer draws this without a conversion step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactDiagnostic {
    /// What was found.
    pub finding: Finding,
    /// 1-based line.
    pub line: usize,
    /// 1-based byte column within the line.
    pub column: usize,
    /// Length of the offending span, in bytes.
    pub span: usize,
    /// The identifier at fault, when the finding is about one.
    pub symbol: Option<CompactString>,
}

impl ReactDiagnostic {
    /// The canonical rule id.
    pub fn rule(&self) -> &'static str {
        self.finding.rule().id()
    }

    /// The message shown to the developer.
    pub fn message(&self) -> &'static str {
        self.finding.message()
    }
}
