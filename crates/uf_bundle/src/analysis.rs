//! Per-route bundle analysis: which modules each route ships to the browser
//! and runs on the server, what each weighs, and the chain of imports that put
//! it there.
//!
//! # Who knows what
//!
//! The bundler knows which module went into which chunk and what each module
//! imports; uf knows the route table. So the builder writes down the first as a
//! [`ModuleGraph`] — `uf-module-graph.json` — and this module does the rest:
//! attributes modules to routes, finds the chains, measures, and writes
//! `uf-bundle-analysis.json` with a static HTML view beside it. Nothing here
//! knows Vite, which is what keeps `build.builder` replaceable (red line 3): a
//! builder that writes the same graph gets the same analysis.
//!
//! # How a module is attributed
//!
//! A route's own files are its page and every other router file — a name
//! starting with `$`: layouts, loading and error boundaries, templates,
//! middleware — in its directory and the directories above it. A router file
//! that is not a route's own belongs to some other route, and a walk for this
//! route never enters it.
//!
//! A **client reference** is an entry of the client build that is not one of
//! its shared entries: a module the browser loads because a server component
//! named it. The client entry reaches every one of them — a registry that
//! `import()`s each by URL is how the browser finds a component it was sent —
//! and reaching one that way says nothing about which route renders it.
//!
//! 1. **Shared.** For each build, a breadth-first walk from the modules every
//!    page of it loads, entering no router file and no client reference. What
//!    it reaches, every route pays for, so it is listed once rather than under
//!    every route.
//! 2. **References.** For each route, a walk of every server-side build from
//!    the route's own files, which stops at what is shared. The client
//!    references it reaches are the ones the route renders, and the shortest
//!    chain to each is remembered.
//! 3. **Per route.** For each route and each build, the same walk, seeded as
//!    well with every reference step 2 found in some *other* build. That is how
//!    a chain crosses from a server component into the browser, and into a
//!    server-rendering bundle that runs the same component, without this
//!    module knowing what a server component is.
//!
//! Every walk is breadth-first, so each chain is a shortest one, and seeds are
//! taken page first, so a module a page and its layout both reach is explained
//! through the page.
//!
//! # What the sizes are
//!
//! Each module's code as the bundler rendered it into its chunk — before the
//! chunk is minified — compressed alone at the report's fixed settings. They
//! rank modules and explain a chunk. What a shipped file weighs is
//! `uf-bundle-report.json`'s answer, and the two are not meant to add up.

#[cfg(test)]
mod tests;

use std::collections::VecDeque;
use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uf_infra::{FxHashMap, FxHashSet};

use crate::size::{AssetSize, BROTLI_QUALITY, GZIP_LEVEL, MeasureError, measure};

/// The file a builder writes its module graph to, in the build's metadata
/// directory.
pub const MODULE_GRAPH_FILE: &str = "uf-module-graph.json";

/// The version of [`ModuleGraph`] this module reads.
pub const MODULE_GRAPH_VERSION: u32 = 1;

/// The analysis, in the build's metadata directory.
pub const ANALYSIS_FILE: &str = "uf-bundle-analysis.json";

/// The static HTML view of the analysis, beside it.
pub const ANALYSIS_VIEW_FILE: &str = "uf-bundle-analysis.html";

/// The build a browser loads. Every other build in a graph runs on a server.
pub const CLIENT_BUILD: &str = "client";

/// The page the view is, with [`VIEW_DATA`] where the analysis goes.
const VIEW: &str = include_str!("analysis/view.html");

/// The placeholder in [`VIEW`].
const VIEW_DATA: &str = "__UF_BUNDLE_ANALYSIS__";

/// Router file names that name one route rather than a directory's worth of
/// them, so an ancestor's copy is never a descendant route's own.
const ONE_ROUTE: [&str; 3] = ["$page", "$route", "$default"];

