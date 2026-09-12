//! The contract checks a built graph runs, once the colours are known.
//!
//! Reachability decides what is worth complaining about: a client-only hook is
//! only a problem in a module the server actually executes, and a server-only
//! import is only a leak when something in the client graph reaches it. That is
//! why these checks live after propagation rather than inside the scan.

use compact_str::CompactString;

use crate::directive::ModuleEnvironment;
use crate::scan::ExportKind;

use super::build::{HookCallVerdict, HookClassifications, ResolvedImports};
use super::diagnostic::RscDiagnostic;
use super::resolve::{is_server_only_path, is_server_only_specifier};
use super::{ModuleId, ModuleReachability, RscModule, RscModuleInput};

pub(crate) fn report_module_diagnostics(
    module: &RscModuleInput,
    module_id: ModuleId,
    reachability: ModuleReachability,
    resolved: &ResolvedImports,
    hook_classifications: &HookClassifications,
    diagnostics: &mut Vec<RscDiagnostic>,
) {
    for issue in &module.directive_issues {
        diagnostics.push(RscDiagnostic::Directive {
            module: module.path.clone(),
            issue: issue.clone(),
        });
    }

    if module.environment == ModuleEnvironment::ServerActions {
        for export in &module.exports {
            match export.kind {
                ExportKind::AsyncFunction | ExportKind::ReExport => {}
                ExportKind::SyncFunction => {
                    diagnostics.push(RscDiagnostic::ServerActionNotAsync {
                        module: module.path.clone(),
                        export: export.name.clone(),
                        line: export.line,
                    });
                }
                ExportKind::Class | ExportKind::Value => {
                    diagnostics.push(RscDiagnostic::ServerActionNotFunction {
                        module: module.path.clone(),
                        export: export.name.clone(),
                        line: export.line,
                    });
                }
            }
        }
    }

    // Client-only APIs only matter for code the server actually executes. A
    // module without a directive that is reached solely from the client graph is
    // bundled for the browser, where the hooks it calls are legal.
    if module.environment.runs_on_server() && reachability.is_server_reachable() {
        for use_site in &module.client_api_uses {
            diagnostics.push(RscDiagnostic::ClientOnlyApiInServerModule {
                module: module.path.clone(),
                api: use_site.api,
                line: use_site.line,
                column: use_site.column,
            });
        }
        // Under exactly the same condition, and for the same reason: a hook
        // the name lists do not know is only a question worth asking about
        // code the server runs.
        //
        // Three answers now. A hook `@uniflowed/hooks` exports is decided from
        // the `server_component_safe` the registry has carried for it all
        // along; a project hook whose export body the graph can follow is
        // decided by the same fixpoint; anything else is still reported as the
        // question it is.
        for call in &module.hook_calls {
            match hook_classifications.call_verdict(module_id, call, resolved) {
                HookCallVerdict::ClientOnly(package) => {
                    diagnostics.push(RscDiagnostic::ClientOnlyHookInServerModule {
                        module: module.path.clone(),
                        hook: call.name.clone(),
                        package,
                        line: call.line,
                        column: call.column,
                    });
                }
                HookCallVerdict::ServerSafe => {}
                HookCallVerdict::Unknown => {
                    diagnostics.push(RscDiagnostic::UnclassifiedHookInServerModule {
                        module: module.path.clone(),
                        hook: call.name.clone(),
                        line: call.line,
                        column: call.column,
                    });
                }
            }
        }
    }

    if module.environment == ModuleEnvironment::Client || reachability.is_client_reachable() {
        for import in &resolved.external {
            if is_server_only_specifier(&import.specifier) {
                diagnostics.push(RscDiagnostic::ServerOnlyImportInClientModule {
                    module: module.path.clone(),
                    specifier: import.specifier.clone(),
                    line: import.line,
                });
            }
        }
    }
}

/// Report `*.server.js` modules that the client graph resolved to.
pub(crate) fn report_client_graph_leaks(
    modules: &[RscModule],
    resolved: &[ResolvedImports],
    diagnostics: &mut Vec<RscDiagnostic>,
) {
    for (position, module) in modules.iter().enumerate() {
        if module.environment != ModuleEnvironment::Client
            && !module.reachability.is_client_reachable()
        {
            continue;
        }
        for target in resolved[position].imports.iter().copied() {
            let imported = &modules[target.index()];
            if is_server_only_path(imported.path.as_str()) {
                diagnostics.push(RscDiagnostic::ServerOnlyImportInClientModule {
                    module: module.path.clone(),
                    specifier: CompactString::from(imported.path.as_str()),
                    line: 0,
                });
            }
        }
    }
}
