// @noflow
//
// Plain JavaScript: both Flow loaders import this, so it cannot need one.
//
// The on-disk transform cache under `.uf/cache/transform/`, as one module that
// `./node-hooks.js` and `../deno-preload.js` both read and write through.
//
// # Why this is shared rather than written twice
//
// The two loaders share a directory. A project run with `uf test` on Node and
// then on Deno — or a machine whose `app.runtime.capabilityJsHost` changed —
// reads entries the other host wrote, and that is only correct while both
// hosts compute the same key for the same module and frame the compiled code
// the same way. Two copies of the key would be two things to keep equal, and
// the failure when they drift is not a miss but a *hit on the wrong entry*: a
// module served in the other host's framing, or a cache that two hosts keep
// overwriting. One function is the only way the property holds by
// construction.
//
// The compiled bytes are the same on both hosts because nothing in them is
// host-specific: `uf transform` does not know which runtime asked, and the
// framing below — the source map appended as a data URL — is read by Node's
// `--enable-source-maps` and ignored harmlessly elsewhere.
//
// # What the key is made of, and why each part is there
//
// See `./node-hooks.js` for the history: the key was the source alone at
// first, and a rebuilt `uf` went on serving what the previous build had
// compiled. So it names the compiler (`ufBinaryIdentity`), every transform
// option that varies between commands sharing the directory (in-source tests),
// the file, and the source.

import { Buffer } from "node:buffer";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";

import { inSourceTests } from "../transform.js";
import { writeAtomically } from "../write-atomically.js";

/**
 * Bumped whenever the *loaders'* framing of the output changes, to retire old
 * entries.
 *
 * Not the compiler's version, which is `ufBinaryIdentity` and which nobody
 * has to remember. What is left for this to cover is what a loader adds
 * around a transform — the appended source map, the module format it forces —
 * and that is all it should ever be bumped for. Shared by both loaders, so a
 * bump retires both hosts' entries at once, which is the point.
 */
export const CACHE_VERSION = "3";

/**
 * Whether this is `uf.config.js` as uf compiled it for evaluation.
 *
 * The one module both loaders decline before they consult the cache: it is
 * already JavaScript, and transforming it again would be transforming uf's own
 * output. Here rather than in either loader so the two hosts cannot decline
 * different files.
 */
export function isCompiledConfig(filename) {
  return filename.includes(`${path.sep}.uf${path.sep}config${path.sep}uf.config.`);
}

/** Where one project's compiled modules are kept. */
export function transformCacheDirectory(root) {
  return path.join(root, ".uf", "cache", "transform");
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
export function cacheEntryFor(directory, identity, source, filename) {
  if (directory == null || identity == null) return null;
  const key = createHash("sha256")
    .update(CACHE_VERSION)
    .update("\0")
    .update(identity)
    .update("\0")
    // Every transform option that changes the output has to be in the key,
    // and this is the first one that varies between two commands sharing a
    // cache directory. `uf test` compiles `import.meta.uf.test` to uf's test
    // API and `uf run` compiles it to `void 0`; without this byte the second
    // command to touch a module would be served the first one's answer, and
    // the symptom would be an in-source test that ran or did not depending on
    // what somebody had typed earlier in the day.
    .update(inSourceTests() ? "in-source" : "plain")
    .update("\0")
    .update(filename)
    .update("\0")
    .update(source)
    .digest("hex");
  return path.join(directory, `${key}.mjs`);
}

/** A cached compiled module, or `null` when there is none to read. */
export function readCachedModule(entry) {
  if (entry == null) return null;
  try {
    return readFileSync(entry, "utf8");
  } catch {
    // not cached yet
    return null;
  }
}

/**
 * A transform's answer as a loader hands it to the host: the code, with the
 * source map appended as a data URL when there is one.
 */
export function frameCompiledModule(out) {
  return out.map
    ? `${out.code}\n//# sourceMappingURL=data:application/json;base64,${Buffer.from(out.map).toString("base64")}\n`
    : out.code;
}

/**
 * Keep one compiled module for the next run.
 *
 * Tolerant: a cache that cannot be written is a slower run, not a failed one —
 * a read-only checkout still works.
 */
export function rememberCompiledModule(entry, output) {
  if (entry == null) return;
  writeAtomically(entry, output, { tolerant: true });
}
