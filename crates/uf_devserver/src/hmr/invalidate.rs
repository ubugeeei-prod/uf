//! Deciding, exactly, what a change made stale.
//!
//! Over-invalidating turns hot module replacement into a page reload, which is
//! the feature failing quietly. Under-invalidating serves stale code, which is
//! worse: the browser shows a build nobody wrote. So this module does not
//! approximate in either direction, and every rule below has a test.
//!
//! # Two walks
//!
//! **Up.** From the changed module, climb importer edges to collect the
//! *affected set*: the modules whose own evaluation changes. There is exactly
//! one edge the climb refuses to cross — a `"use client"` module imported by a
//! server module. The server does not evaluate a Client Component; it emits a
//! reference to it, and that reference is unchanged. Crossing it anyway would
//! turn every Fast Refresh into a route re-render.
//!
//! **Down.** Sides are decided by where an import chain *starts*, not where it
//! ends, so they cannot be read off during the upward climb. The tops of the
//! affected set — the modules the climb stopped at — seed a second walk back
//! down the same edges: a `"use client"` top paints its subtree
//! [`UpdateSide::Client`], a server top paints its subtree
//! [`UpdateSide::Server`], and a module reached from both is both. That is how
//! a shared utility imported by a Client Component *and* by a route ends up
//! correctly invalidating the browser *and* the route.
//!
//! # Termination
//!
//! Import graphs contain cycles. Both walks are worklists with a per-side seen
//! set, so every `(module, side)` pair is processed at most once and neither
//! walk recurses. A cycle costs one visit per member, not a stack overflow.
//!
//! # Fast Refresh
//!
//! A client module whose every runtime export is a component
//! ([`crate::hmr::ModuleSurface::Component`]) accepts the update: React swaps
//! the module and re-renders it in place, so the climb stops there and its
//! importers keep their bindings. A client module that exports anything else
//! cannot be swapped without handing existing callers a stale binding, so the
//! update propagates. When it propagates all the way to the top of the client
//! graph without finding an accepting module, the honest answer is a full
//! reload — reported with a [`ReloadReason`], never silently.

use std::collections::VecDeque;

use uf_rsc::ModuleEnvironment;

use super::graph::{DevGraph, DevModuleId, ModuleState, ModuleSurface};

mod verdict;

pub use verdict::{ChangeKind, Invalidation, ReloadReason, UpdateKind, UpdateSide};

/// How far up the importer chain a single invalidation will climb.
///
/// Rule 4 of `docs/security.md`: the walk is bounded, and exceeding the bound
/// is a full reload with [`ReloadReason::DepthExceeded`] rather than a hang.
/// Over-invalidating is the safe direction, and it is the *reported* direction.
pub const MAX_INVALIDATION_DEPTH: usize = 1_024;

/// Bit flags of the per-module seen set, one byte per module.
///
/// A flat `Vec<u8>` rather than three hash sets: one allocation, one memset,
/// and a constant-time test per visit. On the 5 000-module benchmark graph that
/// is five kilobytes per invalidation, which is why an edit costs microseconds.
const SEEN_AFFECTED: u8 = 1;
const SEEN_CLIENT: u8 = 2;
const SEEN_SERVER: u8 = 4;

/// Decide what `change` to `changed` made stale.
///
/// The walk is total: every input, including a cyclic graph, a module the graph
/// has marked absent, and a graph that is deeper than the bound, produces an
/// [`Invalidation`] rather than an error or a panic.
pub fn invalidate(graph: &DevGraph, changed: DevModuleId, change: ChangeKind) -> Invalidation {
    let Some(module) = graph.module(changed) else {
        return Invalidation::default();
    };

    // A module with nothing to import from it cannot change what an importer
    // sees. Types are erased before a browser ever loads the file, so an edit
    // that only moves type declarations around is genuinely inert.
    if change != ChangeKind::Deleted
        && module.surface() == ModuleSurface::Erased
        && module.state() == ModuleState::Present
    {
        return Invalidation::default();
    }

    let mut seen = vec![0u8; graph.len()];
    let (affected, tops, boundaries, depth_exceeded) = climb(graph, changed, &mut seen);
    let (client, server) = paint(graph, &affected, &tops, &mut seen);

    let mut reload = None;
    if depth_exceeded {
        reload = Some(ReloadReason::DepthExceeded);
    } else if !client.is_empty() {
        if change == ChangeKind::Deleted {
            // The browser holds a module whose bytes no longer exist. Nothing
            // can be swapped in for it.
            reload = Some(ReloadReason::ModuleRemoved);
        } else if boundaries.is_empty() {
            reload = Some(ReloadReason::NoAcceptingBoundary);
        }
    }

    let mut boundaries: Vec<DevModuleId> = boundaries
        .into_iter()
        .filter(|id| client.binary_search(id).is_ok())
        .collect();
    boundaries.sort_unstable();

    Invalidation {
        client,
        server,
        boundaries,
        reload,
    }
}

