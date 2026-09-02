//! Deciding which module goes in which chunk.
//!
//! There are three kinds of chunk root, and they follow the shape of the app
//! rather than a heuristic:
//!
//! * every **route entry** is a root, so a route's own code is one file;
//! * every **client boundary** `uf_rsc` found is a root, because the set of
//!   `"use client"` modules the server graph reaches *is* the client bundle's
//!   set of entry points;
//! * everything reachable from more than one root goes in a **shared** chunk.
//!
//! # The invariant that matters
//!
//! A chunk carries an [`ChunkEnvironment`], and a chunk the browser loads must
//! never hold server-only code. That falls out of how roots are walked rather
//! than from a filter afterwards: a walk from a client root only reaches
//! modules the client half reaches, and `uf_rsc` has already marked anything
//! server-only as unreachable from there. [`Chunk::environment`] is `Client`
//! only when a client root reaches the chunk, so a server-only module can only
//! ever land in a server chunk.

use camino::Utf8Path;
use compact_str::CompactString;
use uf_infra::{FxHashSet, InlineVec};

use crate::graph::{Edge, ModuleGraph, ModuleIndex};
use crate::limits::{BundlerLimits, LimitError};
use crate::shake::Shaken;

/// Why a chunk exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    /// One route or build entry.
    Entry {
        /// The entry module.
        module: ModuleIndex,
    },
    /// One `"use client"` boundary.
    Client {
        /// The client-bundle root.
        module: ModuleIndex,
    },
    /// Modules more than one root reaches.
    Shared,
}

impl ChunkKind {
    /// Stable identifier used in file names and the manifest.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Entry { .. } => "entry",
            Self::Client { .. } => "client",
            Self::Shared => "shared",
        }
    }
}

/// Which half of the app loads a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChunkEnvironment {
    /// Evaluated while rendering on the server.
    Server,
    /// Downloaded and evaluated by the browser.
    Client,
}

impl ChunkEnvironment {
    /// Stable identifier used in file names and the manifest.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Client => "client",
        }
    }
}

/// One chunk, before it is rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Stable logical name, without the content hash.
    pub name: CompactString,
    /// Why the chunk exists.
    pub kind: ChunkKind,
    /// Which half of the app loads it.
    pub environment: ChunkEnvironment,
    /// Its modules, dependencies first.
    pub modules: Vec<ModuleIndex>,
}

/// Longest logical chunk name, before the hash is appended.
const MAX_CHUNK_NAME_BYTES: usize = 64;

/// Plan the chunks of a shaken graph.
pub fn plan_chunks(
    graph: &ModuleGraph,
    shaken: &Shaken,
    limits: &BundlerLimits,
) -> Result<Vec<Chunk>, LimitError> {
    let roots = collect_roots(graph, shaken);
    if roots.len() > limits.max_chunks {
        return Err(LimitError::TooManyChunks {
            count: roots.len(),
            limit: limits.max_chunks,
        });
    }

    let reach = reach_by_root(graph, shaken, &roots);
    let mut chunks = Vec::with_capacity(roots.len() + 2);
    for (position, root) in roots.iter().enumerate() {
        chunks.push(Chunk {
            name: chunk_name(root.kind.as_str(), &graph.module(root.module).path),
            kind: root.kind,
            environment: root.environment,
            modules: owned_by(&reach, position),
        });
    }

    for environment in [ChunkEnvironment::Server, ChunkEnvironment::Client] {
        let modules = shared_modules(&reach, &roots, environment);
        if modules.is_empty() {
            continue;
        }
        chunks.push(Chunk {
            name: CompactString::new(format!("shared-{}", environment.as_str())),
            kind: ChunkKind::Shared,
            environment,
            modules,
        });
    }

    chunks.retain(|chunk| !chunk.modules.is_empty());
    if chunks.len() > limits.max_chunks {
        return Err(LimitError::TooManyChunks {
            count: chunks.len(),
            limit: limits.max_chunks,
        });
    }

    for chunk in &mut chunks {
        chunk.modules = order_modules(graph, &chunk.modules);
    }
    chunks.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(chunks)
}

/// A chunk root: an entry, or a client boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Root {
    module: ModuleIndex,
    kind: ChunkKind,
    environment: ChunkEnvironment,
}

fn collect_roots(graph: &ModuleGraph, shaken: &Shaken) -> Vec<Root> {
    let mut roots: Vec<Root> = Vec::new();
    let mut seen: FxHashSet<ModuleIndex> = FxHashSet::default();

    for module in graph.entries() {
        if shaken.is_live(*module) && seen.insert(*module) {
            roots.push(Root {
                module: *module,
                kind: ChunkKind::Entry { module: *module },
                environment: ChunkEnvironment::Server,
            });
        }
    }
    for module in graph.client_roots() {
        if shaken.is_live(module) && seen.insert(module) {
            roots.push(Root {
                module,
                kind: ChunkKind::Client { module },
                environment: ChunkEnvironment::Client,
            });
        }
    }

    roots.sort_by(|left, right| {
        graph
            .module(left.module)
            .path
            .cmp(&graph.module(right.module).path)
    });
    roots
}

