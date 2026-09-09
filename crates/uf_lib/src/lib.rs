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
    Stability, UiComponent, UiReadiness, UiRuntime, ValidationStep,
};
pub use registry::{
    CLIENT_MODULE_PACKAGE, CLIENT_MODULE_SUBPATHS, builtin_modules, hook_descriptors,
    is_client_module, module_by_specifier, std_module_descriptors, tui_contract,
};
pub use ui::ui_components;
// Re-exported beside [`std_module_descriptors`] rather than left to callers to
// reach for: a consumer that reads the table has to read the status to know
// which of an entry's other fields are facts, and making it take a second
// dependency to do that is how a count gets trusted instead.
pub use uf_std::{StdCategory, StdModule, StdStatus};

#[cfg(test)]
mod tests;
