//! Prerendered documents that a `_uf.middleware.js` never sees.
//!
//! `uf build` writes an HTML file for every route it can render without a
//! request, and a host serves that file to whoever asks for it. A
//! `_uf.middleware.js` is the other thing: code that runs on a server, once
//! per request, before the route resolves — redirects, headers, and the
//! session check the routing guide names first. A route that is both is a
//! route whose guard applies to one half of what this build produced and not
//! to the other, and `dist/` carries both halves.
//!
//! # Why this warns instead of refusing
//!
//! The tension is real rather than a mistake anyone made. Prerendering a page
//! and guarding it are both things a project is entitled to ask for, and every
//! way of resolving it in the build needs a fact the build does not have:
//!
//! * **Refuse to prerender a guarded route.** Correct for a project that
//!   deploys `dist/` to a CDN, and wrong for one that deploys the server
//!   bundle beside it, where the guard does run and the prerendered document
//!   is a cache. `uf build` writes both, and `uf start`, `uf preview` and a
//!   static host are three different answers to which one ships.
//! * **Skip the route and leave it to the server.** The same missing fact with
//!   the damage reversed: it deletes the file the CDN deployment needs, and it
//!   does so quietly, which is the shape of failure this check exists to
//!   report.
//! * **Say so.** True under every deployment: the file is in `dist/`, the
//!   guard is not in the file, and the person about to deploy is the one who
//!   knows which of the two is being served.
//!
//! `build.staticBuild` is the setting that decides it, and since
//! ubugeeei-prod/uf#385 it does. A project that has declared it ships static
//! files and nothing else has asked for two things that cannot both be true,
//! so a middleware under such a build is a **refusal** rather than a row in
//! this table — and the same is true of `rendering.modes: ["ssg"]`, which is
//! the same declaration about the routes instead of about the artefact.
//!
//! That refusal is not made here, and the reason is the shape of one message
//! rather than a preference. Three things need a server — a middleware, a
//! route handler, and a route that cannot be prerendered — and only the
//! builder knows all three: whether a page exports `generateStaticParams` is a
//! property of the module, and `uf` does not evaluate modules. Refusing the
//! middleware here and the other two in the builder would tell a project with
//! both to fix one, rebuild, and be told about the next; so the whole list is
//! refused in one place, and this table is reached only by a build that emits
//! a server. See `renderingPlan` in `@uniflowed/vite`'s `driver.js`, and
//! `uf_config`'s `RenderingPlan` for the decision it is handed.
//!
//! What is not optional is saying something. `docs/security.md` and #260 are
//! both about the same failure: an authorisation check that looks enforced and
//! is not. Documentation is where this is written down today, and a fact that
//! only exists in a guide is a fact about the build that the build refuses to
//! state.

use camino::Utf8Path;
use uf_router::Route;

use super::Prerendered;
use crate::support::relative_to;

/// One prerendered document, and the guards that do not run for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnguardedPage {
    /// The URL the prerender wrote a file for.
    pub(crate) url: String,
    /// The file it wrote, relative to the project root.
    pub(crate) file: String,
    /// The middleware guarding that route, outermost first, relative to the
    /// project root.
    pub(crate) middleware: Vec<String>,
}

/// Every page Vite prerendered whose route a middleware guards.
///
/// Driven from the files the build actually wrote rather than from the route
/// table alone, because the route table is what *could* be prerendered and the
/// pages are what shipped — including the ones a `generateStaticParams`
/// produced, which are the guarded documents easiest to forget exist.
pub(crate) fn unguarded_pages(
    root: &Utf8Path,
    routes: &[Route],
    pages: &[Prerendered],
) -> Vec<UnguardedPage> {
    if !routes.iter().any(Route::is_guarded) {
        return Vec::new();
    }

    let mut found = Vec::new();
    for Prerendered { url, file, .. } in pages {
        // The most specific route wins, the way the router resolves a request:
        // `/posts/new` is `/posts/new` and not `/posts/:slug` when both exist.
        // Chosen over *every* route and then asked whether it is guarded,
        // rather than over the guarded ones: a `(group)` segment is dropped
        // from a route path but not from the directory tree, so
        // `app/(marketing)/posts/new/` serves `/posts/new` and inherits
        // nothing from `app/posts/_uf.middleware.js`. Searching the guarded
        // routes alone would have named the guard that does not apply, which
        // is a report about a file that is served exactly as intended.
        //
        // "Most specific" is `Route::specificity`, which copies the numbers
        // out of `packages/router/internal/runtime.js`'s `specificity`. That
        // function is the source of truth: it is the one that picks the route
        // for a real request, and this warning is a claim about what it will
        // pick. Counting literal segments was the earlier answer and is a
        // different one — `/posts/:a/:b/edit` and `/posts/archive/:z*` have
        // two literals each and the runtime prefers the first — so it named
        // the guards of a route that never answers.
        let Some(route) = routes
            .iter()
            .filter(|route| route.matches_url(url))
            .max_by_key(|route| route.specificity())
        else {
            continue;
        };
        if !route.is_guarded() {
            continue;
        }
        found.push(UnguardedPage {
            url: url.clone(),
            file: file.clone(),
            middleware: route
                .middleware
                .iter()
                .map(|path| relative_to(root, path))
                .collect(),
        });
    }
    found.sort_by(|a, b| a.url.cmp(&b.url));
    found
}

#[cfg(test)]
mod tests;
