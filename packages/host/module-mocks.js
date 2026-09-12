// @noflow
//
// Plain JavaScript: this is part of the loader, and `bun-preload.js` imports
// it before any transform exists.
//
// Standing a module in for another one, at the only place that can do it: the
// loader. `@uniflowed/test`'s `uft.mock` is the API; this is the mechanism, and
// it lives here because by the time the runner sees an `import` the module has
// already been fetched, linked and evaluated.
//
// # Why these are *synchronous* hooks
//
// `./register.js` installs Node's asynchronous customization hooks, and those
// run on a loader thread of their own. That is right for the transform, which
// only needs the file's bytes — and useless for a mock, which is a value the
// test built: a spy the test holds a reference to cannot be sent to another
// thread, and a registry written on the main thread is not a registry the
// loader thread can read.
//
// So interception uses `node:module`'s `registerHooks`, which run in the thread
// that is doing the importing. They see this module's `mocks` map directly, and
// they chain into the asynchronous hooks for everything they do not claim — so
// a module that is not mocked is still transformed, still cached on disk, and
// still costs exactly what it cost before.
//
// # Why a mocked module gets a URL of its own
//
// A worker runs one file at a time and reuses the process, and Node's module
// registry is keyed by URL and never forgets. If a mocked module were served
// under its own URL, the *next* file in that worker would import the plain
// specifier and be handed the previous file's stand-in — a mock leaking across
// files, which is the one failure `docs/architecture.md` says the runner exists
// to prevent.
//
// So a mock does not replace a module; it redirects to a new one. Registering a
// mock takes the next revision number, the resolve hook appends it to the URL,
// and the mocked module is a different module from the real one rather than the
// same module with different contents. `unmock` stops appending, so the real
// URL — which may still hold the real module — is what the next import gets.
// Revisions are handed out for the life of the worker and never reused, which
// is what makes a cleared registry actually clear.
//
// The same parameter carries the module *epoch*. A module that imports a mocked
// one is standing in for it too — `consumer.js` computed its exports from
// whatever `client.js` gave it — so registering a mock, removing one, or
// calling `uft.resetModules` all start a new epoch, and every module reached by
// a path is evaluated again on the next import of it. Epochs are handed out for
// the life of the worker and never reused, for the reason revisions are not:
// the file after this one must not be able to land on a URL this one filled.
//
// The epoch is appended only to modules reached by a path specifier (`./x.js`,
// `/srv/x.js`): a bare specifier is a package, and handing a second copy of
// `@uniflowed/test` to a test file would give it a second registry, a second
// set of spies, and two of everything this package assumes there is one of. A
// *mocked* package still gets a URL of its own, from its revision — being
// stood in for is exactly the case where identity has to be given up.
//
// # Bun, and the doors that were shut
//
// Bun is a declared host, and a suite that can replace a module on one host and
// not the other is a suite a person cannot move between them
// (ubugeeei-prod/uf#419). Bun has no `registerHooks`, and the mechanism it does
// have is the plugin API `bun-preload.js` already uses for the transform. This
// section is what a run of Bun 1.1.27 said when each way of using it was tried,
// written down because "it did not work" is the kind of thing that gets tried
// twice — and because each of the three looks obviously right until it is run.
//
// **Two of the three have since opened.** Re-run on **Bun 1.3.13**, the third
// door below — a stand-in written after the process started — loads, and an
// `onResolve` answering with its path redirects an `import` *declaration* and
// not merely a dynamic `import()`. That is a whole mechanism: with the hook
// installed once and a mutable registry consulted per resolution, a mock
// registers and clears the way it does on Node. So the reason this file still
// raises `UnsupportedError` on Bun is that nobody has written it, not that
// Bun cannot — which is the opposite of what the three paragraphs below said
// when they were written, and the reason they are kept with a date on them
// rather than deleted.
//
// The first door is half open and it is not the useful half: the `ENOENT` is
// gone, so a query-carrying path resolves and loads, but `onLoad` still never
// sees the query and so cannot serve the stand-in's contents. The second is
// still shut. The re-entrancy warning at the end of this section still holds.
//
// The whole design turns on giving the stand-in an identity of its own, so all
// three questions are "where does *somewhere else* go":
//
//   * **In the query string**, the way Node does it. An `onResolve` that
//     answers with a path carrying `?uf-modules=1.0` is met with
//     `ENOENT reading "file:/…"`, and a plain `import("./x.js?n=1")` written in
//     a source file never returns at all. #419 supposes this one works; it does
//     not, on this version.
//   * **In a plugin namespace**, the way esbuild-shaped plugins serve a virtual
//     module. `{ path, namespace }` is honoured for a dynamic `import()` and
//     refused for an `import` declaration — `InvalidURL while resolving
//     package`, from the entry point and from a module three deep alike, with
//     the path plain, suffixed, prefixed, and with the importer served through
//     the transform's own `onLoad`. A mock only a dynamic import can see would
//     miss the case the feature exists for: a module that imports its client at
//     the top and calls it while it is being evaluated.
//   * **In another file.** A stand-in written to disk and resolved to by path
//     is an ordinary module, and every kind of import reaches it — as long as
//     it existed when the process started. A file created *after* that is
//     `ENOENT reading "file:/…"` even though `Bun.resolveSync` finds it and
//     `import()` of the same absolute path loads it, so a mock registered
//     while the suite runs cannot be written down anywhere Bun will read.
//     **This is the one that opened**: on Bun 1.3.13 the file created after
//     start-up loads, and this is the door to build on.
//
// Bun's own `mock.module` does all of this and is available outside `bun test`.
// It is not the answer here, and the reason is above: it maintains live
// bindings, so it reaches back into modules that have already imported the real
// one. That is the wider meaning this package deliberately does not have — see
// "What a mock does not do" in `@uniflowed/test`'s `internal/modules.js` — and
// adopting it on one host is exactly the "one API, two meanings" that rule
// exists to prevent.
//
// One more thing for whoever opens one of the doors: `Bun.resolveSync`
// re-enters the plugin's own `onResolve`, so a hook that calls it needs a
// re-entrancy guard. Without one the first mocked import recurses up the
// directory tree until the resolver runs out of parents, and the symptom is a
// resolution that never returns rather than an error that says why.
//
// So `uft.mock` raises `UnsupportedError` on Bun, naming the host and what it
// would take, and `crates/uf_cli/tests/bun_host.rs` starts a real Bun and holds
// it to that.
//
// That message is still the right one to give today, and it is worth being
// exact about why now that a door has opened: it says this *implementation*
// needs `registerHooks` and Bun has none, which stays true. What is no longer
// true is the conclusion a reader would draw from it. The work is writing a
// second implementation against the plugin API, not waiting for Bun.

