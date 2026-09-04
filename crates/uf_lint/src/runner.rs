//! One module per rule namespace, each owning the runners for the rules in it.
//!
//! A runner is the same shape every time: ask [`crate::severity`] whether the
//! rule is on, walk the already-scanned file, and push a diagnostic per hit. It
//! never decides its own severity and never re-reads the source, which is why
//! adding a rule costs one function rather than a pass.

mod fetch;
mod flow_expression;
mod flow_module;
mod flow_syntax;
mod flow_type;
mod package;
mod react;
mod react_compiler;
mod react_native;
mod router;
mod security;
mod server;
mod structure;
mod uniflowed;

pub(crate) use fetch::run_fetch_no_global_override;
pub(crate) use flow_expression::{
    run_flow_unnecessary_optional_chain, run_flow_unsafe_getters_setters,
    run_flow_unsafe_object_assign,
};
pub(crate) use flow_module::{
    run_flow_export_renamed_default, run_flow_mixed_import_and_require,
    run_flow_non_const_var_export,
};
pub(crate) use flow_syntax::run_flow_syntax;
pub(crate) use flow_type::{
    run_flow_ambiguous_object_type, run_flow_deprecated_type, run_flow_internal_type,
    run_flow_unclear_type,
};
pub(crate) use package::run_package_no_npm_scripts;
pub(crate) use react::{
    run_react_component_syntax, run_react_hook_syntax, run_react_no_default_export_component,
};
pub(crate) use react_compiler::run_react_compiler_rules;
pub(crate) use react_native::run_react_native_platform_split;
pub(crate) use router::run_router_reserved_files;
pub(crate) use security::{run_security_no_dangerously_set_inner_html, run_security_no_eval};
pub(crate) use server::{
    run_server_no_client_secret, run_server_no_server_only_import_in_client,
    run_server_use_client_directive_position, run_server_use_server_actions,
};
pub(crate) use structure::run_structure_rules;
pub(crate) use uniflowed::{run_no_npm_script_invocation, run_no_tabs, run_no_trailing_whitespace};
