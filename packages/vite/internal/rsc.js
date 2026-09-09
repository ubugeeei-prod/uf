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
// describing the tree the server rendered. uf's payload
// (`packages/router/internal/payload.js`) carries the route's *data* and not
// its tree — that half needs a second React module graph, see
// ubugeeei-prod/uf#519 — so `packages/router/client.js` still hydrates by
// re-rendering the matched tree from the same modules the server rendered it
// from, and a module missing from the client bundle is a module React cannot
// hydrate. What *can* be dropped is a
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

import { readFileSync, statSync } from "node:fs";
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
    if ((route.templates ?? []).some((entry) => needed(entry.module))) return true;
    // A boundary with no module of its own is the record the scan synthesises
    // at the router root, and what renders there is the framework's own page —
    // already in `@uniflowed/router`, reaching nothing this project wrote. It
    // is skipped rather than left to `needed`, whose answer for a value that is
    // not a file is "assume it is needed": that answer is right for a path the
    // manifest has never heard of and wrong for the absence of a path, and
    // taking it here would have kept every page of every project in the client
    // bundle. See ubugeeei-prod/uf#351.
    for (const boundary of notFound) {
      if (boundary.page == null) continue;
      if (covers(boundary.path, route.path) && needed(boundary.page)) return true;
    }
    for (const boundary of errors) {
      if (boundary.module == null) continue;
      if (covers(boundary.path, route.path) && needed(boundary.module)) return true;
    }
    return false;
  };
}

// ---------------------------------------------------------------------------
// Server actions
//
// The other half of the split, and the one the manifest was already carrying
// an answer for. `serverActions` in the manifest is every action `uf_rsc`
// decided is a *callable endpoint* — an action some module that can hand it
// across a client boundary reaches — with the keyed id
// `crates/uf_rsc/src/action.rs` derived for it. An action nothing exposes is
// tracked in the registry and never written here, so a table built out of this
// file cannot contain a row that was not meant to be dialable.
//
// Two tables come out of it, for the two graphs:
//
// * `serverActionModules` is the browser's. It says, for each `"use server"`
//   file, which exports become `createServerReference` calls — and the plugin
//   answers that source *instead of the file*, so the module's body never
//   enters the client graph and neither does anything only it imported.
// * `serverActionTable` is the server's. It is what `virtual:uf/actions`
//   emits and what `createActionDispatcher` dials into.
//
// Both are keyed on the id and nothing else. No request-derived value ever
// becomes a path, a specifier or an export name here or downstream; see the
// header of `packages/router/internal/action-endpoint.js`.
// ---------------------------------------------------------------------------

/**
 * The request header carrying an action id, lowercased as Node delivers it.
 *
 * A second spelling of `ACTION_HEADER` in
 * `packages/router/internal/action-wire.js`, and it has to be one: this module
 * is plain JavaScript the Vite host imports before any transform, and that one
 * is Flow, which Node cannot import at all. `RSC_MANIFEST_ENV` above is the
 * same situation with `crates/uf_rsc/src/manifest.rs`. What keeps a second
 * spelling from becoming a second answer is
 * `tests/library/server-actions.test.js`, which reads both and compares them.
 */
export const ACTION_HEADER = "uf-action";

/** The id an action row must carry: 64 lowercase hexadecimal characters. */
function isActionId(value) {
  if (typeof value !== "string" || value.length !== 64) return false;
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    const digit = code >= 0x30 && code <= 0x39;
    const lower = code >= 0x61 && code <= 0x66;
    if (!digit && !lower) return false;
  }
  return true;
}

/**
 * Whether a name can be written as `export const <name>`.
 *
 * The scanner only ever produces identifiers, so this refuses nothing a real
 * project has. It is here because the alternative to refusing is emitting a
 * module that does not parse, and a generated file that does not parse fails a
 * build somewhere far from the module that caused it. `default` is handled by
 * the caller, which writes `export default`.
 */
function isExportableName(name) {
  if (typeof name !== "string" || name.length === 0) return false;
  const first = name.charCodeAt(0);
  const startish = (code) =>
    (code >= 0x41 && code <= 0x5a) ||
    (code >= 0x61 && code <= 0x7a) ||
    code === 0x24 ||
    code === 0x5f;
  if (!startish(first)) return false;
  for (let index = 1; index < name.length; index += 1) {
    const code = name.charCodeAt(index);
    if (!startish(code) && !(code >= 0x30 && code <= 0x39)) return false;
  }
  return true;
}

/** Every callable action of the manifest, in the manifest's own order. */
function callableActions(manifest) {
  if (manifest == null || !Array.isArray(manifest.serverActions)) return [];
  return manifest.serverActions.filter(
    (action) =>
      action != null &&
      isActionId(action.id) &&
      typeof action.module === "string" &&
      action.module !== "" &&
      // An inline `"use server"` closure has no export name to import, so it
      // has no reference in the client bundle and no row in the server's
      // table. It is in the manifest, and reaching it needs the payload
      // ubugeeei-prod/uf#252 is about.
      action.kind === "module-export" &&
      isExportableName(action.export === "default" ? "default_" : action.export),
  );
}

