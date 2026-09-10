// @flow
//
// Writing a file a second process may be reading at the same time.
//
// A person runs `uf dev` in one terminal and `uf build` in another, and
// nothing tells them not to. Both compile `uf.config.js` to
// `.uf/config/uf.config.<hash of the source>.mjs` — the same path, because
// they agree on the hash — and then import it.
//
// `writeFileSync` truncates before it writes, so the second process could
// import that file while the first was inside that call. What it got was not
// an error it could act on. It was a module with no exports, reported as
//
//     uf: uf.config.js must `export default defineConfig({ ... })`
//
// a sentence about a file that is perfectly correct. It took down
// `build_renders_the_docs_site_through_vite` on two unrelated pull requests
// before anybody read it as a race. See ubugeeei-prod/uf#240.
//
// `packages/host/internal/node-hooks.js` had learned this already — its cache
// writes went through a `writeAtomically` whose comment names this exact
// symptom — and the config loader had not. It is one helper now, and this is
// its test.
//
// The test races real processes, because the race is between processes: within
// one, the writes are ordered by the event loop and nothing overlaps.

import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";

/** This checkout, found by the file under test rather than by counting `..`. */
const repository: string = (() => {
  const wanted = path.join("packages", "host", "write-atomically.js");
  let directory = process.env.UF_PROJECT_ROOT ?? process.cwd();
  for (let up = 0; up < 8; up += 1) {
    if (fs.existsSync(path.join(directory, wanted))) return directory;
    directory = path.dirname(directory);
  }
  throw new Error(`could not find ${wanted} above ${process.cwd()}`);
})();

/** Big enough that a write takes more than one syscall to land. */
const SIZE = 512 * 1024;

/** How many times each writer rewrites the file. */
const ROUNDS = 40;

/**
 * A writer process: `ROUNDS` rewrites of `target`, each a different byte.
 *
 * Each round's content is one repeated character, so a reader can tell a whole
 * file from two halves of different ones without knowing which round it caught
 * — the check is "every byte is the same and there are enough of them", which
 * a truncated or half-overwritten file fails.
 */
const writer = (target: string): string => `
import { writeAtomically } from ${JSON.stringify(path.join(repository, "packages/host/write-atomically.js"))};
const alphabet = "abcdefghijklmnopqrstuvwxyz";
for (let round = 0; round < ${ROUNDS}; round += 1) {
  writeAtomically(${JSON.stringify(target)}, alphabet[round % alphabet.length].repeat(${SIZE}));
}
`;

/**
 * A reader process: read `target` until the writers stop, and report anything
 * that was not a whole file.
 *
 * A missing file is not a fault — the first write has not landed yet. A file
 * of the wrong length, or one holding two different characters, is: that is
 * half of one round beside half of another, which is what an importer sees as
 * a module with no exports.
 */
const reader = (target: string): string => `
const deadline = Date.now() + 15_000;
let seen = 0;
while (Date.now() < deadline) {
  let text;
  try {
    text = require("node:fs").readFileSync(${JSON.stringify(target)}, "utf8");
  } catch {
    continue;
  }
  seen += 1;
  if (text.length !== ${SIZE} || text !== text[0].repeat(text.length)) {
    process.stdout.write("torn:" + text.length);
    process.exit(0);
  }
  if (seen > 400) break;
}
process.stdout.write("whole:" + seen);
`;

/** Run a script and resolve with what it said. */
const run = (script: string): Promise<{ out: string, err: string }> =>
  new Promise((resolve) => {
    const child = spawn(process.execPath, [script], { encoding: "utf8" });
    let out = "";
    let err = "";
    child.stdout.on("data", (chunk) => {
      out += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      err += String(chunk);
    });
    child.on("close", () => resolve({ out, err }));
  });

describe("writing a file somebody else is reading", () => {
  it("is never observed half written", async () => {
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-atomic-")));
    try {
      const target = path.join(root, "module.mjs");
      const writerScript = path.join(root, "writer.mjs");
      const readerScript = path.join(root, "reader.cjs");
      fs.writeFileSync(writerScript, writer(target));
      fs.writeFileSync(readerScript, reader(target));

      // Started together and left to overlap — two writers rather than one,
      // because the target is a path two commands agree on, so they race each
      // other as well as the reader.
      const [first, second, read] = await Promise.all([
        run(writerScript),
        run(writerScript),
        run(readerScript),
      ]);

      expect(first.err).toBe("");
      expect(second.err).toBe("");
      expect(read.err).toBe("");
      expect(read.out.startsWith("whole:")).toBe(true);
      // And it did read something: a reader that found nothing proves nothing.
      expect(Number(read.out.slice("whole:".length))).toBeGreaterThan(0);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
