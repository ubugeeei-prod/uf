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
// same file is a read rather than a round trip.
//
// Both halves are load-bearing. The key was the source alone at first, on the
// reasoning that a content-addressed cache has no invalidation to get wrong —
// which quietly assumed the compiler was a constant. It is not: edit
// `crates/uf_transform` or `crates/uf_stylex`, rebuild, run `uf test`, and
// every module whose *source* had not changed came back as the previous
// binary had compiled it. The suite then passed, or failed, for the previous
// build's reasons, and the only symptom was an answer that made no sense.
// `rm -rf .uf/cache/transform` was the cure, and finding that out cost a
// debugging session while `@uniflowed/stylex`'s preset was being written.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { isFlowModule, sharedService, transformFlow, ufBinaryIdentity } from "../transform.js";
import { writeAtomically } from "../write-atomically.js";

/**
 * Bumped whenever *this file's* framing of the output changes, to retire old
 * entries.
 *
 * Not the compiler's version, which is `ufBinaryIdentity` and which nobody
 * has to remember. What is left for this to cover is what the loader adds
 * around a transform — the appended source map, the module format it forces —
 * and that is all it should ever be bumped for.
 */
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

/**
 * The file this module's compiled form belongs in under `identity`, or `null`
 * when it must not be cached at all.
 *
 * `null` when there is no cache directory, and — the case worth spelling out —
 * when the caller has no identity to give: nothing is read and nothing is
 * written. Hashing the rest anyway would give every build of `uf` one key
 * again, and writing under it would leave an entry for the next run to trust.
 * A host that cannot name its compiler compiles everything, every time, which
 * is slower and is never wrong.
 */
function cacheEntryFor(identity, source, filename) {
  if (cacheDirectory == null || identity == null) return null;
  const key = createHash("sha256")
    .update(CACHE_VERSION)
    .update("\0")
    .update(identity)
    .update("\0")
    .update(filename)
    .update("\0")
    .update(source)
    .digest("hex");
  return path.join(cacheDirectory, `${key}.mjs`);
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
  const entry = cacheEntryFor(ufBinaryIdentity(), source, filename);

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

  const written = cacheEntryFor(sharedService(root).identity, source, filename);
  if (written) {
    // Tolerant: a cache that cannot be written is a slower run, not a
    // failed one — a read-only checkout still works.
    writeAtomically(written, output, { tolerant: true });
  }
  return output;
}