import fs from "node:fs";
import * as nodeModule from "node:module";
import os from "node:os";
import path from "node:path";

/** The URL parameter carrying `<module epoch>.<mock revision>`. */
export const REVISION_PARAM = "uf-modules";

/** The URL parameter marking an import that must reach the real module. */
export const ACTUAL_PARAM = "uf-actual";

/** This module's own URL, which a generated stand-in imports its values from. */
const SELF = import.meta.url;

/** Registered mocks, by module key. */
const mocks = new Map();

/** Namespaces handed to a generated module, by the exact URL it was loaded as. */
const served = new Map();

/** Bun stand-in files, keyed by the mocked module identity they serve. */
const bunStandins = new Map();

/** Where Bun stand-in modules are written, lazily. */
let bunStandinRoot = null;

/** How many Bun stand-in files have ever been written in this process. */
let bunStandinFiles = 0;

/** How many mocks have ever been registered in this process. */
let revisions = 0;

/** How many epochs have ever been started in this process. */
let epochs = 0;

/** The epoch this file is in; `0` until it does something that starts one. */
let epoch = 0;

/** The installed hooks, or `null` when interception is not installed. */
let handle = null;

/**
 * A module's identity for the purpose of mocking: its URL with no query.
 *
 * The query is where every mechanism in this file writes — the worker's
 * cache-busting `uf-run`, this file's revision, `importActual`'s marker — so
 * two URLs that differ only there are the same module as far as a mock is
 * concerned.
 */
export function moduleKey(url) {
  const parsed = new URL(url);
  parsed.search = "";
  parsed.hash = "";
  return parsed.href;
}

/**
 * Whether this host can intercept a module before it is imported.
 *
 * The one requirement is synchronous, in-thread module hooks. Node has them;
 * Bun's `node:module` has neither `register` nor `registerHooks`, and Deno has
 * no loader in `@uniflowed/host` at all. `@uniflowed/test` turns a `false` here
 * into an error that names the host rather than a mock that quietly does
 * nothing.
 *
 * Bun's plugin API is not a second answer, and "Bun, and the three doors that
 * are shut" at the top of this file is why — it is a `false` on purpose rather
 * than a `true` nobody wired up.
 */
export function interceptionSupported() {
  return typeof nodeModule.registerHooks === "function";
}

/**
 * Install the interception hooks, once.
 *
 * Called on the first `uft.mock` or `uft.resetModules` rather than at import,
 * because a resolve hook that runs for every specifier in the process is not
 * something a suite that never mocks anything should pay for.
 */
export function installInterception() {
  if (handle != null) {
    return true;
  }
  if (!interceptionSupported()) {
    return false;
  }
  handle = nodeModule.registerHooks({ load: loadHook, resolve: resolveHook });
  return true;
}