/// What a builder writes: every bundle it produced, the modules each was built
/// from, and the chunks their code went into.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleGraph {
    /// [`MODULE_GRAPH_VERSION`], for a graph this module can read.
    pub version: u32,
    /// One per bundle.
    #[serde(default)]
    pub builds: Vec<GraphBuild>,
}

/// One bundle of a [`ModuleGraph`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphBuild {
    /// The environment it was built for: [`CLIENT_BUILD`] for the browser, and
    /// any other name for a bundle that runs on a server.
    pub environment: CompactString,
    /// The modules every page of this bundle loads.
    #[serde(default)]
    pub entries: Vec<String>,
    /// Every module the bundle was built from.
    #[serde(default)]
    pub modules: Vec<GraphModule>,
    /// Every chunk it emitted.
    #[serde(default)]
    pub chunks: Vec<GraphChunk>,
}

/// One module and what it imports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphModule {
    /// Relative to the project root with `/`; a virtual module keeps its id.
    pub id: String,
    /// What it imports statically, as module ids.
    #[serde(default)]
    pub imports: Vec<String>,
    /// What it imports with `import()`, as module ids.
    #[serde(default)]
    pub dynamic_imports: Vec<String>,
}

/// One emitted chunk.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphChunk {
    /// The file, relative to the bundle's output directory.
    pub file: CompactString,
    /// The module this chunk is an entry for, when it is an entry chunk.
    #[serde(default)]
    pub facade: Option<String>,
    /// The modules whose code is in it.
    #[serde(default)]
    pub modules: Vec<ChunkModule>,
}

/// One module's code inside a chunk.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkModule {
    /// The module's id.
    pub id: String,
    /// Its code as the bundler rendered it into the chunk.
    #[serde(default)]
    pub code: String,
}

/// A route, as the analysis needs it: its path, its page and its directory,
/// both relative to the project root with `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFiles {
    /// The route, as the router prints it.
    pub path: CompactString,
    /// Its page module's id.
    pub page: String,
    /// The directory its page is in.
    pub directory: String,
}

/// Which side of the network a build runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    /// The browser.
    Client,
    /// A server.
    Server,
}

/// The whole analysis: what `uf-bundle-analysis.json` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundleAnalysis {
    /// One per bundle, in the order the graph listed them.
    pub builds: Vec<BuildSummary>,
    /// What every route loads whatever it is.
    pub shared: Sides,
    /// What each route loads beyond that, in the order the routes were given.
    pub routes: Vec<RouteAnalysis>,
}

/// One bundle's size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSummary {
    /// The environment it was built for.
    pub name: CompactString,
    /// Where it runs.
    pub side: Side,
    /// How many modules it emitted code for.
    pub modules: usize,
    /// Their sizes added together.
    pub size: AssetSize,
}

/// A client list and a server list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sides {
    /// Modules the browser loads.
    pub client: ModuleList,
    /// Modules a server runs.
    pub server: ModuleList,
}

/// Modules, largest gzip first, and their sizes added together.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleList {
    /// Every module's size added together.
    pub size: AssetSize,
    /// The modules.
    pub modules: Vec<ModuleEntry>,
}

/// One route's modules beyond what is shared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteAnalysis {
    /// The route.
    pub path: CompactString,
    /// Its own files that are in some bundle, page first.
    pub files: Vec<String>,
    /// What the browser loads for it beyond `shared.client`.
    pub client: ModuleList,
    /// What a server runs for it beyond `shared.server`.
    pub server: ModuleList,
    /// What it costs on each side, shared modules included.
    pub total: RouteTotal,
}

/// A route's cost on each side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct RouteTotal {
    /// In the browser.
    pub client: AssetSize,
    /// On a server.
    pub server: AssetSize,
}

