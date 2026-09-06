// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// The server/client split, as the bundler applies it.
//
// `crates/uf_rsc` decides which modules a `"use client"` boundary is reachable
// from. That answer used to reach nothing: `virtual:uf/routes` emitted
// `page: () => import(<file>)` for every route, `virtual:uf/client` imported
// that table, and so every page in the application was a chunk of the *client*
// bundle whether or not a browser had anything to do with it.
//
// This module is the first thing that reads the answer. What is done with it
// is in `routesModuleSource`, including the one thing a dropped route keeps.
//
// # Why the unit is a route and not a module
//
// Dropping a single Server Component from the client bundle is what Next.js
// does, and it works there because the browser is handed a Flight payload
// describing the tree the server rendered. uf has no such payload yet:
// `packages/router/client.js` hydrates by re-rendering the matched tree from
// the same modules the server rendered it from, so a module missing from the
// client bundle is a module React cannot hydrate. What *can* be dropped is a
// route the browser never renders at all — one where no client boundary is
// reachable from the page, its layouts, its loading fallbacks or the
// boundaries that cover it. Nothing under it is ever re-rendered in the
// browser, so nothing under it has to be shipped. See ubugeeei-prod/uf#350.
//
// # Why "unknown" means "ship it"
//
// The analysis scans `.js`. A page written as `.mdx`, a `.jsx` module, a file
// past the scanner's size limit: none of them is in the manifest, and the
// honest reading of a module the analysis never saw is that it might reach a
// boundary. Every unknown answers `true`, so the split can only ever remove a
// route uf has positively decided needs no browser — and a manifest that is
// missing, unreadable, or written by an older uf removes nothing at all.

import { readFileSync } from "node:fs";
import path from "node:path";

/** Environment variable naming the manifest, set by `uf build` and `uf dev`. */
export const RSC_MANIFEST_ENV = "UF_RSC_MANIFEST";

/**
 * The manifest schema this understands.
 *
 * Version 1 published the client boundaries and nothing that said which
 * modules sat *above* one, so it cannot answer the question this module asks.
 * An older manifest is therefore refused rather than read optimistically: a
 * missing `proximity` would read as `undefined`, compare unequal to
 * `"reaches-boundary"`, and quietly drop every route from the client bundle.
 */
const SUPPORTED_VERSION = 2;

/**
 * Read the RSC manifest, or `null` when there is nothing usable to read.
 *
 * Never throws. The split is an optimisation over a build that is already
 * correct without it, so no failure here may be a failure of the build.
 *
 * @param {string | undefined} file absolute path, from the environment
 */
export function readRscManifest(file) {
  if (file == null || file === "") return null;
  let parsed;
  try {
    parsed = JSON.parse(readFileSync(file, "utf8"));
  } catch {
    return null;
  }
  if (parsed == null || typeof parsed !== "object") return null;
  if (parsed.version !== SUPPORTED_VERSION || !Array.isArray(parsed.modules)) return null;
  return parsed;
}

/**
 * Which modules the browser has to be able to evaluate, keyed by project path.
 *
 * A `"use client"` module is a client bundle root by definition, and
 * `proximity` never says so about it — it is the far side of the boundary
 * rather than a module above one — so both halves are asked. This mirrors
 * `RscModule::requires_client_bundle` in `crates/uf_rsc/src/graph.rs`.
 */
function clientModules(manifest) {
  const modules = new Map();
  for (const module of manifest.modules) {
    if (module == null || typeof module.path !== "string") continue;
    modules.set(
      module.path,
      module.environment === "client" || module.proximity === "reaches-boundary",
    );
  }
  return modules;
}

/**
 * Whether `boundary`'s route path covers `route`'s.
 *
 * Deliberately "covers" and not "is nearest to". `packages/router` picks the
 * nearest not-found and error boundary above a path at render time; asking the
 * same question here would be a second implementation of that rule, and the
 * two would disagree the first time either moved. Every boundary that could
 * apply is counted instead, which can only decide that more routes need the
 * browser than strictly do.
 */
function covers(boundaryPath, routePath) {
  return (
    boundaryPath === "/" || routePath === boundaryPath || routePath.startsWith(`${boundaryPath}/`)
  );
}

/**
 * Build the predicate `routesModuleSource` asks about each route.
 *
 * Returns `(route) => boolean`: true when the route's page module belongs in
 * the client bundle. With no manifest every route answers true, which is the
 * whole table and exactly what the build emitted before this existed.
 *
 * @param {object | null} manifest from {@link readRscManifest}
 * @param {string} root absolute project root
 * @param {{notFound?: Array<object>, errors?: Array<object>}} [boundaries]
 *   the scanned table, so a boundary that needs the browser keeps the routes
 *   it covers in the client bundle
 */
export function clientRouteFilter(manifest, root, boundaries = {}) {
  if (manifest == null) return () => true;
  const modules = clientModules(manifest);

  const needed = (file) => {
    if (typeof file !== "string") return true;
    const relative = path.relative(root, file).split(path.sep).join("/");
    const answer = modules.get(relative);
    return answer === undefined ? true : answer;
  };

  const notFound = boundaries.notFound ?? [];
  const errors = boundaries.errors ?? [];

  return (route) => {
    if (needed(route.page)) return true;
    if (route.layouts.some(needed)) return true;
    if ((route.loading ?? []).some((entry) => needed(entry.module))) return true;
    for (const boundary of notFound) {
      if (covers(boundary.path, route.path) && needed(boundary.page)) return true;
    }
    for (const boundary of errors) {
      if (covers(boundary.path, route.path) && needed(boundary.module)) return true;
    }
    return false;
  };
}
