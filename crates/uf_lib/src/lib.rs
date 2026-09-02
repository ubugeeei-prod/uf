//! The registry of everything `@uniflowed/*` ships, as data the toolchain reads.
//!
//! [`builtin_modules`] and [`ui_components`] are the two tables, and
//! [`NativeModule`] and [`UiComponent`] are the shapes their entries take.
//! Nothing here performs work — it is the catalogue the CLI, the docs
//! generator and the loader all agree on.

mod descriptor;
mod registry;
mod ui;

pub use descriptor::{
    FormContract, FormMutationPhase, HookDescriptor, NativeModule, NativeModuleKind, SchemaKind,
    Stability, UiComponent, UiRuntime, ValidationStep,
};
pub use registry::{
    builtin_modules, hook_descriptors, module_by_specifier, std_module_descriptors, tui_contract,
};
pub use ui::ui_components;

#[cfg(test)]
mod tests;
