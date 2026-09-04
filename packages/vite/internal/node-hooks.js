// @noflow
//
// Plain JavaScript: this *is* the loader, so it cannot be Flow.
//
// Node.js module customization hooks that transform Flow on import.
//
// Registered by `@uniflowed/vite/register` (through `node:module`'s
// `register()`), which makes `node --import @uniflowed/vite/register app.js`
// run a Flow project directly: every `.js` module uf is responsible for is
// transformed as it is loaded through `uf transform`, and everything else is
// left to Node.
//
// Transforms are cached on disk under `.uf/cache/transform/` keyed by a hash
// of the source, so a second run of the same file is a read rather than a
// round trip. The cache is content-addressed: an edited file hashes
// differently, so there is no invalidation to get wrong.

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, renameSync, unlinkSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { isFlowModule, transformFlow } from "../transform.js";

/**
 * Write `contents` to `target` so a concurrent reader never sees half of it.
 *
 * `uf test` runs one of these processes per core and they all import the same
 * few modules at once, so two writers and a reader meet on the same cache
 * entry constantly. `writeFileSync` is not atomic — a reader can observe a
 * truncated file and report a module that "does not provide an export" — so
 * the content goes to a private temporary name first and is then renamed,
 * which is atomic within a filesystem.
 *
 * A failure here is not a failure: a read-only checkout still runs, just
 * without the cache.
 */
function writeAtomically(target, contents) {
  const temporary = `${target}.${process.pid}.${Math.random().toString(36).slice(2)}`;
  try {
    mkdirSync(cacheDirectory, { recursive: true });
    writeFileSync(temporary, contents);
    renameSync(temporary, target);
  } catch {
    try {
      unlinkSync(temporary);
    } catch {
      // Nothing to clean up.
    }
  }
}

/** Bumped whenever the transform's output shape changes, to retire old entries. */
const CACHE_VERSION = "2";

let cacheDirectory = null;
let root = null;

/**
 * Called once by `register()` with `{ root }`; the cache lives under it and
 * the transform service is started there so it reads the right config.
 */
export async function initialize(data) {
  root = data?.root ?? process.cwd();
  cacheDirectory = path.join(root, ".uf", "cache", "transform");
}

/**
 * The `load` hook: transform Flow modules, defer everything else.
 */
export async function load(url, context, nextLoad) {
  if (!url.startsWith("file:")) return nextLoad(url, context);
  const filename = fileURLToPath(url);
  if (!isFlowModule(filename)) return nextLoad(url, context);

  const source = readFileSync(filename, "utf8");
  const code = await cachedTransform(source, filename);
  if (code == null) return nextLoad(url, context);
  // uf projects are ES modules. Forcing the format here means a project whose
  // package.json forgot `"type": "module"` still runs, rather than failing on
  // an `import` in what Node would have guessed was CommonJS.
  return { format: "module", source: code, shortCircuit: true };
}

async function cachedTransform(source, filename) {
  const key = createHash("sha256")
    .update(CACHE_VERSION)
    .update("\0")
    .update(filename)
    .update("\0")
    .update(source)
    .digest("hex");
  const entry = cacheDirectory ? path.join(cacheDirectory, `${key}.mjs`) : null;

  if (entry) {
    try {
      return readFileSync(entry, "utf8");
    } catch {
      // not cached yet
    }
  }

  const out = await transformFlow(source, filename, { root, development: true, sourceMap: true });
  if (out == null) return null;
  const output = out.map
    ? `${out.code}\n//# sourceMappingURL=data:application/json;base64,${Buffer.from(out.map).toString("base64")}\n`
    : out.code;

  if (entry) {
    writeAtomically(entry, output);
  }
  return output;
}
