//! Routes whose document is written once, refused when their render reads the
//! request.
//!
//! A prerendered page is written at build time, and a page that states a cache
//! lifetime is written into the route cache and served to every request.
//! Either way the document is about no request in particular, so a render that
//! reads `cookies()`, `headers()` or `draftMode()` is wrong before anybody asks
//! for it: the prerender bakes in the build machine's lack of a request, and
//! the route cache refuses to keep the render at run time and says
//! `x-uf-cache: BYPASS`. This refuses such a route at build time instead,
//! naming it, the API and the chain of imports that reaches the read. See
//! ubugeeei-prod/uf#996.
//!
//! The analysis is `uf_rsc`'s ([`RscGraph::request_state_read`]). What is
//! decided here is which routes it is asked about, because that takes the
//! route table and the rendering plan, and `uf_rsc` knows neither:
//!
//! * **Prerendered.** The plan prerenders routes, and the page has no
//!   parameters or exports `generateStaticParams`, and does not say
//!   `export const dynamic = "force-dynamic"`. That is the route decision
//!   `@uniflowed/vite`'s driver makes, read from the source rather than by
//!   evaluating the page.
//! * **Cached.** `rendering.cache.route` is on, the plan leaves a server behind
//!   to answer from the cache, and the render reaches a `cacheLife` import.
//!
//! A read reached through a layout is one diagnostic naming every route under
//! that layout, rather than one per route saying the same thing.
//!
//! # Except where the build can leave the read for the request
//!
//! When the build prerenders partially — `ppr` allowed, a server left behind,
//! routes rendered as Server Components; see
//! [`super::prerenders_partially`] — a prerendered route may read the request
//! inside a `<Suspense>` boundary: the build writes the page's static shell and
//! a server renders the boundary per request. Whether a read is inside a
//! boundary is a fact about the rendered tree, not about the import graph —
//! the boundary is often in the page and the read three modules away — so this
//! analysis cannot answer it, and it asks nothing about such routes. The
//! prerender answers instead, by rendering, and refuses a route whose read is
//! outside every boundary, naming the route and what it read. A route cache
//! that states a lifetime is still refused here: a document kept for every
//! request is about nobody, shell or not.

use camino::{Utf8Path, Utf8PathBuf};
use uf_config::{Prerender, RenderingPlan, UniflowedConfig};
use uf_router::{Route, RouteTarget, layout_chain};
use uf_rsc::{ModuleId, RscDiagnostic, RscGraph, StaticRouteReason, scan_exported_string};

use crate::support::relative_to;

/// Every read of the request in the render of a route whose document is
/// written once, as diagnostics.
pub(crate) fn request_state_in_static_routes(
    root: &Utf8Path,
    config: &UniflowedConfig,
    plan: RenderingPlan,
    target: RouteTarget,
    routes: &[Route],
    graph: &RscGraph,
) -> Vec<RscDiagnostic> {
    // A build that prerenders partially asks the render about its prerendered
    // routes rather than the graph; see the module documentation.
    let prerenders = matches!(
        plan.prerender(),
        Prerender::Everything | Prerender::Possible
    ) && !super::prerenders_partially(config, plan);
    let caches = config.app.rendering.cache.route && plan.emits_a_server();
    if !prerenders && !caches {
        return Vec::new();
    }
    let app_root = root.join(config.app.router.root.as_str());

    let mut found: Vec<RscDiagnostic> = Vec::new();
    for route in routes {
        let rendering: Vec<ModuleId> = layout_chain(&app_root, &route.directory, target)
            .iter()
            .chain(std::iter::once(&route.page))
            .filter_map(|path| graph.module_id(relative_to(root, path)))
            .collect();
        let Some(read) = graph.request_state_read(&rendering) else {
            continue;
        };
        let stated = if caches {
            graph.cache_lifetime_statement(&rendering)
        } else {
            None
        };
        let reason = if prerenders && is_prerendered(root, route, graph) {
            StaticRouteReason::Prerendered
        } else if let Some(stated) = stated {
            StaticRouteReason::Cached {
                module: path_of(graph, stated.module()),
                line: stated.site.line,
            }
        } else {
            continue;
        };

        let module = path_of(graph, read.module());
        let chain: Vec<Utf8PathBuf> = read.chain.iter().map(|id| path_of(graph, *id)).collect();
        let same = found.iter().position(|existing| {
            matches!(
                existing,
                RscDiagnostic::RequestStateInStaticRoute {
                    reason: existing_reason,
                    api,
                    module: existing_module,
                    line,
                    chain: existing_chain,
                    ..
                } if *existing_reason == reason
                    && *api == read.site.name
                    && *existing_module == module
                    && *line == read.site.line
                    && *existing_chain == chain
            )
        });
        match same {
            Some(index) => {
                if let RscDiagnostic::RequestStateInStaticRoute { routes, .. } = &mut found[index] {
                    routes.push(route.path.clone());
                }
            }
            None => found.push(RscDiagnostic::RequestStateInStaticRoute {
                routes: vec![route.path.clone()],
                reason,
                api: read.site.name.clone(),
                module,
                line: read.site.line,
                chain,
            }),
        }
    }

    for diagnostic in &mut found {
        if let RscDiagnostic::RequestStateInStaticRoute { routes, .. } = diagnostic {
            routes.sort();
        }
    }
    found
}

/// Whether the build prerenders `route`, read from its page's source.
///
/// A page that cannot be read now, although the analysis just read it, is
/// answered as not prerendered: a build is not refused on a guess.
fn is_prerendered(root: &Utf8Path, route: &Route, graph: &RscGraph) -> bool {
    let Ok(source) = std::fs::read_to_string(&route.page) else {
        return false;
    };
    if scan_exported_string(&source, "dynamic").as_deref() == Some("force-dynamic") {
        return false;
    }
    let exports = |name: &str| {
        graph
            .module(relative_to(root, &route.page))
            .is_some_and(|page| page.exports.iter().any(|export| export.name == name))
    };
    // A page that declares a schema for its query renders what the query says,
    // so `@uniflowed/vite`'s prerender leaves it to a server the way it does a
    // `force-dynamic` one. ubugeeei-prod/uf#1362.
    if exports("searchParams") {
        return false;
    }
    route.params.is_empty() || exports("generateStaticParams")
}

/// The project-relative path of a module in `graph`.
fn path_of(graph: &RscGraph, id: ModuleId) -> Utf8PathBuf {
    graph
        .module_by_id(id)
        .map(|module| module.path.clone())
        .unwrap_or_default()
}