/**
 * The absolute path of a module the manifest names, or `null`.
 *
 * The manifest's paths are project-relative with forward slashes and were
 * written by a walk that already refused anything outside the root; joined
 * here and checked again, because a path that escapes the project is a path
 * this plugin would otherwise hand to Rollup as a module to emit.
 */
function moduleFile(root, relative) {
  const joined = path.resolve(root, relative);
  const inside = path.relative(root, joined);
  if (inside === "" || inside.startsWith("..") || path.isAbsolute(inside)) return null;
  return joined;
}

/**
 * Which exports of each `"use server"` file become references in the browser.
 *
 * Keyed by absolute path, because that is what Vite's `load` hook is given.
 * A file with no callable action is absent rather than present-and-empty: the
 * plugin substitutes a module only for a key it finds, and substituting an
 * empty module for a file something imports would be a build error in place of
 * a working import.
 *
 * @param {object | null} manifest from {@link readRscManifest}
 * @param {string} root absolute project root
 * @returns {Map<string, Array<{id: string, module: string, export: string}>>}
 */
export function serverActionModules(manifest, root) {
  const modules = new Map();
  for (const action of callableActions(manifest)) {
    const file = moduleFile(root, action.module);
    if (file == null) continue;
    const rows = modules.get(file);
    const row = { id: action.id, module: action.module, export: action.export };
    if (rows === undefined) modules.set(file, [row]);
    else rows.push(row);
  }
  return modules;
}

/**
 * Every callable action, as the server's dispatcher table.
 *
 * @param {object | null} manifest from {@link readRscManifest}
 * @param {string} root absolute project root
 * @returns {Array<{id: string, module: string, export: string, file: string}>}
 */
export function serverActionTable(manifest, root) {
  const rows = [];
  for (const action of callableActions(manifest)) {
    const file = moduleFile(root, action.module);
    if (file == null) continue;
    rows.push({ id: action.id, module: action.module, export: action.export, file });
  }
  return rows;
}

/**
 * The client bundle's stand-in for one `"use server"` module.
 *
 * What the browser gets in place of the file: one `createServerReference` per
 * callable export, an id each, and nothing the module itself imported. This is
 * the whole of how a database handle reached only through an action stays on
 * the server — `crates/uf_rsc/src/graph/build.rs` colours the module server for
 * the same reason, so that the analysis and the bundle agree about it.
 *
 * @param {Array<{id: string, module: string, export: string}>} actions
 */
export function actionReferenceSource(actions) {
  const lines = ['import { createServerReference } from "@uniflowed/router/action";', ""];
  for (const action of actions) {
    const reference = `createServerReference(${JSON.stringify(action.id)}, ${JSON.stringify(
      `${action.module}#${action.export}`,
    )})`;
    lines.push(
      action.export === "default"
        ? `export default ${reference};`
        : `export const ${action.export} = ${reference};`,
    );
  }
  return `${lines.join("\n")}\n`;
}

/**
 * The source of `virtual:uf/actions`: the table the endpoint dials into.
 *
 * One `import()` thunk per file rather than one per action, so a module with
 * four actions is one chunk of the server bundle and not four. Lazy for the
 * reason the handler table is: an action module is loaded when an action in it
 * is called, and a project's actions are not something every request should
 * pay to import.
 *
 * With no manifest the table is empty and every action call is a `404` — the
 * same answer a project driving Vite itself gets for the route split, and for
 * the same reason: uf will not guess at an analysis it was not given.
 *
 * @param {Array<{id: string, module: string, export: string, file: string}>} actions
 */
export function actionsModuleSource(actions) {
  const loaders = new Map();
  const declarations = [];
  const loaderId = (file) => {
    let id = loaders.get(file);
    if (id === undefined) {
      id = `load${loaders.size}`;
      loaders.set(file, id);
      declarations.push(`const ${id} = () => import(${JSON.stringify(file)});`);
    }
    return id;
  };

  const entries = actions.map(
    (action) => `  {
    id: ${JSON.stringify(action.id)},
    module: ${JSON.stringify(action.module)},
    export: ${JSON.stringify(action.export)},
    load: ${loaderId(action.file)},
  }`,
  );

  return `${declarations.join("\n")}
export const actions = [
${entries.join(",\n")}
];
export default actions;
`;
}

/**
 * A cheap identity for the manifest file, so a reader can tell it has changed.
 *
 * The plugin's `load` hook runs for every module in the graph and cannot parse
 * the manifest each time. Size and modification time together are what
 * `.uf/cache/transform` already keys on for the binary that wrote it, and the
 * same reasoning applies: a file that differs in neither is the file that was
 * read. `uf dev` also clears the cache outright when its watcher sees the
 * manifest change, so this is the build's answer rather than the only one.
 *
 * @param {string | undefined} file
 */
export function rscManifestKey(file) {
  if (file == null || file === "") return "";
  try {
    const stats = statSync(file);
    return `${String(stats.size)}:${String(stats.mtimeMs)}`;
  } catch {
    return "";
  }
}
