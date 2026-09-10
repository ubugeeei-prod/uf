//! What a static host cannot serve, said out loud instead of dropped.
//!
//! `--adapter static` is the one target that emits nothing new: `uf build`
//! already writes `dist/`, and a static host is a thing that returns files
//! from it. So the whole of this adapter is the sentence it refuses with, and
//! that sentence is the point rather than a guard around the feature —
//! `ubugeeei-redundancy.md` says a static host does not become a server merely
//! because an adapter exists, and that an unsupported configuration is
//! rejected clearly rather than silently changing semantics. Before this
//! module, a project with a route handler and a parameterised route built
//! `dist/`, uploaded it, and discovered which half of itself had disappeared
//! from production. That is item 4 of ubugeeei-prod/uf#335.
//!
//! # Four things need a server, and each is found somewhere different
//!
//! | what | where it is found | why a file is not it |
//! | --- | --- | --- |
//! | a route handler | [`uf_router::discover_server_modules`] | it answers a request; a `POST` has no file behind it |
//! | a middleware | the same walk | it runs *before* the route resolves, once per request |
//! | a route the prerender wrote no document for | the route table against the pages Vite reported | there is nothing to upload for it |
//! | a server action | the RSC registry | it is a `POST` the browser makes back to the application |
//!
//! Two of those four are invisible to the route table, which is why
//! [`uf_router::discover_server_modules`] exists: a `$route.js` has no page
//! and is therefore in no [`Route`], and a `$middleware.js` reaches
//! [`Route::middleware`] only for routes that have a page under it — a project
//! may guard a subtree it has not written a page in yet.
//!
//! # Why this reports after the build rather than before it
//!
//! [`super::resolve`] refuses an adapter nobody wrote before Vite starts,
//! because that answer is available without building. This one is not: the
//! third row above is "the prerender wrote no document", and the only thing
//! that knows what the prerender wrote is the prerender. Splitting it — the
//! handlers before the build, the routes after — would tell a reader about
//! half of their project, take a fix, and then tell them about the other half.
//! One list, once.

use camino::Utf8Path;
use uf_router::{Route, ServerModule, ServerModuleKind};

use crate::commands::build::Prerendered;
use crate::support::relative_to;

/// Why a static host has no answer for something in this project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// A `$route.js`.
    RouteHandler,
    /// A `$middleware.js`.
    Middleware,
    /// A route with parameters that no `generateStaticParams` enumerated.
    NoStaticParams,
    /// A route with no parameters that the prerender still wrote no file for.
    NotPrerendered,
    /// A `"use server"` export the browser can dial.
    ServerAction,
}

impl Reason {
    /// The clause that goes after the subject, in one line.
    ///
    /// Written per reason rather than per finding so that a project with
    /// thirty parameterised routes reads as one problem thirty times and not
    /// as thirty problems — and so the sentence a reader acts on is written
    /// once, here, rather than in whichever branch produced the row.
    const fn because(self) -> &'static str {
        match self {
            Self::RouteHandler => {
                "a route handler answers a request, and a static host has only files"
            }
            Self::Middleware => {
                "a middleware runs once per request, before the route resolves, and nothing \
                 runs on a static host"
            }
            Self::NoStaticParams => {
                "the build wrote no document for it: it has parameters and no \
                 `generateStaticParams` to enumerate them"
            }
            Self::NotPrerendered => "the build wrote no document for it",
            Self::ServerAction => {
                "a server action is a `POST` the browser makes back to the application"
            }
        }
    }
}

/// One thing this project does that a static host cannot do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unservable {
    /// What it is called: a URL for a route, an export name for an action.
    pub(crate) subject: String,
    /// The file that says so, relative to the project root.
    pub(crate) file: String,
    /// Which of the five it is.
    pub(crate) reason: Reason,
}

/// Everything in this project a static host has no answer for.
///
/// Ordered by kind and then by name, rather than by the order each was found,
/// because the list is read: the two that are a whole module come first, then
/// the routes, then the actions, and a reader fixing them from the top works
/// through one kind of change at a time.
///
/// `actions` is the callable set rather than every `"use server"` export. An
/// action no client boundary can reach is not an endpoint — it is a function
/// the server calls itself — so listing it would be refusing a project for
/// something that never becomes a request.
pub(crate) fn unservable(
    root: &Utf8Path,
    routes: &[Route],
    modules: &[ServerModule],
    pages: &[Prerendered],
    actions: &[(String, camino::Utf8PathBuf)],
) -> Vec<Unservable> {
    let mut found = Vec::new();

    for module in modules {
        found.push(Unservable {
            subject: module.path.to_string(),
            file: relative_to(root, &module.file),
            reason: match module.kind {
                ServerModuleKind::RouteHandler => Reason::RouteHandler,
                ServerModuleKind::Middleware => Reason::Middleware,
            },
        });
    }

    for route in routes {
        // Any document at all, not one per URL: a `generateStaticParams` that
        // enumerated three of a route's slugs wrote three files, and the route
        // is served for those three. Whether it enumerated *every* slug is a
        // question about the project's data that neither the build nor this
        // can ask — which is why the check is "the prerender produced nothing
        // here" rather than "the prerender produced everything".
        if pages.iter().any(|page| route.matches_url(&page.url)) {
            continue;
        }
        found.push(Unservable {
            subject: route.path.to_string(),
            file: relative_to(root, &route.page),
            reason: if route.params.is_empty() {
                Reason::NotPrerendered
            } else {
                Reason::NoStaticParams
            },
        });
    }

    for (export, module) in actions {
        found.push(Unservable {
            subject: export.clone(),
            file: relative_to(root, module),
            reason: Reason::ServerAction,
        });
    }

    found
}

/// How many findings a message names before it stops and says how many are
/// left.
///
/// A refusal a reader scrolls past is a refusal they act on by guessing. Ten
/// is enough to see the shape of the problem — and the count that follows is
/// what says the shape is not the whole of it.
const SHOWN: usize = 10;

/// The refusal, as one message.
///
/// It names what, where and why for each finding, and then what to do instead,
/// because "this cannot be served statically" is only half an answer to
/// somebody who chose this target on purpose. The other half is that uf has
/// four targets that *can* serve it and the project does not have to change to
/// use one.
pub(crate) fn refusal(findings: &[Unservable]) -> String {
    let mut message = format!(
        "this project cannot be served by a static host, so `uf build --adapter static` \
         would have written a directory that answers {} of it and silently dropped the rest",
        if findings.len() == 1 { "most" } else { "part" }
    );
    for finding in findings.iter().take(SHOWN) {
        message.push_str(&format!(
            "\n  {} ({}) — {}",
            finding.subject,
            finding.file,
            finding.reason.because()
        ));
    }
    if findings.len() > SHOWN {
        message.push_str(&format!(
            "\n  … and {} more",
            findings.len().saturating_sub(SHOWN)
        ));
    }
    message.push_str(
        "\n  Serve it with `uf build --adapter node`, `--adapter container`, \
         `--adapter edge` or `--adapter serverless` — the application is the same file in \
         all four — or remove what needs a server and build again.\n  \
         https://github.com/ubugeeei-prod/uf/issues/335",
    );
    message
}

#[cfg(test)]
mod tests;
