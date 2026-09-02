//! The table of server actions a build exposes, and the lookup into it.
//!
//! Registration is where exposure is decided: an action is a callable endpoint
//! only when some module that can hand it across a client boundary reaches it,
//! and everything else is recorded but never dialable. The lookup answers every
//! failure identically and in the same time, so the endpoint cannot be used to
//! enumerate what a build contains.

use camino::Utf8PathBuf;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

use crate::directive::ModuleEnvironment;
use crate::graph::RscGraph;
use crate::scan::ExportKind;

use super::crypto::constant_time_eq;
use super::{ActionExposure, ActionId, BuildId, ServerActionKind, UnknownAction};

/// One server action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAction {
    /// Keyed identifier the client uses to call it.
    pub id: ActionId,
    /// Declaring module, relative to the project root.
    pub module: Utf8PathBuf,
    /// Export name, or the binding of an inline closure.
    pub export: CompactString,
    /// Where the action was declared.
    pub kind: ServerActionKind,
    /// Whether the client can reach it.
    pub exposure: ActionExposure,
}

/// Every server action of a build, and the lookup the runtime dials into.
#[derive(Debug, Clone)]
pub struct ServerActionRegistry {
    actions: Vec<ServerAction>,
    fingerprint: CompactString,
}

impl ServerActionRegistry {
    /// Collect the actions of a resolved graph.
    pub fn from_graph(graph: &RscGraph, build_id: &BuildId) -> Self {
        let modules = graph.modules();
        let hands_off = hands_off_to_client(graph);

        let mut actions: Vec<ServerAction> = Vec::new();
        for (position, module) in modules.iter().enumerate() {
            let exposure = if hands_off[position] {
                ActionExposure::CallableEndpoint
            } else {
                ActionExposure::UnreachableFromClient
            };

            if module.environment == ModuleEnvironment::ServerActions {
                for export in &module.exports {
                    // Only shapes React can actually invoke become endpoints;
                    // anything else is reported by the graph and left out.
                    if !matches!(
                        export.kind,
                        ExportKind::AsyncFunction | ExportKind::ReExport
                    ) {
                        continue;
                    }
                    actions.push(ServerAction {
                        id: ActionId::derive(
                            build_id,
                            module.path.as_str(),
                            &export.name,
                            ServerActionKind::ModuleExport,
                        ),
                        module: module.path.clone(),
                        export: export.name.clone(),
                        kind: ServerActionKind::ModuleExport,
                        exposure,
                    });
                }
            }

            for directive in &module.function_actions {
                let export = CompactString::from(directive.owner.to_string());
                actions.push(ServerAction {
                    id: ActionId::derive(
                        build_id,
                        module.path.as_str(),
                        &export,
                        ServerActionKind::InlineClosure,
                    ),
                    module: module.path.clone(),
                    export,
                    kind: ServerActionKind::InlineClosure,
                    exposure,
                });
            }
        }

        actions.sort_by(|left, right| {
            left.module
                .cmp(&right.module)
                .then(left.kind.cmp(&right.kind))
                .then(left.export.cmp(&right.export))
        });
        actions.dedup_by(|left, right| {
            left.module == right.module && left.kind == right.kind && left.export == right.export
        });

        Self {
            actions,
            fingerprint: build_id.fingerprint(),
        }
    }

    /// Every action, callable or not, ordered by module and name.
    pub fn actions(&self) -> &[ServerAction] {
        &self.actions
    }

    /// Only the actions that are callable endpoints.
    pub fn callable_actions(&self) -> impl Iterator<Item = &ServerAction> {
        self.actions
            .iter()
            .filter(|action| action.exposure.is_callable())
    }

    /// Number of registered actions.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// A publishable digest of the build id.
    pub fn build_fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Resolve an id received from a client.
    ///
    /// Malformed, forged, unknown and unreachable ids are all
    /// [`UnknownAction`]: the caller learns nothing beyond "no".
    pub fn resolve(&self, raw_id: &str) -> Result<&ServerAction, UnknownAction> {
        let id = ActionId::parse(raw_id).map_err(|_| UnknownAction)?;
        self.lookup(&id)
    }

    /// Resolve a parsed id.
    ///
    /// The scan visits every entry whatever happens, and compares with
    /// [`constant_time_eq`], so neither the running time nor the branch pattern
    /// depends on which entry matched — or on whether one did.
    pub fn lookup(&self, id: &ActionId) -> Result<&ServerAction, UnknownAction> {
        let mut selected = 0u32;
        let mut found = 0u8;

        for (position, action) in self.actions.iter().enumerate() {
            let matches = constant_time_eq(&action.id.0, &id.0)
                & u8::from(action.exposure == ActionExposure::CallableEndpoint);
            let mask = 0u32.wrapping_sub(u32::from(matches));
            selected = (selected & !mask) | ((position as u32) & mask);
            found |= matches;
        }

        if found == 1 {
            self.actions.get(selected as usize).ok_or(UnknownAction)
        } else {
            Err(UnknownAction)
        }
    }
}

/// Which modules can hand a server action across a client boundary.
///
/// A module qualifies when the client graph already contains it, or when the
/// server renders it *and* it reaches a `"use client"` import — that is the only
/// way a closure defined there can end up as a prop on a Client Component. Every
/// `"use server"` module such a module imports becomes callable with it.
fn hands_off_to_client(graph: &RscGraph) -> Vec<bool> {
    let modules = graph.modules();
    let mut hands_off = vec![false; modules.len()];

    for (position, module) in modules.iter().enumerate() {
        hands_off[position] = module.reachability.is_client_reachable()
            || (module.reachability.is_server_reachable() && module.proximity.reaches_boundary());
    }

    for (position, module) in modules.iter().enumerate() {
        if !hands_off[position] {
            continue;
        }
        for target in module.imports.iter().copied() {
            if modules[target.index()].environment == ModuleEnvironment::ServerActions {
                hands_off[target.index()] = true;
            }
        }
    }

    hands_off
}
