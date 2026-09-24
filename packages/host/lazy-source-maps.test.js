// @flow
//
// Stacks mapped when they are read: `internal/lazy-source-maps.js`.
//
// A `uf test` worker on Node hands V8 each module without its inline source
// map and maps a stack only when one is formatted. The promise is that nobody
// can tell: the stack a module prints is the one Node's own
// `--enable-source-maps` prints for it, frame for frame, and a registration's
// call site is mapped as Node would have mapped it. So the same Flow module is
// run both ways, through the real loader and the real `uf transform`, and the
// two outputs are compared.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "@uniflowed/test";

/** The loader under test, reached as a path so no resolution is involved. */
const REGISTER: string = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "./register.js",
);

const HELPER = `// @flow
type Box = {| value: number |};
export function explode(box: Box): empty {
  class Thing {
    go(): empty {
      throw new TypeError(\`bad \${box.value}\`);
    }
  }
  return new Thing().go();
}
export async function later(): Promise<void> {
  await null;
  explode({ value: 2 });
}
`;

const MAIN = `// @flow
import { explode, later } from "./helper.js";
const annotated: number = 1;
try {
  explode({ value: annotated });
} catch (error) {
  process.stdout.write(String(error.stack) + "\\n--\\n");
}
try {
  await later();
} catch (error) {
  process.stdout.write(String(error.stack) + "\\n--\\n");
}
const found = globalThis[Symbol.for("@uniflowed/host/source-maps")];
process.stdout.write(found == null ? "node maps\\n" : "lazy maps\\n");
`;

/** Run the project once, lazily or not, and hand back what it printed. */
function run(root: string, lazy: boolean): string {
  const env: { [string]: string | void } = { ...process.env, UF_PROJECT_ROOT: root };
  delete env.UF_TEST_LAZY_SOURCE_MAPS;
  if (lazy) env.UF_TEST_LAZY_SOURCE_MAPS = "1";
  const result = spawnSync(
    "node",
    ["--enable-source-maps", "--import", REGISTER, path.join(root, "main.js")],
    { cwd: root, env, encoding: "utf8" },
  );
  expect(result.stderr).toBe("");
  expect(result.status).toBe(0);
  return result.stdout;
}

describe("a stack mapped when it is read", () => {
  it("is the stack Node's own source maps print", () => {
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-lazy-maps-")));
    try {
      fs.writeFileSync(path.join(root, "helper.js"), HELPER);
      fs.writeFileSync(path.join(root, "main.js"), MAIN);
      // Once to fill the transform cache, so both runs below read the same
      // compiled modules.
      run(root, false);
      const eager = run(root, false);
      const lazy = run(root, true);

      expect(eager.endsWith("node maps\n")).toBe(true);
      expect(lazy.endsWith("lazy maps\n")).toBe(true);
      const stacks = (out: string) => out.slice(0, out.lastIndexOf("--\n"));
      expect(stacks(lazy)).toBe(stacks(eager));
      // And it is mapped at all: the throw is on line 6 of the Flow source,
      // and the call into it on line 5 of the entry.
      expect(stacks(lazy)).toContain(`${path.join(root, "helper.js")}:6:`);
      expect(stacks(lazy)).toContain(`${path.join(root, "main.js")}:5:`);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
