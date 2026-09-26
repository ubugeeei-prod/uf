// @noflow
//
// Plain JavaScript: this is part of the loader, so it cannot be Flow.
//
// A fresh copy of the project's modules for every test file a worker runs.
//
// `uf test` runs many files in one worker process, because starting Node and
// loading React and a DOM for every file is most of what a small file costs.
// A process keeps every module it evaluated, though, and module state is
// state: a React context created in `@uniflowed/hooks` whose current value one
// file's render left set was still set when the next file rendered, and the
// next file's `<Time>` printed the previous file's time zone
// (ubugeeei-prod/uf#1443). Putting back each piece of shared state by name
// (`@uniflowed/test`'s `internal/isolation.js`) cannot reach state like that,
// because nobody can list it.
//
// So the promise is made by construction instead: every module of the project
// a test file reaches is imported under a URL that names the file's run —
// `?uf-file=<n>` — and a different URL is a different module instance. The
// test file's own imports, their imports, and every dynamic `import()` they
// make later are all resolved from a parent that carries the number, so the
// whole graph the file reaches is its own, evaluated from scratch.
//
// # What is shared, and why that is still the promise
//
// Two kinds of module keep one instance per process:
//
// * **Installed packages** — anything under `node_modules` that is not a
//   workspace package: React, react-dom, happy-dom, axe-core. They are
//   evaluated once per worker. This is the line every runner that isolates by
//   module registry draws somewhere; loading React and a DOM afresh for every
//   file is the cost this design exists to avoid. What a file can change in
//   them through their public API — a rendered root, the document's contents,
//   the window's own properties — is what `@uniflowed/react-testing` and
//   `@uniflowed/test` put back between files, as they always have.
// * **The runner itself**: the directories the worker names as its own
//   (`@uniflowed/test` and `@uniflowed/host`). A test file registers its cases
//   in the runner's registry and asks the runner's module mocker for stand-ins,
//   so it must reach the one instance the worker is running, not a copy.
//
// A module either kind imports stays in that kind's world: the scope applies
// only to imports made from the test file or from a module that already
// carries the file's number. A package that imports a project module — rare,
// and a design smell — shares that one instance with every file.
//
// Nothing here runs outside `uf test`'s workers: the scope is only ever set by
// `@uniflowed/test/worker.js`, and with none set every URL is answered as it
// was.

import { fileURLToPath } from "node:url";

/** Where the worker says which run it is serving and what it keeps shared. */
export const FILE_SCOPE = Symbol.for("@uniflowed/host/file-scope");

/** The query parameter a file's run number is carried in. */
export const FILE_PARAM = "uf-file";

/** The worker's own cache-buster on the test file, which starts a file's graph. */
const RUN_PARAM = "uf-run";

/**
 * The URL `url` should be loaded as, imported from `parentURL`.
 *
 * `isImport` is false for a `require()`, which is never rewritten: the
 * CommonJS cache is keyed by path, so a new URL would not make a new instance,
 * only a lie about one.
 *
 * A URL that already carries a query of its own keeps it and gains the run
 * as well: `import("./temporal.js?a-native-host")` from a test file is a
 * second copy the test asked for on purpose, and the modules *that* copy
 * imports must still be this file's — its clock the one the test file set —
 * not whichever copy the first file to ask for them got.
 */
export function fileScoped(parentURL, url, isImport) {
  const scope = globalThis[FILE_SCOPE];
  if (scope == null || !isImport) return url;
  if (typeof url !== "string" || !url.startsWith("file:")) return url;
  const run = runOf(parentURL);
  if (run == null) return url;
  const query = url.indexOf("?");
  const path = fileURLToPath(query === -1 ? url : url.slice(0, query));
  if (isInstalled(path) || scope.shared.some((directory) => isInside(path, directory))) {
    return url;
  }
  if (query === -1) return `${url}?${FILE_PARAM}=${run}`;
  const params = new URLSearchParams(url.slice(query + 1));
  if (params.has(FILE_PARAM)) return url;
  return `${url}&${FILE_PARAM}=${run}`;
}

/**
 * The run a module imported from `parentURL` belongs to, or `null` for none.
 *
 * The test file names its run in its own URL, and every module of its graph
 * carries that number on, so work a finished file left behind — a timer that
 * imports something late — stays in that file's world rather than joining the
 * next one's.
 */
function runOf(parentURL) {
  if (typeof parentURL !== "string" || !parentURL.startsWith("file:")) return null;
  const query = parentURL.indexOf("?");
  if (query === -1) return null;
  const params = new URLSearchParams(parentURL.slice(query + 1).split("#")[0]);
  return params.get(FILE_PARAM) ?? params.get(RUN_PARAM);
}

/** Whether `path` is an installed package rather than one of the project's. */
function isInstalled(path) {
  return path.includes("/node_modules/") || path.includes("\\node_modules\\");
}

/** Whether `path` is `directory` or somewhere under it. */
function isInside(path, directory) {
  return (
    path === directory || path.startsWith(directory.endsWith("/") ? directory : `${directory}/`)
  );
}
