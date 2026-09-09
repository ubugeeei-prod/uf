//! What a single-page build cannot answer, refused before it is built.
//!
//! `app.rendering.modes: ["csr"]` writes one shell and renders every route in
//! the browser. There is no server anywhere in that deployment — not at build
//! time, because no route is prerendered, and not afterwards, because no server
//! bundle survives — so anything in the project that only a server can do is a
//! hole in the application with nothing between it and the first visitor.
//!
//! The other rendering plans find these the way they can afford to. Under
//! `Prerender::Everything` the builder discovers them by *trying*: it renders
//! every route, and a page that calls `cookies()` throws where there is no
//! request. Under `Prerender::Shell` nothing is rendered at all, so the same
//! page would build cleanly, deploy cleanly, and fail in a browser — which is
//! the failure ubugeeei-prod/uf#336 and ubugeeei-prod/uf#385 are both about,
//! moved one step later and made invisible.
//!
//! So it is answered statically, from the analysis `uf build` has already run.
//! `crates/uf_rsc` resolves the module graph and already answers "is this
//! reachable from here" for server-only imports — the same question this asks
//! with the two halves swapped, because under `csr` the server graph *is* the
//! browser's. Inventing a second walk over the same imports would be a second
//! answer to a question that has one.
//!
//! # What counts
//!
//! * a `_uf.route.js`, which answers a request rather than rendering, and a
//!   `_uf.middleware.js`, which runs before one is answered. Neither has a
//!   process to run in;
//! * any module the application reaches that imports a **server-only package**
//!   — `@uniflowed/server`, `@uniflowed/db`, `server-only` — or is named
//!   `*.server.js`. `cookies()`, `headers()` and `draftMode()` come from the
//!   first of those, so "a loader that reads the request" is this case rather
//!   than a case of its own: reading a request needs a request, and a document
//!   rendered in a browser was not one.
//!
//! A server action is **not** here, and that is deliberate rather than an
//! omission: `refuse_unanswerable_actions` in the parent module already refuses
//! every callable one for any plan that emits no server, which a `csr` plan
//! does not. Repeating it would report one fault twice with two fixes.
//!
//! # Why the route and not only the file
//!
//! A module path is where the code is; a route is what a visitor asks for and
//! what a person recognises. So a finding a route reaches is reported as both,
//! and one that no route's page reaches — a layout, most often, which no page
//! imports — is reported by its file, because naming a route it does not belong
//! to would be worse than naming none.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::{Result, bail};
use camino::Utf8Path;
use uf_router::{Route, ServerModule, ServerModuleKind};
use uf_rsc::{ModuleId, RscGraph, SERVER_ONLY_SUFFIX, is_server_only_specifier};

use crate::support::{plural, relative_to};

/// How many findings a refusal lists before it stops.
///
/// The same ceiling `deploy::static_host` uses, for the same reason: a project
/// that has just written `["csr"]` wants to see the shape of what it costs,
/// and a hundred lines of it is a wall rather than a list.
const SHOWN: usize = 20;

/// One thing this deployment has and cannot run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Unanswerable {
    /// The route path, when one reaches it.
    pub(crate) route: Option<String>,
    /// The file, relative to the project root.
    pub(crate) file: String,
    /// What is wrong with it, as a clause following "because".
    pub(crate) because: String,
}

impl Unanswerable {
    /// The line a refusal prints for this finding.
    fn line(&self) -> String {
        match &self.route {
            Some(route) => format!("  {route} ({}) — {}", self.file, self.because),
            None => format!("  {} — {}", self.file, self.because),
        }
    }
}

