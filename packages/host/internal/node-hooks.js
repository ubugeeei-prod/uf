// @noflow
//
// Plain JavaScript: this *is* the loader, so it cannot be Flow.
//
// Node.js module customization hooks that transform Flow on import.
//
// Registered by `@uniflowed/host/register` (through `node:module`'s
// `register()`), which makes `node --import @uniflowed/host/register app.js`
// run a Flow project directly: every `.js` module uf is responsible for is
// transformed as it is loaded through `uf transform`, and everything else is
// left to Node.
//
// Transforms are cached on disk under `.uf/cache/transform/` keyed by a hash
// of the source *and* of the `uf` that compiled it, so a second run of the
// same file is a read rather than a round trip. The key and the framing live
// in `./transform-cache.js`, because Deno's loader (`../deno-preload.js`) reads
// and writes the same directory and has to agree with this one byte for byte.
//
// Both halves of the key are load-bearing. The key was the source alone at
// first, on the reasoning that a content-addressed cache has no invalidation
// to get wrong — which quietly assumed the compiler was a constant. It is not:
// edit `crates/uf_transform` or `crates/uf_stylex`, rebuild, run `uf test`,
// and every module whose *source* had not changed came back as the previous
// binary had compiled it. The suite then passed, or failed, for the previous
// build's reasons, and the only symptom was an answer that made no sense.
// `rm -rf .uf/cache/transform` was the cure, and finding that out cost a
// debugging session while `@uniflowed/stylex`'s preset was being written.
//
// # What takes entries back out, and why it is not here
//
// Nothing in this file. Every write below is an addition — a source edit
// orphans one entry the moment it lands, and a rebuild of `uf` orphans a whole
// generation at once — so the directory only ever grew, at about 330 modules
// and 9 MB per build of this repository, until `uf` itself started bounding
// it: `uf transform` sweeps this directory to 128 MiB, coldest entries first,
// when it starts. See `uf_infra::cache` for the policy and ubugeeei-prod/uf#218 for the
// three options it was chosen from.
//
// It belongs there rather than here for two reasons that point the same way. A
// `readdir` and a `stat` over thousands of entries, in JavaScript, on every
// host start is the repository-wide work uf's own guide says must be native.
// And `uf transform` is started only when something actually has to be
// compiled, which is the only thing that adds to this directory — so the sweep
// happens exactly when it can have grown, and a fully warm run, which spawns
// no `uf` at all, pays nothing and needs to pay nothing.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import {
  inSourceTests,
  isFlowModule,
  sharedService,
  transformFlow,
  ufBinaryIdentity,
} from "../transform.js";
import {
  cacheEntryFor,
  frameCompiledModule,
  isCompiledConfig,
  readCachedModule,
  rememberCompiledModule,
  transformCacheDirectory,
} from "./transform-cache.js";

let cacheDirectory = null;
let root = null;

/**
 * Called once by `register()` with `{ root }`; the cache lives under it and
 * the transform service is started there so it reads the right config.
 */
export async function initialize(data) {
  root = data?.root ?? process.cwd();
  cacheDirectory = transformCacheDirectory(root);
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
 * The two keys are computed from two different identities on purpose.
 *
 * The **read** is keyed by the binary as it is *now*, stat'd per module rather
 * than once when the hooks were installed. A rebuild between installing them
 * and loading the first Flow module would otherwise serve the old build's
 * output while the new one is what would run — the same staleness this key
 * exists to remove, with a smaller window. A stat is a microsecond and a
 * fully warm run still spawns nothing, which is the property that decided the
 * key's shape in the first place.
 *
 * The **write** is keyed by the binary the compiler process is actually
 * executing, which `sharedService` read before it spawned and which cannot
 * change afterwards. Keying the write by the file's current state would file
 * this build's output under the next build's name if the rebuild landed while
 * the module was being compiled — the same lie pointing the other way.
 *
 * They are usually the same string. When they are not, a rebuild happened
 * during this run, and each half is right about its own half.
 */
async function cachedTransform(source, filename) {
  const cached = readCachedModule(
    cacheEntryFor(cacheDirectory, ufBinaryIdentity(), source, filename),
  );
  if (cached != null) return cached;

  const configBootstrap = process.env.UF_TRANSFORM_BOOTSTRAP_CONFIG === "1";
  const out = await transformFlow(source, filename, {
    root,
    development: true,
    sourceMap: true,
    inSourceTests: inSourceTests(),
    configBootstrap,
  });
  if (out == null) return null;
  const output = frameCompiledModule(out);

  rememberCompiledModule(
    cacheEntryFor(
      cacheDirectory,
      sharedService(root, { configBootstrap }).identity,
      source,
      filename,
    ),
    output,
  );
  return output;
}
