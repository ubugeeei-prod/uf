// @noflow
//
// Plain JavaScript: the driver imports it, and the driver runs before any Flow
// loader exists.
//
// The module graph `uf build --analyze` reads: every module each bundle was
// built from, what it imports, and which chunk its rendered code went into.
//
// What the graph means — which route a module belongs to, the chain of imports
// behind it, what it weighs — is uf's, in `crates/uf_bundle/src/analysis.rs`.
// This file writes down only what the bundler knows and uf cannot, and that
// split is the reason the graph is a file at all: a builder other than this
// one that writes the same file gets the same analysis.
//
//   {
//     "version": 1,
//     "builds": [{
//       "environment": "client",
//       "entries": ["virtual:uf/client"],
//       "modules": [{ "id": "app/$page.js", "imports": ["app/Counter.js"], "dynamicImports": [] }],
//       "chunks": [{
//         "file": "assets/index-3f2a.js",
//         "facade": "virtual:uf/client",
//         "modules": [{ "id": "virtual:uf/client", "code": "…" }]
//       }]
//     }]
//   }

import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

/** Where the driver writes the graph, in the build's metadata directory. */
export const MODULE_GRAPH_FILE = "uf-module-graph.json";

const VERSION = 1;

/**
 * A plugin that records the graph of every bundle it takes part in, and the
 * means to write what it recorded.
 *
 * `isReference(file)` says whether an absolute file is a client reference: an
 * entry the browser loads because a server component named it, rather than on
 * every page. Its chunk still carries it as the facade, and it is left out of
 * `entries`, which is the list of what every page of a bundle loads.
 */
export function createModuleGraphCollector(root, { isReference = () => false } = {}) {
  const builds = [];
  const graph = () => ({ version: VERSION, builds });
  return {
    plugin: {
      name: "uf:module-graph",
      // Last, so the bundle it reads is one every other plugin has finished.
      enforce: "post",
      generateBundle(_options, bundle) {
        builds.push(describeBundle(this, root, bundle, isReference));
      },
    },
    graph,
    write(file) {
      mkdirSync(path.dirname(file), { recursive: true });
      writeFileSync(file, `${JSON.stringify(graph())}\n`);
    },
  };
}

/** One bundle, as `generateBundle` sees it. */
function describeBundle(context, root, bundle, isReference) {
  const ids = typeof context.getModuleIds === "function" ? [...context.getModuleIds()] : [];
  const modules = ids
    .map((id) => {
      const info = context.getModuleInfo(id);
      return {
        id: moduleId(root, id),
        imports: (info?.importedIds ?? []).map((imported) => moduleId(root, imported)),
        dynamicImports: (info?.dynamicallyImportedIds ?? []).map((imported) =>
          moduleId(root, imported),
        ),
      };
    })
    .sort(byKey("id"));
  const entries = new Set();
  const chunks = [];
  for (const output of Object.values(bundle)) {
    if (output.type !== "chunk") {
      continue;
    }
    const facade = output.isEntry ? (output.facadeModuleId ?? null) : null;
    if (facade != null && !isReference(facade.split("?")[0])) {
      entries.add(moduleId(root, facade));
    }
    chunks.push({
      file: output.fileName,
      facade: facade == null ? null : moduleId(root, facade),
      // A module the bundler left no code for was still walked through to
      // reach what it imports, so it stays in `modules` above; it has nothing
      // to weigh, so it is not in the chunk.
      modules: Object.entries(output.modules ?? {})
        .map(([id, rendered]) => ({ id: moduleId(root, id), code: rendered?.code ?? "" }))
        .filter((module) => module.code !== ""),
    });
  }
  return {
    environment: context.environment?.name ?? "client",
    entries: [...entries].sort(),
    modules,
    chunks: chunks.sort(byKey("file")),
  };
}

/**
 * A module id as the analysis spells it: relative to the project root with
 * `/`, without the `\0` a resolved virtual module carries, and with its query.
 *
 * Relative even when it climbs out of the root, as a workspace package does,
 * because an absolute path would make two machines' reports of the same build
 * disagree.
 */
export function moduleId(root, id) {
  const bare = id.startsWith("\0") ? id.slice(1) : id;
  const at = bare.indexOf("?");
  const file = at === -1 ? bare : bare.slice(0, at);
  if (!path.isAbsolute(file)) {
    return bare;
  }
  const relative = path.relative(root, file);
  if (path.isAbsolute(relative)) {
    return bare;
  }
  return `${relative.split(path.sep).join("/")}${at === -1 ? "" : bare.slice(at)}`;
}

function byKey(key) {
  return (a, b) => (a[key] < b[key] ? -1 : a[key] > b[key] ? 1 : 0);
}