/// One module in one bundle, and why it is there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleEntry {
    /// The module's id.
    pub id: String,
    /// The bundle that emitted it.
    pub build: CompactString,
    /// The chunk its code is in.
    pub chunk: CompactString,
    /// Its rendered code's size.
    pub size: AssetSize,
    /// Module ids from a route's own file — or, under `shared`, a bundle's
    /// entry — to this module.
    pub chain: Vec<String>,
    /// Whether the chain crosses an `import()`.
    pub dynamic: bool,
    /// How many routes list it; every route, under `shared`.
    pub routes: usize,
}

/// Anything that stops an analysis.
#[derive(Debug, Error)]
pub enum AnalysisError {
    /// The module graph could not be read.
    #[error("failed to read {path}: {source}")]
    Read {
        /// The graph's path.
        path: Utf8PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
    /// The module graph is not JSON of the expected shape.
    #[error("{path} is not a module graph uf can read: {message}")]
    Parse {
        /// The graph's path.
        path: Utf8PathBuf,
        /// What the parser said.
        message: String,
    },
    /// The module graph is a version this module does not read.
    #[error(
        "the module graph is version {found}, and this uf reads version {MODULE_GRAPH_VERSION}"
    )]
    Version {
        /// The version it declared.
        found: u32,
    },
    /// A module's code could not be measured.
    #[error("failed to measure `{id}`: {source}")]
    Measure {
        /// The module.
        id: String,
        /// Why.
        #[source]
        source: MeasureError,
    },
    /// The analysis could not be serialized.
    #[error("failed to serialize the bundle analysis: {0}")]
    Serialize(#[source] serde_json::Error),
    /// A file could not be written.
    #[error("failed to write {path}: {source}")]
    Write {
        /// The file.
        path: Utf8PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
}

/// Read the graph a builder wrote.
pub fn read_module_graph(path: &Utf8Path) -> Result<ModuleGraph, AnalysisError> {
    let text = fs::read_to_string(path).map_err(|source| AnalysisError::Read {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|error| AnalysisError::Parse {
        path: path.to_owned(),
        message: error.to_string(),
    })
}

/// Attribute every module in `graph` to the routes that load it.
pub fn analyze(
    graph: &ModuleGraph,
    routes: &[RouteFiles],
) -> Result<BundleAnalysis, AnalysisError> {
    if graph.version != MODULE_GRAPH_VERSION {
        return Err(AnalysisError::Version {
            found: graph.version,
        });
    }
    let builds = index(graph)?;
    let references: FxHashSet<&str> = builds
        .iter()
        .filter(|build| build.side == Side::Client)
        .flat_map(|build| build.references.iter().copied())
        .collect();
    let unbridged = FxHashMap::default();

    // 1. What every page of each build loads.
    let shared: Vec<Walk<'_>> = builds
        .iter()
        .map(|build| {
            Walk::new(&build.edges, &build.entries, &unbridged, |id| {
                !is_router_file(id) && !references.contains(id)
            })
        })
        .collect();
    let mut shared_lists = (Vec::new(), Vec::new());
    for (build, walk) in builds.iter().zip(&shared) {
        let listed = list(build, walk, &unbridged);
        match build.side {
            Side::Client => shared_lists.0.extend(listed),
            Side::Server => shared_lists.1.extend(listed),
        }
    }

    let mut per_route = Vec::with_capacity(routes.len());
    for route in routes {
        // 2. The client references this route's server-side builds render.
        let mut bridged: FxHashMap<&str, Bridge<'_>> = FxHashMap::default();
        for (index, build) in builds.iter().enumerate() {
            if build.side != Side::Server {
                continue;
            }
            let seeds = own_files(build, route);
            let walk = Walk::new(
                &build.edges,
                &seeds,
                &unbridged,
                enters(route, &shared[index]),
            );
            for &id in walk.order.iter().filter(|id| references.contains(**id)) {
                let chain = walk.chain(id, &unbridged);
                if bridged
                    .get(id)
                    .is_none_or(|existing| chain.len() < existing.chain.len())
                {
                    bridged.insert(
                        id,
                        Bridge {
                            dynamic: walk.dynamic(id),
                            chain,
                            found_in: index,
                        },
                    );
                }
            }
        }

        // 3. Everything the route loads, in every build.
        let mut files: Vec<&str> = Vec::new();
        let mut client = Vec::new();
        let mut server = Vec::new();
        for (index, build) in builds.iter().enumerate() {
            let mut seeds = own_files(build, route);
            files.extend(seeds.iter().copied());
            let prefixes = bridge_into(index, build, &bridged, &mut seeds);
            let walk = Walk::new(
                &build.edges,
                &seeds,
                &prefixes,
                enters(route, &shared[index]),
            );
            let listed = list(build, &walk, &prefixes);
            match build.side {
                Side::Client => client.extend(listed),
                Side::Server => server.extend(listed),
            }
        }
        files.sort_unstable_by_key(|id| (*id != route.page, *id));
        files.dedup();
        per_route.push((route, files, client, server));
    }

    let mut counts: FxHashMap<(CompactString, String), usize> = FxHashMap::default();
    for (_, _, client, server) in &per_route {
        for entry in client.iter().chain(server) {
            *counts
                .entry((entry.build.clone(), entry.id.clone()))
                .or_default() += 1;
        }
    }
    let every_route = |mut modules: Vec<ModuleEntry>| {
        for entry in &mut modules {
            entry.routes = routes.len();
        }
        finish(modules)
    };
    let shared = Sides {
        client: every_route(shared_lists.0),
        server: every_route(shared_lists.1),
    };
    let counted = |mut modules: Vec<ModuleEntry>| {
        for entry in &mut modules {
            entry.routes = counts
                .get(&(entry.build.clone(), entry.id.clone()))
                .copied()
                .unwrap_or(0);
        }
        finish(modules)
    };
    let routes = per_route
        .into_iter()
        .map(|(route, files, client, server)| {
            let client = counted(client);
            let server = counted(server);
            RouteAnalysis {
                path: route.path.clone(),
                files: files.into_iter().map(str::to_owned).collect(),
                total: RouteTotal {
                    client: shared.client.size.saturating_add(client.size),
                    server: shared.server.size.saturating_add(server.size),
                },
                client,
                server,
            }
        })
        .collect();

    Ok(BundleAnalysis {
        builds: builds
            .iter()
            .map(|build| BuildSummary {
                name: build.name.clone(),
                side: build.side,
                modules: build.emitted.len(),
                size: build
                    .emitted
                    .values()
                    .fold(AssetSize::zero(), |total, (_, size)| {
                        total.saturating_add(*size)
                    }),
            })
            .collect(),
        shared,
        routes,
    })
}

/// Write `uf-bundle-analysis.json` and `uf-bundle-analysis.html` into `dir`,
/// returning both paths.
pub fn write_analysis(
    dir: &Utf8Path,
    analysis: &BundleAnalysis,
) -> Result<(Utf8PathBuf, Utf8PathBuf), AnalysisError> {
    let file = AnalysisFile {
        version: 1,
        gzip_level: GZIP_LEVEL,
        brotli_quality: BROTLI_QUALITY,
        analysis,
    };
    let mut pretty = serde_json::to_string_pretty(&file).map_err(AnalysisError::Serialize)?;
    pretty.push('\n');
    let json = dir.join(ANALYSIS_FILE);
    fs::write(&json, pretty).map_err(|source| AnalysisError::Write {
        path: json.clone(),
        source,
    })?;
    let compact = serde_json::to_string(&file).map_err(AnalysisError::Serialize)?;
    let view = dir.join(ANALYSIS_VIEW_FILE);
    fs::write(&view, render_view(&compact)).map_err(|source| AnalysisError::Write {
        path: view.clone(),
        source,
    })?;
    Ok((json, view))
}

/// The HTML view with `json` — the analysis file's contents — inside it.
///
/// The JSON goes into a `<script type="application/json">`, where the one
/// thing it must not contain is the text that closes the element. Every `<`,
/// `>` and `&` is written as its `\u` escape, which JSON reads back as the same
/// character and HTML does not read at all, so a module id that happens to
/// spell `</script>` stays a module id. They can only occur inside strings,
/// where the escape is legal. U+2028 and U+2029 are escaped as well, for a
/// reader that evaluates the text as script rather than parsing it.
#[must_use]
pub fn render_view(json: &str) -> String {
    let mut escaped = String::with_capacity(json.len());
    for character in json.chars() {
        match character {
            '<' => escaped.push_str("\\u003c"),
            '>' => escaped.push_str("\\u003e"),
            '&' => escaped.push_str("\\u0026"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            other => escaped.push(other),
        }
    }
    VIEW.replacen(VIEW_DATA, &escaped, 1)
}

/// The on-disk shape: the compression settings travel with the numbers, as
/// they do in `uf-bundle-report.json`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AnalysisFile<'a> {
    version: u8,
    gzip_level: u32,
    brotli_quality: u32,
    #[serde(flatten)]
    analysis: &'a BundleAnalysis,
}

/// One import.
#[derive(Debug, Clone, Copy)]
struct Edge<'g> {
    to: &'g str,
    dynamic: bool,
}

/// One build, indexed for walking.
struct Indexed<'g> {
    name: &'g CompactString,
    side: Side,
    edges: FxHashMap<&'g str, Vec<Edge<'g>>>,
    /// Every module the build has code for: the chunk it is in, and its size.
    emitted: FxHashMap<&'g str, (&'g CompactString, AssetSize)>,
    /// The shared entries, sorted.
    entries: Vec<&'g str>,
    /// Entry chunks' modules that are not shared entries.
    references: FxHashSet<&'g str>,
    /// Every id the build mentions, sorted, for finding a route's files.
    ids: Vec<&'g str>,
}