/// For each live module, which roots reach it.
///
/// Iterative, one queue per root, and a walk never descends into another root:
/// that is what makes a client boundary its own chunk instead of being inlined
/// into every route that renders it.
fn reach_by_root(graph: &ModuleGraph, shaken: &Shaken, roots: &[Root]) -> Vec<InlineVec<u32, 4>> {
    let mut reach: Vec<InlineVec<u32, 4>> = vec![InlineVec::new(); graph.modules().len()];
    let root_set: FxHashSet<ModuleIndex> = roots.iter().map(|root| root.module).collect();

    for (position, root) in roots.iter().enumerate() {
        let mut queue = vec![root.module];
        let mut seen: FxHashSet<ModuleIndex> = FxHashSet::default();
        seen.insert(root.module);
        reach[root.module.get()].push(position as u32);

        while let Some(index) = queue.pop() {
            let module = graph.module(index);
            for (import, edge) in module.edges.iter().enumerate() {
                let Edge::Module(target) = edge else {
                    continue;
                };
                if !module.record.imports[import].form.is_linked() || !shaken.is_live(*target) {
                    continue;
                }
                if root_set.contains(target) || !seen.insert(*target) {
                    continue;
                }
                reach[target.get()].push(position as u32);
                queue.push(*target);
            }
        }
    }

    reach
}

fn owned_by(reach: &[InlineVec<u32, 4>], root: usize) -> Vec<ModuleIndex> {
    reach
        .iter()
        .enumerate()
        .filter(|(_, roots)| roots.len() == 1 && roots[0] == root as u32)
        .map(|(position, _)| ModuleIndex::from_position(position))
        .collect()
}

fn shared_modules(
    reach: &[InlineVec<u32, 4>],
    roots: &[Root],
    environment: ChunkEnvironment,
) -> Vec<ModuleIndex> {
    reach
        .iter()
        .enumerate()
        .filter(|(_, owners)| owners.len() > 1)
        .filter(|(_, owners)| shared_environment(roots, owners) == environment)
        .map(|(position, _)| ModuleIndex::from_position(position))
        .collect()
}

/// A shared chunk is a client chunk as soon as one client root reaches it.
///
/// That is the safe direction: shipping a module the browser needs is correct,
/// and a module no client root reaches stays out of the browser entirely.
fn shared_environment(roots: &[Root], owners: &InlineVec<u32, 4>) -> ChunkEnvironment {
    if owners
        .iter()
        .any(|owner| roots[*owner as usize].environment == ChunkEnvironment::Client)
    {
        ChunkEnvironment::Client
    } else {
        ChunkEnvironment::Server
    }
}

/// Order a chunk's modules so every module follows the ones it imports.
///
/// Iterative post-order depth-first search. A cycle is broken at the back edge,
/// which puts the modules of the cycle in a deterministic order rather than
/// looping — the same choice every bundler makes, and the reason a chunk's
/// modules are documented as evaluation-ordered rather than fully linked.
fn order_modules(graph: &ModuleGraph, modules: &[ModuleIndex]) -> Vec<ModuleIndex> {
    let members: FxHashSet<ModuleIndex> = modules.iter().copied().collect();
    let mut ordered = Vec::with_capacity(modules.len());
    let mut visited: FxHashSet<ModuleIndex> = FxHashSet::default();

    let mut sorted = modules.to_vec();
    sorted.sort_by(|left, right| graph.module(*left).path.cmp(&graph.module(*right).path));

    for start in sorted {
        if visited.contains(&start) {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        visited.insert(start);

        while let Some((index, cursor)) = stack.pop() {
            let module = graph.module(index);
            let mut next = cursor;
            let mut descended = false;

            while next < module.edges.len() {
                let edge = &module.edges[next];
                next += 1;
                let Edge::Module(target) = edge else {
                    continue;
                };
                if !members.contains(target) || !visited.insert(*target) {
                    continue;
                }
                stack.push((index, next));
                stack.push((*target, 0));
                descended = true;
                break;
            }

            if !descended {
                ordered.push(index);
            }
        }
    }

    ordered
}

/// A file-system-safe logical name for a chunk.
fn chunk_name(prefix: &str, path: &Utf8Path) -> CompactString {
    let mut name = String::with_capacity(prefix.len() + path.as_str().len() + 1);
    name.push_str(prefix);
    name.push('-');

    for byte in path.as_str().bytes() {
        if name.len() >= MAX_CHUNK_NAME_BYTES {
            break;
        }
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' => name.push(byte as char),
            _ => {
                if !name.ends_with('-') {
                    name.push('-');
                }
            }
        }
    }
    while name.ends_with('-') {
        name.pop();
    }

    CompactString::new(name)
}
