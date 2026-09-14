// @noflow
//
// Plain JavaScript: this *is* the loader, so it cannot be Flow.
//
// Deno's Flow loader: `node:module`'s `registerHooks` with a `load` hook that
// transforms every module uf is responsible for through `uf transform` as Deno
// asks for it. Installed at process start by `../deno-preload.js`, and by
// `@uniflowed/vite`'s driver at the point it installs Node's hooks.
//
// # Why synchronous, and what that costs
//
// Node's `register()` runs asynchronous hooks on a loader thread of their own.
// Deno has never implemented `register()`. What it implements, from 2.8, is
// `registerHooks()` — the synchronous hooks Node added later, which run in the
// thread doing the importing and must return a module's source rather than a
// promise of it. The transform service in `../transform.js` answers over a pipe
// read asynchronously, and a synchronous hook cannot wait on that without
// blocking the event loop that would deliver the reply. So a module the cache
// does not already hold is compiled by `transformFlowSync` — one short-lived
// `uf transform` per module, same binary and same protocol — and a warm run
// starts none, because the cache is read first and its key is the Node
// loader's (`./transform-cache.js`).
//
// # The one thing a load hook breaks on Deno, and where that leaves the driver
//
// Measured on Deno 2.9: while *any* `load` hook is registered — including one
// that only calls `nextLoad` — `require()` of a native `.node` addon fails
// with `Invalid or unexpected token`, because the addon's bytes are compiled as
// script. A `resolve`-only hook does not do it, and no answer a `load` hook can
// give for the addon (a `format` of `addon`, a CommonJS shim over
// `process.dlopen`) is taken. So a process that needs a native addon has to
// have loaded it before this is installed. `@uniflowed/vite`'s driver does:
// Vite, and Rolldown's binding with it, are static imports of the driver, and
// the hooks go in afterwards — the same moment Node's `register()` is called.
// An addon first required *after* installation still fails, and that is
// Deno's to fix. See ubugeeei-prod/uf#246.

import { readFileSync } from "node:fs";
import * as nodeModule from "node:module";
import { fileURLToPath } from "node:url";

import { inSourceTests, isFlowModule, transformFlowSync, ufBinaryIdentity } from "../transform.js";
import {
  cacheEntryFor,
  frameCompiledModule,
  isCompiledConfig,
  readCachedModule,
  rememberCompiledModule,
  transformCacheDirectory,
} from "./transform-cache.js";

/** The first Deno with `registerHooks`, as a person reads it. */
export const DENO_WITH_HOOKS = "2.8";

/**
 * Install the Flow loader in this Deno process, and return the handle
 * `registerHooks` gave.
 *
 * `root` is the project whose `.uf/cache/transform` the compiled modules are
 * kept in, and where `uf transform` is started so it reads the right
 * `uf.config.js`; `UF_PROJECT_ROOT` and then the working directory when it is
 * not given.
 *
 * Throws when this runtime has no `registerHooks`, naming the release that
 * added it — a namespace import rather than `import { registerHooks }`,
 * because a named import of an export a runtime does not have is a link error
 * that names the export and not the reason.
 */
export function registerDenoFlowHooks(options = {}) {
  if (typeof nodeModule.registerHooks !== "function") {
    const version = globalThis.Deno?.version?.deno ?? "an unknown version";
    throw new Error(
      `uf loads Flow on Deno through \`registerHooks\` from \`node:module\`, which Deno added in ` +
        `${DENO_WITH_HOOKS}, and this is Deno ${version}. Run \`deno upgrade\`, or run the project ` +
        "on Node.js or Bun.",
    );
  }

  const root = options.root ?? variable("UF_PROJECT_ROOT") ?? process.cwd();
  const cacheDirectory = transformCacheDirectory(root);

  return nodeModule.registerHooks({
    load(url, context, nextLoad) {
      if (!url.startsWith("file:")) return nextLoad(url, context);
      // `fileURLToPath` reads the path and not the query, so the test worker's
      // `?uf-run=<generation>` and a mock's epoch both name the file they are on.
      const filename = fileURLToPath(url);
      if (isCompiledConfig(filename)) return nextLoad(url, context);
      if (!isFlowModule(filename)) return nextLoad(url, context);

      const source = readFileSync(filename, "utf8");
      const code = compiled(cacheDirectory, root, source, filename);
      if (code == null) return nextLoad(url, context);
      // uf projects are ES modules, and saying so here is what lets a project
      // whose package.json forgot `"type": "module"` still run — the reason
      // `./node-hooks.js` gives for the same line.
      return { format: "module", source: code, shortCircuit: true };
    },
  });
}

/**
 * The compiled form of one module: from the cache when a build of `uf` already
 * produced it, and from `uf transform` otherwise.
 *
 * Both keys are the binary as it is now. `./node-hooks.js` keys its write by
 * the identity its long-lived service read before spawning, because a rebuild
 * can land while that service is still answering; here the process that
 * compiled the module has exited by the time the answer is written, and it ran
 * the file whose identity was read a moment before it started.
 */
function compiled(cacheDirectory, root, source, filename) {
  const entry = cacheEntryFor(cacheDirectory, ufBinaryIdentity(), source, filename);
  const cached = readCachedModule(entry);
  if (cached != null) return cached;

  const out = transformFlowSync(source, filename, {
    root,
    development: true,
    sourceMap: true,
    inSourceTests: inSourceTests(),
    configBootstrap: variable("UF_TRANSFORM_BOOTSTRAP_CONFIG") === "1",
  });
  if (out == null) return null;
  const output = frameCompiledModule(out);
  rememberCompiledModule(entry, output);
  return output;
}

/**
 * One environment variable, or `undefined` when it is unset *or* this process
 * may not read it.
 *
 * Deno denies by default, and a run `uf test` starts is granted the variables
 * uf set and nothing else. Reading one outside that list throws `NotCapable`
 * rather than answering `undefined`, and a loader that took the process down
 * over a variable it only consults would be failing a suite over nothing.
 */
function variable(name) {
  try {
    return process.env[name];
  } catch {
    return undefined;
  }
}