/// Index every build, measuring every module's code once.
///
/// The measuring is the cost of an analysis — brotli at quality 11 — and no
/// module's size depends on another's, so it fans out across threads.
fn index(graph: &ModuleGraph) -> Result<Vec<Indexed<'_>>, AnalysisError> {
    let mut pending: Vec<(usize, &str, &CompactString, &str)> = Vec::new();
    let mut seen: FxHashSet<(usize, &str)> = FxHashSet::default();
    for (index, build) in graph.builds.iter().enumerate() {
        for chunk in &build.chunks {
            for module in &chunk.modules {
                if !module.code.is_empty() && seen.insert((index, module.id.as_str())) {
                    pending.push((index, &module.id, &chunk.file, &module.code));
                }
            }
        }
    }
    let sizes = uf_infra::parallel::map(&pending, |(_, id, _, code)| {
        measure(code.as_bytes()).map_err(|source| AnalysisError::Measure {
            id: (*id).to_owned(),
            source,
        })
    })?;

    let mut indexed: Vec<Indexed<'_>> = graph
        .builds
        .iter()
        .map(|build| {
            let mut edges: FxHashMap<&str, Vec<Edge<'_>>> = FxHashMap::default();
            let mut ids: FxHashSet<&str> = FxHashSet::default();
            for module in &build.modules {
                ids.insert(&module.id);
                let list = edges.entry(&module.id).or_default();
                list.extend(module.imports.iter().map(|to| Edge { to, dynamic: false }));
                list.extend(
                    module
                        .dynamic_imports
                        .iter()
                        .map(|to| Edge { to, dynamic: true }),
                );
            }
            for chunk in &build.chunks {
                ids.extend(chunk.modules.iter().map(|module| module.id.as_str()));
            }
            let mut entries: Vec<&str> = build.entries.iter().map(String::as_str).collect();
            entries.sort_unstable();
            entries.dedup();
            let references = build
                .chunks
                .iter()
                .filter_map(|chunk| chunk.facade.as_deref())
                .filter(|facade| entries.binary_search(facade).is_err())
                .collect();
            let mut ids: Vec<&str> = ids.into_iter().collect();
            ids.sort_unstable();
            Indexed {
                name: &build.environment,
                side: if build.environment == CLIENT_BUILD {
                    Side::Client
                } else {
                    Side::Server
                },
                edges,
                emitted: FxHashMap::default(),
                entries,
                references,
                ids,
            }
        })
        .collect();
    for ((index, id, chunk, _), size) in pending.into_iter().zip(sizes) {
        indexed[index].emitted.insert(id, (chunk, size));
    }
    Ok(indexed)
}

