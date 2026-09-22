// @noflow
//
// Plain JavaScript: this *is* the loader, so it cannot be Flow.
//
// Node.js module customization hooks that transform Flow on import, on the
// loader thread `node:module`'s `register()` starts.
//
// Every `.js` module uf is responsible for is transformed as it is loaded
// through `uf transform`, and everything else is left to Node. What a
// transform is cached under, and why, is `./flow-cache.js`, which this loader
// and `./sync-hooks.js` both read and write through.
//
// # Who still gets this loader
//
// `@uniflowed/host/register` installs these only on a Node without
// `registerHooks`. Where Node has it, the same work runs in the importing
// thread (`./sync-hooks.js`), which is cheaper by a thread per process and a
// round trip per module — that file has the measurements — and which `uf test`
// pays for once per worker.
//
// `@uniflowed/vite`'s driver registers these directly. It is one long-lived
// process per `uf dev` or `uf build`, so the thread is paid once, and its
// transforms are concurrent in a way an in-thread hook's cannot be: Vite
// imports many modules at once, and a hook that blocks its thread would compile
// them one at a time.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  environmentVariable,
  isFlowModule,
  sharedService,
  transformFlow,
  ufBinaryIdentity,
} from "../transform.js";
import {
  cacheDirectoryFor,
  cacheEntryFor,
  compileOptions,
  framed,
  isCompiledConfig,
  readCached,
  writeCached,
} from "./flow-cache.js";

let cacheDirectory = null;
let root = null;

/**
 * Called once by `register()` with `{ root }`; the cache lives under it and
 * the transform service is started there so it reads the right config.
 */
export async function initialize(data) {
  root = data?.root ?? process.cwd();
  cacheDirectory = cacheDirectoryFor(root);
}

const TEST_COMPILER_RUNTIME_URL =
  "data:text/javascript;charset=utf-8," +
  encodeURIComponent(`const sentinel = Symbol.for("react.memo_cache_sentinel");
export function c(size) {
  const cache = new Array(size);
  for (let index = 0; index < size; index += 1) cache[index] = sentinel;
  return cache;
}
`);

function isTestCompilerRuntime(specifier) {
  return (
    specifier === "react/compiler-runtime" && environmentVariable("UF_IN_SOURCE_TESTS") === "1"
  );
}

/**
 * The `resolve` hook: while `uf test` is running Flow-compiled modules
 * without Vite's RSC graph, supply the compiler's memo-cache helper directly.
 */
export async function resolve(specifier, context, nextResolve) {
  try {
    return await nextResolve(specifier, context);
  } catch (error) {
    if (isTestCompilerRuntime(specifier) && error.code === "ERR_MODULE_NOT_FOUND") {
      return { url: TEST_COMPILER_RUNTIME_URL, shortCircuit: true };
    }
    throw error;
  }
}

/**
 * The `load` hook: transform Flow modules, defer everything else.
 */
export async function load(url, context, nextLoad) {
  if (!url.startsWith("file:")) return nextLoad(url, context);
  const filename = fileURLToPath(url);
  if (isCompiledConfig(filename)) return nextLoad(url, context);
  if (!isFlowModule(filename)) return nextLoad(url, context);

  const source = readFileSync(filename, "utf8");
  const code = await cachedTransform(source, filename);
  if (code == null) return nextLoad(url, context);
  // uf projects are ES modules. Forcing the format here means a project whose
  // package.json forgot `"type": "module"` still runs, rather than failing on
  // an `import` in what Node would have guessed was CommonJS.
  return { format: "module", source: code, shortCircuit: true };
}

/**
 * The compiled form of one module, from disk if some build already produced
 * it and from `uf` otherwise.
 *
 * Read under the binary as it is now and written under the binary the service
 * is executing — "The two identities" in `./flow-cache.js` says why those are
 * two different keys.
 */
async function cachedTransform(source, filename) {
  const cached = readCached(cacheEntryFor(cacheDirectory, ufBinaryIdentity(), source, filename));
  if (cached != null) return cached;

  const options = compileOptions(root);
  const out = await transformFlow(source, filename, options);
  if (out == null) return null;
  const output = framed(out);

  const service = sharedService(root, { configBootstrap: options.configBootstrap });
  writeCached(cacheEntryFor(cacheDirectory, service.identity, source, filename), output);
  return output;
}
