// @noflow
//
// Plain JavaScript: this is part of the loader, so it cannot be Flow.
//
// The on-disk transform cache, shared by Node's two Flow loaders.
//
// `./sync-hooks.js` runs in the thread that is importing and `./node-hooks.js`
// runs on the loader thread `register()` starts; which of them a process gets
// is `../register.js`'s decision. A module compiled under one and read back
// under the other is the same module, so the two must agree about every byte
// this file decides — the key, the framing, and what a read and a write are
// keyed by — and the only way two loaders stay agreed is by not having two
// copies of the answer.
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
//
// # The two identities
//
// A cached transform is read under one identity of `uf` and written under
// another, on purpose, and both loaders follow the same rule.
//
// The **read** is keyed by the binary as it is *now*, stat'd per module rather
// than once when the hooks were installed. A rebuild between installing them
// and loading the first Flow module would otherwise serve the old build's
// output while the new one is what would run — the same staleness this key
// exists to remove, with a smaller window. A stat is a microsecond and a
// fully warm run still spawns nothing, which is the property that decided the
// key's shape in the first place.
//
// The **write** is keyed by the binary the compiler process is actually
// executing, which `TransformService` read before it spawned and which cannot
// change afterwards. Keying the write by the file's current state would file
// this build's output under the next build's name if the rebuild landed while
// the module was being compiled — the same lie pointing the other way.
//
// They are usually the same string. When they are not, a rebuild happened
// during this run, and each half is right about its own half.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";

import { environmentVariable, inSourceTests } from "../transform.js";
import { writeAtomically } from "../write-atomically.js";

/**
 * Bumped whenever *the loaders'* framing of the output changes, to retire old
 * entries.
 *
 * Not the compiler's version, which is `ufBinaryIdentity` and which nobody
 * has to remember. What is left for this to cover is what the loader adds
 * around a transform — the appended source map, the module format it forces —
 * and that is all it should ever be bumped for.
 */
export const CACHE_VERSION = "3";

/** Where a project's compiled modules live. */
export function cacheDirectoryFor(root) {
  return path.join(root, ".uf", "cache", "transform");
}

/**
 * Whether `filename` is the compiled `uf.config.*` the config loader wrote.
 *
 * It is already JavaScript, and it is the one module that must never go back
 * through the transform: the transform reads the config it came from.
 */
export function isCompiledConfig(filename) {
  return filename.includes(`${path.sep}.uf${path.sep}config${path.sep}uf.config.`);
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

/** What `entry` holds, or `null` when there is no entry or nothing in it yet. */
export function readCached(entry) {
  if (entry == null) return null;
  try {
    return readFileSync(entry, "utf8");
  } catch {
    // not cached yet
    return null;
  }
}

/**
 * Keep a compiled module for the next run.
 *
 * Tolerant: a cache that cannot be written is a slower run, not a failed one —
 * a read-only checkout still works.
 */
export function writeCached(entry, output) {
  if (entry != null) {
    writeAtomically(entry, output, { tolerant: true });
  }
}

/**
 * A transform's reply as the module Node is handed: the code, with its source
 * map appended inline when there is one.
 *
 * Inline, because `--enable-source-maps` reads a `data:` URL without touching
 * the disk again, and because every loader that shares this cache — Node's two,
 * and Deno's, which is the in-thread one — has to frame a module the same way
 * for an entry one of them wrote to be the module another reads.
 */
export function framed(out) {
  return out.map
    ? `${out.code}\n//# sourceMappingURL=data:application/json;base64,${Buffer.from(out.map).toString("base64")}\n`
    : out.code;
}

/**
 * The options every Node loader compiles with, for one module.
 *
 * Development output with a source map, because a stack frame has to name the
 * line the author wrote; `inSourceTests` from the run, which is also in the
 * key above; and the config bootstrap flag the config loader sets on the
 * process that compiles `uf.config.js` itself.
 *
 * The flag is read through `environmentVariable`, which answers "unset" for a
 * variable the process may not read. A Deno worker `uf test` starts is granted
 * the variables uf set on it and nothing else, and a plain `process.env` read
 * of this one threw `NotCapable` out of the first compile of every cold run —
 * measured on Deno 2.9, and a variable no `uf test` worker is ever given.
 */
export function compileOptions(root) {
  return {
    root,
    development: true,
    sourceMap: true,
    inSourceTests: inSourceTests(),
    configBootstrap: environmentVariable("UF_TRANSFORM_BOOTSTRAP_CONFIG") === "1",
  };
}