/// How a walk reached one module.
#[derive(Debug, Clone, Copy)]
struct Step<'g> {
    from: Option<&'g str>,
    dynamic: bool,
}

/// A client reference a server-side walk reached, and how.
#[derive(Debug, Clone)]
struct Bridge<'g> {
    /// From one of the route's own files to the reference.
    chain: Vec<&'g str>,
    /// Whether that chain crosses an `import()`.
    dynamic: bool,
    /// The build whose walk found it, which reaches it on its own.
    found_in: usize,
}

/// A breadth-first walk: how each module was reached, in the order it was.
struct Walk<'g> {
    steps: FxHashMap<&'g str, Step<'g>>,
    order: Vec<&'g str>,
}

impl<'g> Walk<'g> {
    /// Walk from `seeds`, entering a module only when `may_enter` allows it. A
    /// seed that is a bridged reference starts as dynamic as the chain that
    /// reached it.
    fn new(
        edges: &FxHashMap<&'g str, Vec<Edge<'g>>>,
        seeds: &[&'g str],
        bridged: &FxHashMap<&'g str, Bridge<'g>>,
        may_enter: impl Fn(&str) -> bool,
    ) -> Self {
        let mut steps: FxHashMap<&'g str, Step<'g>> = FxHashMap::default();
        let mut order = Vec::new();
        let mut queue = VecDeque::new();
        for &seed in seeds {
            if !steps.contains_key(seed) && may_enter(seed) {
                steps.insert(
                    seed,
                    Step {
                        from: None,
                        dynamic: bridged.get(seed).is_some_and(|bridge| bridge.dynamic),
                    },
                );
                order.push(seed);
                queue.push_back(seed);
            }
        }
        while let Some(current) = queue.pop_front() {
            let dynamic = steps.get(current).is_some_and(|step| step.dynamic);
            for edge in edges.get(current).map(Vec::as_slice).unwrap_or_default() {
                if steps.contains_key(edge.to) || !may_enter(edge.to) {
                    continue;
                }
                steps.insert(
                    edge.to,
                    Step {
                        from: Some(current),
                        dynamic: dynamic || edge.dynamic,
                    },
                );
                order.push(edge.to);
                queue.push_back(edge.to);
            }
        }
        Self { steps, order }
    }

    fn contains(&self, id: &str) -> bool {
        self.steps.contains_key(id)
    }

    fn dynamic(&self, id: &str) -> bool {
        self.steps.get(id).is_some_and(|step| step.dynamic)
    }

    /// The ids from where the walk started to `id`, behind the chain another
    /// build's walk reached that start by, when it was a bridged reference.
    fn chain(&self, id: &'g str, bridged: &FxHashMap<&'g str, Bridge<'g>>) -> Vec<&'g str> {
        let mut reversed = vec![id];
        let mut current = id;
        while let Some(from) = self.steps.get(current).and_then(|step| step.from) {
            reversed.push(from);
            current = from;
        }
        let mut chain = bridged
            .get(current)
            .map(|bridge| bridge.chain[..bridge.chain.len().saturating_sub(1)].to_vec())
            .unwrap_or_default();
        chain.extend(reversed.into_iter().rev());
        chain
    }
}

/// What a walk for `route` may enter: its own router files and no other
/// route's, and nothing that is shared.
fn enters<'a>(route: &'a RouteFiles, shared: &'a Walk<'_>) -> impl Fn(&str) -> bool + 'a {
    move |id| (!is_router_file(id) || is_own_file(route, id)) && !shared.contains(id)
}