/**
 * Register `namespace` as the stand-in for the module at `url`.
 *
 * Returns nothing: what the caller needs is that the *next* resolution of that
 * module lands somewhere else, which the resolve hook arranges from the
 * revision recorded here.
 */
export function defineModuleMock(url, namespace) {
  revisions += 1;
  mocks.set(moduleKey(url), { namespace, revision: revisions });
  startModuleEpoch();
}

/** Stop standing in for the module at `url`. */
export function removeModuleMock(url) {
  if (!mocks.delete(moduleKey(url))) {
    return false;
  }
  startModuleEpoch();
  return true;
}

/** Whether the module at `url` is currently stood in for. */
export function isModuleMocked(url) {
  return mocks.has(moduleKey(url));
}

/** The epoch path imports are currently loading into. */
export function moduleEpoch() {
  return epoch;
}

/**
 * Begin a new epoch, so a path import evaluates its module again.
 *
 * Counted process-wide rather than per file. A file that reused the number a
 * previous file's epoch had would be handed that file's modules, mocks and all,
 * which is the leak the whole scheme exists to prevent.
 */
export function startModuleEpoch() {
  epochs += 1;
  epoch = epochs;
  return epoch;
}

/**
 * Forget every mock and leave the epoch.
 *
 * Called by the worker between files. Epochs and revisions deliberately keep
 * counting: the next file's mock of the same module must not be handed the URL
 * this file's mock is cached under.
 */
export function resetModuleMocks() {
  mocks.clear();
  served.clear();
  bunStandins.clear();
  epoch = 0;
}

/**
 * The URL an import that must reach the real module should use.
 *
 * A mocked module is standing at the URL an ordinary import resolves to, so
 * reaching past it takes a URL of its own — one per epoch, so that two calls in
 * a row hand back the same module rather than compiling it twice.
 *
 * A module nobody is standing in for needs no marker: the URL an ordinary
 * import would use already holds the real thing, and taking the marked path
 * anyway would hand back a second copy of a module the test is already holding.
 *
 * `pathLike` is whether the caller wrote a path rather than a package name, and
 * it is not a detail: this is the resolve hook's rule applied by hand, and the
 * rule is that a package keeps its identity across an epoch. Getting it wrong
 * here would hand a test a second copy of `@uniflowed/test` — a second registry
 * and a second set of spies — from the one call that is supposed to reach the
 * real thing.
 */
export function actualUrl(url, pathLike) {
  const parsed = new URL(url);
  if (!mocks.has(moduleKey(url))) {
    if (epoch !== 0 && pathLike) {
      parsed.searchParams.set(REVISION_PARAM, `${epoch}.0`);
    }
    return parsed.href;
  }
  parsed.searchParams.set(ACTUAL_PARAM, String(epoch));
  return parsed.href;
}

/**
 * The values a generated stand-in module exports.
 *
 * Keyed by the exact URL the stand-in was loaded as rather than by the module
 * key, so that a stand-in evaluated after its mock was replaced still reads the
 * namespace its export list was written from.
 */
export function namespaceFor(url) {
  const namespace = served.get(url);
  if (namespace == null) {
    throw new Error(`@uniflowed/host: no mocked namespace was recorded for ${url}`);
  }
  return namespace;
}

/**
 * The source of the stand-in module for `url`, or `null` when there is none.
 *
 * An ES module's export names are fixed when it is compiled, so they are
 * written out here from the keys the mock actually has. That is the whole
 * reason this returns source rather than an object: a namespace object cannot
 * be handed to `import`, and a module with the wrong names would fail to link
 * with an error about the importer rather than about the mock.
 */
export function mockedSource(url) {
  const parsed = new URL(url);
  if (parsed.searchParams.has(ACTUAL_PARAM)) {
    return null;
  }
  const record = mocks.get(moduleKey(url));
  if (record == null) {
    return null;
  }

  served.set(url, record.namespace);
  const lines = [
    `import { namespaceFor } from ${JSON.stringify(SELF)};`,
    `const values = namespaceFor(${JSON.stringify(url)});`,
  ];
  const bindings = [];
  for (const [index, name] of Object.keys(record.namespace).entries()) {
    lines.push(`const binding${index} = values[${JSON.stringify(name)}];`);
    bindings.push(`binding${index} as ${exportName(name)}`);
  }
  // `export {}` rather than nothing, so a mock with no exports is still an ES
  // module: without an export or import declaration Node would have to guess
  // the format from the package, and the guess is not always "module".
  lines.push(bindings.length === 0 ? "export {};" : `export { ${bindings.join(", ")} };`);
  return `${lines.join("\n")}\n`;
}

