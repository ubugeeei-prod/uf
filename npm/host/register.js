// @noflow
//
// Plain JavaScript: this file registers the loader, so it cannot need one.
//
// `node --import @uniflowed/host/register app.js` runs a Flow project on
// Node.js without a build step. Importing this module installs a Flow loader
// for the rest of the process: every module uf is responsible for is
// transformed as it is loaded, and everything else is left to Node.
//
// Which loader depends on the Node. Where `node:module` has `registerHooks` —
// 22.15, and 23.5 onward — the hooks run in the importing thread
// (`./internal/sync-hooks.js`). Anywhere else they run on the loader thread
// `register()` starts (`./internal/node-hooks.js`). Both read and write one
// cache through `./internal/flow-cache.js`, so which of them compiled a module
// is never visible in the module.
//
// The in-thread loader is preferred, and not for tidiness. A loader thread is
// a second V8 isolate started before the program's first line and a round trip
// for every module it imports, and `uf test` starts a Node process per worker:
// on the suite `docs/app/guide/testing` measures, it was a third of every
// worker's start-up. Node 26 also deprecates `register()`, and says so on
// stderr from every process that calls it.

import * as nodeModule from "node:module";

import { installFlowHooks } from "./internal/sync-hooks.js";

const root = process.env.UF_PROJECT_ROOT ?? process.cwd();

if (typeof nodeModule.registerHooks === "function") {
  installFlowHooks(root);
} else {
  nodeModule.register("./internal/node-hooks.js", import.meta.url, { data: { root } });
}