/// Add to `seeds` every bridged reference `build` has that another build's
/// walk found, and return how each was reached.
fn bridge_into<'g>(
    index: usize,
    build: &Indexed<'g>,
    bridged: &FxHashMap<&'g str, Bridge<'g>>,
    seeds: &mut Vec<&'g str>,
) -> FxHashMap<&'g str, Bridge<'g>> {
    let mut found: Vec<(&'g str, &Bridge<'g>)> = bridged
        .iter()
        .filter(|(id, bridge)| {
            bridge.found_in != index && build.ids.binary_search(id).is_ok() && !seeds.contains(id)
        })
        .map(|(id, bridge)| (*id, bridge))
        .collect();
    found.sort_unstable_by_key(|(id, _)| *id);
    let mut prefixes = FxHashMap::default();
    for (id, bridge) in found {
        seeds.push(id);
        prefixes.insert(id, bridge.clone());
    }
    prefixes
}

/// Every module `walk` reached that `build` has code for.
fn list<'g>(
    build: &Indexed<'g>,
    walk: &Walk<'g>,
    bridged: &FxHashMap<&'g str, Bridge<'g>>,
) -> Vec<ModuleEntry> {
    walk.order
        .iter()
        .filter_map(|&id| {
            let (chunk, size) = build.emitted.get(id)?;
            Some(ModuleEntry {
                id: id.to_owned(),
                build: build.name.clone(),
                chunk: (*chunk).clone(),
                size: *size,
                chain: walk
                    .chain(id, bridged)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
                dynamic: walk.dynamic(id),
                routes: 0,
            })
        })
        .collect()
}

