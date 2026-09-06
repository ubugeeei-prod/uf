// @noflow
//
// Plain JavaScript: this is reached from the loader hooks, which run before
// any transform, and from the config loader, which runs before them.
//
// Writing a file a second process may be reading at the same time.
//
// `writeFileSync` truncates and then writes, so a reader can observe the empty
// file or half of one. When what is being written is a module, the reader does
// not get an error it can act on — it gets a module with no exports, and says
// so about the *source*:
//
//     uf: uf.config.js must `export default defineConfig({ ... })`
//
// which is a sentence about a file that is perfectly correct. That is not
// hypothetical: two `uf` commands in one project, which is `uf dev` in one
// terminal and `uf build` in another, produced exactly it. See
// ubugeeei-prod/uf#240.
//
// Writing to a private name and renaming is atomic within a filesystem, so a
// reader sees the old file or the new one and never a part of either.

import { mkdirSync, renameSync, unlinkSync, writeFileSync } from "node:fs";
import path from "node:path";

/**
 * Write `contents` to `target` so a concurrent reader never sees half of it.
 *
 * The temporary name carries the process id and a random suffix, because two
 * processes racing to write the *same* target is the case this exists for and
 * they must not collide on the temporary either.
 *
 * @param {string} target absolute path to write
 * @param {string} contents what to write
 * @param {{ tolerant?: boolean }} [options] `tolerant` swallows a failure,
 *   which is what a cache wants — a read-only checkout still runs, just
 *   without one. A config that cannot be written is a failure the caller has
 *   to see.
 */
export function writeAtomically(target, contents, options = {}) {
  const temporary = `${target}.${process.pid}.${Math.random().toString(36).slice(2)}`;
  try {
    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(temporary, contents);
    renameSync(temporary, target);
  } catch (error) {
    try {
      unlinkSync(temporary);
    } catch {
      // Nothing to clean up.
    }
    if (options.tolerant !== true) throw error;
  }
}
