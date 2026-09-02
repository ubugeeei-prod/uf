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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiComponent {
    pub name: CompactString,
    pub parts: InlineVec<CompactString, 16>,
    pub runtime: UiRuntime,
    pub preset_style: bool,
    pub form: Option<FormContract>,
}

impl UiComponent {
    pub fn new(name: &str, parts: &[&str], runtime: UiRuntime) -> Self {
        Self {
            name: name.to_compact_string(),
            parts: parts
                .iter()
                .map(ToCompactString::to_compact_string)
                .collect(),
            runtime,
            preset_style: true,
            form: None,
        }
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