/// Largest gzip first, then by id, and the sizes added together.
fn finish(mut modules: Vec<ModuleEntry>) -> ModuleList {
    modules.sort_by(|a, b| {
        b.size
            .gzip
            .cmp(&a.size.gzip)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.build.cmp(&b.build))
    });
    let size = modules.iter().fold(AssetSize::zero(), |total, entry| {
        total.saturating_add(entry.size)
    });
    ModuleList { size, modules }
}

/// `route`'s own files among the ids `build` mentions, page first.
fn own_files<'g>(build: &Indexed<'g>, route: &RouteFiles) -> Vec<&'g str> {
    let mut own: Vec<&'g str> = build
        .ids
        .iter()
        .copied()
        .filter(|id| is_own_file(route, id))
        .collect();
    own.sort_unstable_by_key(|id| (*id != route.page, *id));
    own
}

/// The router name — `$page`, `$layout` — of a module that is a router file.
fn router_name(id: &str) -> Option<&str> {
    let file = id.split('?').next().unwrap_or(id);
    if file.starts_with("node_modules/") || file.contains("/node_modules/") {
        return None;
    }
    let name = file.rsplit('/').next().unwrap_or(file);
    name.starts_with('$')
        .then(|| name.split('.').next().unwrap_or(name))
}

fn is_router_file(id: &str) -> bool {
    router_name(id).is_some()
}

/// Whether `id` is the route's page, or a router file of its directory or one
/// above it that is not some other route's page.
fn is_own_file(route: &RouteFiles, id: &str) -> bool {
    if id == route.page {
        return true;
    }
    let Some(name) = router_name(id) else {
        return false;
    };
    if ONE_ROUTE.contains(&name) {
        return false;
    }
    let file = id.split('?').next().unwrap_or(id);
    let directory = file.rsplit_once('/').map_or("", |(directory, _)| directory);
    directory.is_empty()
        || route.directory == directory
        || route
            .directory
            .strip_prefix(directory)
            .is_some_and(|rest| rest.starts_with('/'))
}
