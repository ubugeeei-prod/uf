//! What a hook is handed, what it may answer, and how it may fail.

use compact_str::CompactString;
use serde::Serialize;
use thiserror::Error;

use crate::hook::PluginHook;

/// What a plugin did with the value it was handed.
///
/// This is the type that makes the container's two combining rules safe. A
/// plugin never decides whether it "wins": it says only whether it produced
/// something, and [`PluginHook::dispatch`](crate::PluginHook::dispatch) decides
/// what that means. A first-wins hook stops at the first `Handled`; a chained
/// hook feeds each `Handled` to the next plugin and keeps going.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HookOutcome<T> {
    /// The plugin produced a value.
    Handled(T),
    /// The plugin declined; the input is unchanged and the next plugin runs.
    Passthrough,
}

impl<T> HookOutcome<T> {
    /// Whether the plugin produced a value.
    pub const fn is_handled(&self) -> bool {
        matches!(self, Self::Handled(_))
    }

    /// Whether the plugin declined.
    pub const fn is_passthrough(&self) -> bool {
        matches!(self, Self::Passthrough)
    }

    /// The produced value, if any.
    pub fn handled(self) -> Option<T> {
        match self {
            Self::Handled(value) => Some(value),
            Self::Passthrough => None,
        }
    }

    /// The produced value, or `fallback` when the plugin declined.
    pub fn unwrap_or(self, fallback: T) -> T {
        self.handled().unwrap_or(fallback)
    }

    /// Apply `f` to the produced value, keeping `Passthrough` as it is.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> HookOutcome<U> {
        match self {
            Self::Handled(value) => HookOutcome::Handled(f(value)),
            Self::Passthrough => HookOutcome::Passthrough,
        }
    }

    /// A borrowed view of the produced value.
    pub fn as_ref(&self) -> HookOutcome<&T> {
        match self {
            Self::Handled(value) => HookOutcome::Handled(value),
            Self::Passthrough => HookOutcome::Passthrough,
        }
    }
}

impl<T> From<Option<T>> for HookOutcome<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Handled(value),
            None => Self::Passthrough,
        }
    }
}

/// A hook's answer, or a typed failure.
pub type HookResult<T> = Result<HookOutcome<T>, HookFailure>;

/// The import specifier a plugin is asked to resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolveInput<'a> {
    /// The specifier exactly as it appeared in the source.
    pub specifier: &'a str,
    /// The module that imported it, if this is not an entry point.
    pub importer: Option<&'a str>,
}

/// What kind of thing a resolved id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolvedKind {
    /// A real module that goes into the graph.
    Bundled,
    /// Left for the runtime to resolve; never read from disk.
    External,
    /// Produced by a plugin's `Load` hook rather than by the filesystem.
    Virtual,
}

/// A module id a plugin claimed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedId {
    /// The id every later hook uses for this module.
    pub id: CompactString,
    /// What the id names.
    pub kind: ResolvedKind,
}

impl ResolvedId {
    /// A resolved id for a module that goes into the graph.
    pub fn bundled(id: impl Into<CompactString>) -> Self {
        Self {
            id: id.into(),
            kind: ResolvedKind::Bundled,
        }
    }

    /// A resolved id left for the runtime.
    pub fn external(id: impl Into<CompactString>) -> Self {
        Self {
            id: id.into(),
            kind: ResolvedKind::External,
        }
    }

    /// A resolved id a `Load` hook will supply the source for.
    pub fn virtual_module(id: impl Into<CompactString>) -> Self {
        Self {
            id: id.into(),
            kind: ResolvedKind::Virtual,
        }
    }
}

/// The module a plugin is asked to load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadInput<'a> {
    /// The resolved module id.
    pub id: &'a str,
}

/// The module a plugin is asked to transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransformInput<'a> {
    /// The resolved module id.
    pub id: &'a str,
    /// The source text as the previous plugin left it.
    pub code: &'a str,
}

/// Source text a plugin produced, from either `Load` or `Transform`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleCode {
    /// The module's source text.
    pub code: String,
    /// A source map for it, when the plugin produced one.
    pub source_map: Option<String>,
}

impl ModuleCode {
    /// Source text with no map.
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            source_map: None,
        }
    }

    /// Attach a source map.
    #[must_use]
    pub fn with_source_map(mut self, source_map: impl Into<String>) -> Self {
        self.source_map = Some(source_map.into());
        self
    }
}

/// Why a plugin could not complete a hook.
///
/// The variants are a closed set with structured fields rather than a message,
/// so a failure can be matched on, counted, and rendered by the CLI without
/// anyone parsing prose. `Rejected` carries a `&'static str` rule id for the
/// general case, matching how the linter names its rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum HookFailure {
    /// The module's syntax is not something this plugin can process.
    #[error("unsupported syntax at byte offset {offset}")]
    UnsupportedSyntax {
        /// Byte offset into the input the plugin was handed.
        offset: usize,
    },
    /// The plugin needs another hook to have run first.
    #[error("the {required} hook must run before {hook}")]
    MissingPrerequisite {
        /// The hook that has to run first.
        required: PluginHook,
        /// The hook that could not run.
        hook: PluginHook,
    },
    /// The input is larger than the plugin is willing to hold in memory.
    #[error("input is {bytes} bytes, over the {limit} byte ceiling")]
    InputTooLarge {
        /// Size of the input.
        bytes: usize,
        /// The plugin's ceiling.
        limit: usize,
    },
    /// The plugin refused the module because it breaks a project rule.
    #[error("rejected by rule {rule}")]
    Rejected {
        /// Rule id, in the same `namespace/name` shape the linter uses.
        rule: &'static str,
    },
}

impl std::fmt::Display for PluginHook {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
