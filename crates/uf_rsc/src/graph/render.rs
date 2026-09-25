//! What a route's render reaches: a read of the request, or a stated lifetime.
//!
//! A document written once — at build time, or into the route cache — is a
//! document about no request in particular. A render that reads one, through
//! `cookies()`, `headers()` or `draftMode()`, is therefore wrong the moment it
//! is written. The route cache already refuses to keep such a render at run
//! time, and says `x-uf-cache: BYPASS`. These two questions let `uf build`
//! refuse the route before anything is written, naming the chain of imports
//! that reaches the read. See ubugeeei-prod/uf#996.
//!
//! # What "reaches" means
//!
//! The imports a render evaluates, starting from the modules that render a
//! route: its layouts and its page. Two kinds of module are not followed:
//!
//! * a `"use client"` module. The server graph does not own its imports, and it
//!   may not import `@uniflowed/server` at all, which is
//!   `rsc/server-only-import-in-client`;
//! * a `"use server"` module. Its functions run when the browser calls them as
//!   actions, not while the page renders, so a page that imports an action to
//!   hand to a form has not read the request by doing so.
//!
//! A read is an import binding rather than a call, because this crate is a
//! lexer: a module that imports `cookies` is taken to read cookies. A namespace
//! import of `@uniflowed/server` binds no single name and is not counted, and
//! neither is a dynamic `import()`. The run-time refusal stays behind both.

use compact_str::CompactString;

use crate::directive::ModuleEnvironment;
use crate::scan::{ImportSpecifier, ImportedName};

use super::{ModuleId, RscGraph};

/// The package whose request APIs make a render about one request.
pub const REQUEST_STATE_PACKAGE: &str = "@uniflowed/server";

/// The request APIs of [`REQUEST_STATE_PACKAGE`], sorted.
pub const REQUEST_STATE_APIS: &[&str] = &["cookies", "draftMode", "headers"];

/// The package a render states its cache lifetime through.
pub const CACHE_LIFETIME_PACKAGE: &str = "@uniflowed/server/cache";

/// The binding of [`CACHE_LIFETIME_PACKAGE`] that states a lifetime.
pub const CACHE_LIFETIME_API: &str = "cacheLife";

/// [`CACHE_LIFETIME_API`] as the list [`import_sites`] matches against.
const CACHE_LIFETIME_APIS: &[&str] = &[CACHE_LIFETIME_API];

/// One import of a name, and the line it is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSite {
    /// The exported name imported, e.g. `cookies`.
    pub name: CompactString,
    /// 1-based line of the import.
    pub line: u32,
}

/// The nearest module a render reaches that imports what was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderReach {
    /// What that module imports, and where.
    pub site: ImportSite,
    /// The modules from one that renders the route down to the importer, both
    /// included, each importing the next. Never empty; one module long when a
    /// module that renders the route imports the name itself.
    pub chain: Vec<ModuleId>,
}

impl RenderReach {
    /// The module that imports the name, which is the last one in the chain.
    pub fn module(&self) -> ModuleId {
        *self
            .chain
            .last()
            .expect("a chain names at least the module that imports the name")
    }
}

/// The request APIs, and the lifetime statement, that a module's imports bind.
///
/// Only named bindings count, re-exports included: `export { cookies } from
/// "@uniflowed/server"` hands the read to whatever imports the re-export. The
/// first `cacheLife` import is kept, because one is enough to say the render
/// states a lifetime.
pub(crate) fn import_sites(external: &[ImportSpecifier]) -> (Vec<ImportSite>, Option<ImportSite>) {
    let mut reads = Vec::new();
    let mut lifetime = None;
    for import in external {
        let (wanted, states_lifetime) = match import.specifier.as_str() {
            REQUEST_STATE_PACKAGE => (REQUEST_STATE_APIS, false),
            CACHE_LIFETIME_PACKAGE => (CACHE_LIFETIME_APIS, true),
            _ => continue,
        };
        for binding in &import.bindings {
            let ImportedName::Named(name) = &binding.imported else {
                continue;
            };
            if !wanted.contains(&name.as_str()) {
                continue;
            }
            let site = ImportSite {
                name: name.clone(),
                line: import.line,
            };
            if states_lifetime {
                lifetime.get_or_insert(site);
            } else {
                reads.push(site);
            }
        }
    }
    (reads, lifetime)
}

impl RscGraph {
    /// The nearest read of the request that a render starting at `from` reaches.
    ///
    /// `from` is every module that renders a route: its layouts, outermost
    /// first, then its page. The search starts from all of them at once, so the
    /// chain it names is a shortest one, and the order of `from` decides between
    /// chains of the same length, so the same route reports the same chain on
    /// every run. `None` when nothing it reaches imports `cookies`, `headers`
    /// or `draftMode` from `@uniflowed/server`.
    pub fn request_state_read(&self, from: &[ModuleId]) -> Option<RenderReach> {
        self.nearest_in_render(from, |module| module.request_state_imports.first().cloned())
    }

    /// The nearest `cacheLife` import that a render starting at `from` reaches.
    ///
    /// Read the same way as [`Self::request_state_read`].
    pub fn cache_lifetime_statement(&self, from: &[ModuleId]) -> Option<RenderReach> {
        self.nearest_in_render(from, |module| module.cache_lifetime_import.clone())
    }

    /// Breadth-first from every module in `from`, over the imports a render
    /// evaluates, to the first module `found` answers for.
    ///
    /// The same shape as [`Self::client_bundle_reason`], and for the same
    /// reasons: `O(V + E)` on a cyclic graph, and a chain rebuilt from one
    /// predecessor per module is acyclic by construction.
    fn nearest_in_render(
        &self,
        from: &[ModuleId],
        found: impl Fn(&super::RscModule) -> Option<ImportSite>,
    ) -> Option<RenderReach> {
        const NONE: u32 = u32::MAX;
        let mut predecessor = vec![NONE; self.modules.len()];
        let mut seen = vec![false; self.modules.len()];
        let mut work = std::collections::VecDeque::new();
        for &source in from {
            if source.index() < self.modules.len()
                && !seen[source.index()]
                && self.renders_on_the_server(source)
            {
                seen[source.index()] = true;
                work.push_back(source);
            }
        }

        while let Some(current) = work.pop_front() {
            if let Some(site) = found(&self.modules[current.index()]) {
                let mut chain = vec![current];
                let mut at = current;
                while predecessor[at.index()] != NONE {
                    at = ModuleId(predecessor[at.index()]);
                    chain.push(at);
                }
                chain.reverse();
                return Some(RenderReach { site, chain });
            }
            for target in self.modules[current.index()].render_imports.iter().copied() {
                if seen[target.index()] || !self.renders_on_the_server(target) {
                    continue;
                }
                seen[target.index()] = true;
                predecessor[target.index()] = current.0;
                work.push_back(target);
            }
        }
        None
    }

    /// Whether a server render evaluates this module's imports; see the header.
    fn renders_on_the_server(&self, id: ModuleId) -> bool {
        self.modules[id.index()].environment == ModuleEnvironment::Server
    }
}