/**
 * Write the current stand-in for `url` to a real file Bun can redirect to.
 *
 * Node serves generated modules from `loadHook`, but Bun's `onLoad` never sees
 * the query-carrying identity this file uses. The piece of Bun that does work
 * is an `onResolve` answer naming another file, so this materializes the same
 * generated module under a unique path. It is intentionally only a primitive:
 * Bun still cannot make a direct dynamic import take that redirect, so
 * `@uniflowed/test` keeps the public API disabled on Bun until that path is
 * solved too.
 */
export function bunMockedModulePath(url) {
  const record = mocks.get(moduleKey(url));
  if (record == null) {
    return null;
  }

  const identity = mockedModuleIdentity(url, record.revision);
  let file = bunStandins.get(identity);
  if (file != null) {
    return file;
  }

  const source = mockedSource(identity);
  if (source == null) {
    return null;
  }

  const root = bunStandinDirectory();
  bunStandinFiles += 1;
  file = path.join(root, `${bunStandinFiles}.mjs`);
  fs.writeFileSync(file, source);
  bunStandins.set(identity, file);
  return file;
}

/** The mocked module identity for a registered module. */
function mockedModuleIdentity(url, revision) {
  const parsed = new URL(url);
  parsed.searchParams.set(REVISION_PARAM, `${epoch}.${revision}`);
  return parsed.href;
}

/** Directory for generated Bun stand-in files. */
function bunStandinDirectory() {
  if (bunStandinRoot == null) {
    bunStandinRoot = fs.mkdtempSync(path.join(os.tmpdir(), "uf-bun-module-mocks-"));
  }
  return bunStandinRoot;
}

/** How an export is named in an `export {}` clause. */
function exportName(name) {
  // Reserved words are fine — `export { x as default }` is the whole point —
  // but anything that is not an identifier at all has to be a string, which
  // ES2022 allows and which is how a mock keeps a name like `foo-bar`.
  return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : JSON.stringify(name);
}

/**
 * The `load` hook: serve a stand-in, or defer to the transform.
 *
 * Deferring is the common case and has to stay cheap: with no mocks registered
 * this is one map lookup on an empty map.
 */
function loadHook(url, context, nextLoad) {
  if (mocks.size === 0 || !url.startsWith("file:")) {
    return nextLoad(url, context);
  }
  const source = mockedSource(url);
  if (source == null) {
    return nextLoad(url, context);
  }
  return { format: "module", shortCircuit: true, source };
}

/**
 * The `resolve` hook: send an import to the revision it belongs to.
 *
 * Runs after the rest of the chain, so what it rewrites is a fully resolved
 * URL rather than a specifier it would have to resolve itself.
 */
function resolveHook(specifier, context, nextResolve) {
  const resolved = nextResolve(specifier, context);
  if (mocks.size === 0 && epoch === 0) {
    return resolved;
  }
  const url = redirect(specifier, context?.parentURL, resolved?.url);
  return url === resolved.url ? resolved : { ...resolved, url };
}

/** Where an import of `url` should actually go. */
function redirect(specifier, parentURL, url) {
  if (typeof url !== "string" || !url.startsWith("file:")) {
    return url;
  }
  const parsed = new URL(url);
  // Already answered: `importActual` names the URL it wants, and a URL that
  // carries a revision was produced by this function on the way in.
  if (parsed.searchParams.has(ACTUAL_PARAM) || parsed.searchParams.has(REVISION_PARAM)) {
    return url;
  }

  const revision = mocks.get(moduleKey(url))?.revision ?? 0;
  // A package keeps its identity across an epoch; a module of this project's
  // own does not. See the note at the top of the file.
  const at = isPathSpecifier(specifier) ? epochOf(parentURL) : 0;
  if (revision === 0 && at === 0) {
    return url;
  }
  parsed.searchParams.set(REVISION_PARAM, `${at}.${revision}`);
  return parsed.href;
}

/** Whether `specifier` names a file rather than a package. */
function isPathSpecifier(specifier) {
  return specifier.startsWith("./") || specifier.startsWith("../") || specifier.startsWith("/");
}

/**
 * The epoch an import from `parentURL` belongs to.
 *
 * A module loaded in epoch two loads its own dependencies in epoch two, however
 * many mocks have been registered since: a graph half of which is one epoch and
 * half another is two copies of a module that expects to be one.
 */
function epochOf(parentURL) {
  if (parentURL == null || !parentURL.startsWith("file:")) {
    return epoch;
  }
  const carried = new URL(parentURL).searchParams.get(REVISION_PARAM);
  if (carried == null) {
    return epoch;
  }
  return Number.parseInt(carried.split(".")[0], 10);
}