/// Everything a `csr` build would deploy with nothing to answer it.
///
/// Sorted, because two builds of the same tree that list the same findings in
/// two orders is a message nobody can diff. Routes first — `Option<String>`
/// orders `None` last — so the lines a person recognises are at the top.
pub(crate) fn unanswerable(
    root: &Utf8Path,
    routes: &[Route],
    server_modules: &[ServerModule],
    graph: &RscGraph,
) -> Vec<Unanswerable> {
    let mut found = Vec::new();

    for module in server_modules {
        found.push(Unanswerable {
            route: Some(match module.kind {
                // A middleware guards a subtree rather than a URL that is
                // necessarily served, and is reported the way
                // `deploy::static_host` reports one: by the subtree.
                ServerModuleKind::Middleware => {
                    format!("{}/*", if module.path == "/" { "" } else { &module.path })
                }
                ServerModuleKind::RouteHandler => module.path.to_string(),
            }),
            file: relative_to(root, &module.file),
            because: match module.kind {
                ServerModuleKind::RouteHandler => String::from(
                    "it is a route handler, and a handler answers a request rather than \
                     rendering a page",
                ),
                ServerModuleKind::Middleware => String::from(
                    "a middleware runs before a request is answered, and nothing in this \
                     deployment answers one",
                ),
            },
        });
    }

    // Every module the application reaches that only a server can evaluate,
    // and the reason each is one. Computed once for the whole graph rather than
    // per route, because a helper five routes share is one fault.
    let offenders = server_only_modules(graph);
    if offenders.is_empty() {
        found.sort();
        return found;
    }

    // Which of them each route's page reaches. A route is what a person
    // recognises, so the walk is from the pages outwards rather than from the
    // offenders back.
    let mut attributed: BTreeSet<ModuleId> = BTreeSet::new();
    for route in routes {
        let Some(page) = graph.module_id(relative_to(root, &route.page)) else {
            continue;
        };
        for id in reachable(graph, page) {
            let Some(reason) = offenders.get(&id) else {
                continue;
            };
            attributed.insert(id);
            let file = graph
                .module_by_id(id)
                .map_or_else(String::new, |module| module.path.to_string());
            found.push(Unanswerable {
                route: Some(route.path.to_string()),
                file,
                because: reason.clone(),
            });
        }
    }

    // And the ones no page reaches. A layout is the ordinary case — a page does
    // not import the layout it renders inside — and reporting it under a route
    // it does not belong to would be a worse answer than reporting it under
    // none.
    for (id, reason) in &offenders {
        if attributed.contains(id) {
            continue;
        }
        let Some(module) = graph.module_by_id(*id) else {
            continue;
        };
        found.push(Unanswerable {
            route: None,
            file: module.path.to_string(),
            because: reason.clone(),
        });
    }

    found.sort();
    found.dedup();
    found
}

/// Refuse the build, or let it through.
///
/// Before the bundle rather than after it, for the reason the RSC diagnostics
/// are: a project that has to change what it deploys is not going to be helped
/// by thirty seconds of bundling first.
pub(crate) fn refuse(
    root: &Utf8Path,
    routes: &[Route],
    server_modules: &[ServerModule],
    graph: &RscGraph,
    because: &str,
) -> Result<()> {
    let findings = unanswerable(root, routes, server_modules, graph);
    if findings.is_empty() {
        return Ok(());
    }
    let mut message = format!(
        "{} {} this project {} a server, and {because}",
        findings.len(),
        plural(findings.len(), "thing"),
        if findings.len() == 1 { "needs" } else { "need" },
    );
    for finding in findings.iter().take(SHOWN) {
        message.push('\n');
        message.push_str(&finding.line());
    }
    if findings.len() > SHOWN {
        message.push_str(&format!(
            "\n  … and {} more",
            findings.len().saturating_sub(SHOWN)
        ));
    }
    message.push_str(
        "\n\nA single-page build renders every route in the browser, and a browser has no \
         request to read and no handler to run. Take these out, or allow `\"ssg\"` or \
         `\"ssr\"` in `app.rendering.modes` and deploy a build that has a server in it.",
    );
    bail!("{message}")
}

/// Modules only a server can evaluate, by id, with the reason each one is.
///
/// Reachability is the filter that makes this a fact about *this application*
/// rather than about the directory: a module nothing imports is dead code, and
/// refusing a build over code that never runs is a refusal nobody can act on
/// except by deleting a file they were not using.
fn server_only_modules(graph: &RscGraph) -> BTreeMap<ModuleId, String> {
    let mut found = BTreeMap::new();
    for module in graph.modules() {
        if !module.reachability.is_reachable() {
            continue;
        }
        // Through the graph's own lookup rather than by constructing an id from
        // the loop index. The two agree today; only one of them is a promise
        // `uf_rsc` has made, and an index cast into an opaque handle is the
        // kind of shortcut that is right until the module list is filtered.
        let Some(id) = graph.module_id(&module.path) else {
            continue;
        };
        if module.path.as_str().ends_with(SERVER_ONLY_SUFFIX) {
            found.insert(
                id,
                format!("its name ends in `{SERVER_ONLY_SUFFIX}`, which says it runs on a server"),
            );
            continue;
        }
        if let Some(specifier) = module
            .external_imports
            .iter()
            .find(|specifier| is_server_only_specifier(specifier))
        {
            found.insert(
                id,
                format!(
                    "it imports `{specifier}`, which only runs on a server — `cookies()`, \
                     `headers()` and `draftMode()` all read a request, and there is none"
                ),
            );
        }
    }
    found
}

/// Every module reachable from `start`, including it.
///
/// Breadth-first over the graph's own `imports`, which is the resolved edge
/// list `uf_rsc` already computed. A second resolver here would be a second
/// answer to "what does this module import", and the two would drift the first
/// time an alias or an extension rule changed.
fn reachable(graph: &RscGraph, start: ModuleId) -> BTreeSet<ModuleId> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    seen.insert(start);
    queue.push_back(start);
    while let Some(id) = queue.pop_front() {
        let Some(module) = graph.module_by_id(id) else {
            continue;
        };
        for next in &module.imports {
            if seen.insert(*next) {
                queue.push_back(*next);
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests;