/// Climb importer edges, collecting the affected set and the modules the climb
/// stopped at.
///
/// Returns `(affected, tops, boundaries, depth_exceeded)`.
fn climb(
    graph: &DevGraph,
    changed: DevModuleId,
    seen: &mut [u8],
) -> (Vec<DevModuleId>, Vec<DevModuleId>, Vec<DevModuleId>, bool) {
    let mut affected = Vec::new();
    let mut tops = Vec::new();
    let mut boundaries = Vec::new();
    let mut depth_exceeded = false;
    let mut work: VecDeque<(DevModuleId, usize)> = VecDeque::new();

    seen[changed.index()] |= SEEN_AFFECTED;
    affected.push(changed);
    work.push_back((changed, 0));

    while let Some((id, depth)) = work.pop_front() {
        let Some(module) = graph.module(id) else {
            continue;
        };

        // A client module whose exports are all components takes the update
        // itself. Nothing above it re-evaluates, so the climb ends here — and
        // this is the one place a hot update stays small.
        if module.environment() == ModuleEnvironment::Client && module.surface().accepts_update() {
            boundaries.push(id);
            tops.push(id);
            continue;
        }

        if depth >= MAX_INVALIDATION_DEPTH {
            depth_exceeded = true;
            tops.push(id);
            continue;
        }

        let mut climbed = false;
        for importer in module.importers().iter().copied() {
            let Some(above) = graph.module(importer) else {
                continue;
            };
            // The one edge the climb refuses. A server module holds a reference
            // to a `"use client"` module; it never evaluates it, so a change
            // below the boundary leaves the server's own output identical.
            if module.environment() == ModuleEnvironment::Client
                && above.environment() != ModuleEnvironment::Client
            {
                continue;
            }
            climbed = true;
            if seen[importer.index()] & SEEN_AFFECTED != 0 {
                continue;
            }
            seen[importer.index()] |= SEEN_AFFECTED;
            affected.push(importer);
            work.push_back((importer, depth + 1));
        }

        if !climbed {
            tops.push(id);
        }
    }

    (affected, tops, boundaries, depth_exceeded)
}

/// Paint the affected set with the sides its tops belong to.
///
/// Runs down the same edges the climb came up, so a module is client-side
/// exactly when a `"use client"` module above it is, and server-side exactly
/// when a server root above it is.
fn paint(
    graph: &DevGraph,
    affected: &[DevModuleId],
    tops: &[DevModuleId],
    seen: &mut [u8],
) -> (Vec<DevModuleId>, Vec<DevModuleId>) {
    let mut client = Vec::new();
    let mut server = Vec::new();
    let mut work: VecDeque<(DevModuleId, UpdateSide)> = VecDeque::new();

    for top in tops.iter().copied() {
        let side = own_side(graph, top);
        push_side(top, side, seen, &mut client, &mut server, &mut work);
    }
    drain(graph, seen, &mut client, &mut server, &mut work);

    // An affected module no top reaches is a module inside an import cycle that
    // reaches no root at all. It still has to be told apart, so it is seeded
    // with the side its own environment implies and the walk runs again.
    for id in affected.iter().copied() {
        if seen[id.index()] & (SEEN_CLIENT | SEEN_SERVER) != 0 {
            continue;
        }
        let side = own_side(graph, id);
        push_side(id, side, seen, &mut client, &mut server, &mut work);
        drain(graph, seen, &mut client, &mut server, &mut work);
    }

    client.sort_unstable();
    server.sort_unstable();
    (client, server)
}

/// Run the downward worklist to exhaustion.
fn drain(
    graph: &DevGraph,
    seen: &mut [u8],
    client: &mut Vec<DevModuleId>,
    server: &mut Vec<DevModuleId>,
    work: &mut VecDeque<(DevModuleId, UpdateSide)>,
) {
    while let Some((id, side)) = work.pop_front() {
        let Some(module) = graph.module(id) else {
            continue;
        };
        for target in module.imports().iter().copied() {
            if seen[target.index()] & SEEN_AFFECTED == 0 {
                continue;
            }
            let Some(below) = graph.module(target) else {
                continue;
            };
            // Crossing back down into a `"use client"` module from a server
            // module would paint the client subtree server-side. The climb
            // never came up that edge, and the paint does not go down it.
            if side == UpdateSide::Server
                && below.environment() == ModuleEnvironment::Client
                && module.environment() != ModuleEnvironment::Client
            {
                continue;
            }
            push_side(target, side, seen, client, server, work);
        }
    }
}

/// The side a module belongs to when nothing above it decides.
fn own_side(graph: &DevGraph, id: DevModuleId) -> UpdateSide {
    match graph.module(id).map(|module| module.environment()) {
        Some(ModuleEnvironment::Client) => UpdateSide::Client,
        _ => UpdateSide::Server,
    }
}

fn push_side(
    id: DevModuleId,
    side: UpdateSide,
    seen: &mut [u8],
    client: &mut Vec<DevModuleId>,
    server: &mut Vec<DevModuleId>,
    work: &mut VecDeque<(DevModuleId, UpdateSide)>,
) {
    let (flag, bucket) = match side {
        UpdateSide::Client => (SEEN_CLIENT, client),
        UpdateSide::Server => (SEEN_SERVER, server),
    };
    if seen[id.index()] & flag != 0 {
        return;
    }
    seen[id.index()] |= flag;
    bucket.push(id);
    work.push_back((id, side));
}

#[cfg(test)]
mod tests;
