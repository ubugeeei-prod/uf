//! The shapes the registry tables are made of.
//!
//! Every entry a table emits is one of these: a native module, a hook, a UI
//! component, or the form contract a component opts into. Keeping them apart
//! from the tables means a new entry never has to touch a type, and a changed
//! type is reviewed on its own.

use compact_str::{CompactString, ToCompactString};
use serde::Serialize;
use uf_infra::InlineVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeModuleKind {
    Data,
    Effect,
    Framework,
    Hooks,
    Runtime,
    Std,
    Style,
    Testing,
    Ui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stability {
    Experimental,
    Planned,
    Stable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeModule {
    pub specifier: CompactString,
    pub kind: NativeModuleKind,
    pub stability: Stability,
    pub flow_exports: InlineVec<CompactString, 8>,
}

impl NativeModule {
    pub fn new(
        specifier: &str,
        kind: NativeModuleKind,
        stability: Stability,
        exports: &[&str],
    ) -> Self {
        Self {
            specifier: specifier.to_compact_string(),
            kind,
            stability,
            flow_exports: exports
                .iter()
                .map(ToCompactString::to_compact_string)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookDescriptor {
    pub name: CompactString,
    pub idempotent_render: bool,
    pub server_component_safe: bool,
}

impl HookDescriptor {
    pub fn new(name: &str, idempotent_render: bool, server_component_safe: bool) -> Self {
        Self {
            name: name.to_compact_string(),
            idempotent_render,
            server_component_safe,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiRuntime {
    Server,
    Client,
    Split,
}

/// Whether an entry in the UI table is a thing a caller can import.
///
/// The table was written as a roadmap and read as an inventory, and for a long
/// time it was only the first: at ubugeeei-prod/uf#249 it held fifty-one names,
/// of which the package shipped seven, and `uf inspect` reported the fifty-one.
/// That is the whole of that issue — the table is a good roadmap and was a bad
/// inventory, and it should be allowed to stay the first while it stops being
/// the second.
///
/// Three members rather than two, because the check in
/// `the_ui_table_names_exactly_what_the_package_ships` has to tell an entry
/// nobody has written yet from one nobody is going to. Both are absent from
/// `packages/ui`; only one of them is a gap. Keeping that in a comment — which
/// is where `Command` and `DataTable` kept it — means the check cannot read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiReadiness {
    /// `@uniflowed/ui` exports it, and [`UiComponent::parts`] is what its
    /// namespace object holds. Checked against the package on every test run.
    Implemented,
    /// Nobody has written it yet. The parts are the shape it is expected to
    /// take, which is a design note and not a promise.
    Planned,
    /// Deliberately not a component in this package. The entry stays because
    /// the decision is worth finding where somebody would look for the
    /// component rather than only in a documentation page, and the comment on
    /// it says what was decided and what would reopen it.
    Declined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiComponent {
    pub name: CompactString,
    pub parts: InlineVec<CompactString, 16>,
    pub runtime: UiRuntime,
    pub readiness: UiReadiness,
    /// The `@uniflowed/stylex/preset` exports that dress this component.
    ///
    /// Names rather than the `bool` this was, and the reason is the same one
    /// [`UiReadiness`] exists for: the flag was set by `new` and never unset,
    /// so it said "styled" about every entry in the table, including the ones
    /// nobody had drawn and the ones the preset has no function for. A list of
    /// names is a claim a reader can follow and a test can check —
    /// `the_preset_styles_a_component_names_are_exports_of_the_preset` holds
    /// every name here to `packages/stylex/preset.js` and every export there to
    /// this table, in both directions.
    ///
    /// Claimed conservatively: an entry names a function only where the
    /// preset's own sentence for that function names the element this
    /// component renders. A component whose look a caller has to author is
    /// empty here rather than optimistic, which is the difference between this
    /// field and the one it replaced.
    pub preset_styles: InlineVec<CompactString, 4>,
    pub form: Option<FormContract>,
}

impl UiComponent {
    /// An implemented component with no preset styles, which is what most of
    /// the table is.
    ///
    /// Implemented is the default because it is the answer this crate can
    /// check: an entry that is wrong about being shipped fails
    /// `the_ui_table_names_exactly_what_the_package_ships` on the next run,
    /// while an entry wrongly marked [`UiReadiness::Planned`] would only be
    /// under-claiming quietly. Defaulting to the checked answer is what keeps
    /// a new entry from being able to drift.
    pub fn new(name: &str, parts: &[&str], runtime: UiRuntime) -> Self {
        Self {
            name: name.to_compact_string(),
            parts: parts
                .iter()
                .map(ToCompactString::to_compact_string)
                .collect(),
            runtime,
            readiness: UiReadiness::Implemented,
            preset_styles: InlineVec::new(),
            form: None,
        }
    }

    /// Nobody has written it yet. See [`UiReadiness::Planned`].
    pub fn planned(mut self) -> Self {
        self.readiness = UiReadiness::Planned;
        self
    }

    /// Deliberately not a component here. See [`UiReadiness::Declined`].
    pub fn declined(mut self) -> Self {
        self.readiness = UiReadiness::Declined;
        self
    }

    /// The preset functions that dress it. See [`UiComponent::preset_styles`].
    pub fn styled_by(mut self, exports: &[&str]) -> Self {
        self.preset_styles = exports
            .iter()
            .map(ToCompactString::to_compact_string)
            .collect();
        self
    }

    pub fn with_form(mut self, form: FormContract) -> Self {
        self.form = Some(form);
        self
    }

    pub fn has_part(&self, part: &str) -> bool {
        self.parts.iter().any(|candidate| candidate == part)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormContract {
    pub validator_module: CompactString,
    pub schema_kind: SchemaKind,
    pub allowed_steps: InlineVec<ValidationStep, 8>,
    pub compiler_safe: bool,
    pub render_idempotent: bool,
    pub mutation_phase: FormMutationPhase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaKind {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationStep {
    MinLength,
    MaxLength,
    StartsWith,
    Min,
    Max,
    Integer,
}

impl FormContract {
    pub fn validator_backed() -> Self {
        Self {
            validator_module: CompactString::const_new("@uniflowed/validator"),
            schema_kind: SchemaKind::Object,
            allowed_steps: InlineVec::new(),
            compiler_safe: true,
            render_idempotent: true,
            mutation_phase: FormMutationPhase::EventOrServerAction,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormMutationPhase {
    EventOrServerAction,
}
