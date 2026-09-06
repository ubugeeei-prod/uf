// @flow
//
// Top-level `await`, through the loader that actually runs it.
//
// ubugeeei-prod/uf#204. `await` at the top level of a module is ES2022 and
// Node has run it for years; uf's own parser read it as the identifier
// `await`, so `uf check`, `uf fmt`, `uf lint` and — the one this file is about
// — `packages/host` all refused a module Node would have run. The unit tests
// beside the parser say the tree is right. This says the module runs.
//
// Nothing here is a stand-in: the real `register.js`, the real hooks, the real
// `uf transform` (`UF_BINARY`, which the test host pins to the binary running
// this suite), a real Node process. The module is written with a Flow
// annotation on the awaited value so that a run proves the transform saw it —
// a file that reached Node untransformed would die on the `: number`.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "@uniflowed/test";

/** The loader under test, reached as a path so no resolution is involved. */
const REGISTER: string = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../packages/host/register.js",
);

/** What one run of a module said. */
type Result = { status: number | null, stdout: string, stderr: string };

/**
 * Write `files` into a throwaway project, run `entry` under the real loader,
 * and take the project away afterwards.
 *
 * Each project gets its own directory, so each gets its own
 * `.uf/cache/transform` and no run is served what another one compiled.
 */
const runModule = (files: { [string]: string }, entry: string): Result => {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-top-level-await-")));
  try {
    for (const name of Object.keys(files)) {
      fs.writeFileSync(path.join(root, name), files[name]);
    }
    const result = spawnSync(process.execPath, ["--import", REGISTER, path.join(root, entry)], {
      cwd: root,
      env: { ...process.env, UF_PROJECT_ROOT: root },
      encoding: "utf8",
    });
    return { status: result.status, stdout: result.stdout, stderr: result.stderr };
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
};

describe("a module that awaits at its top level", () => {
  it("runs", () => {
    const result = runModule(
      {
        "settings.js": "// @flow\nexport const load = (): Promise<number> => Promise.resolve(3);\n",
        "main.js":
          "// @flow\n" +
          'import { load } from "./settings.js";\n' +
          "const retries: number = await load();\n" +
          "process.stdout.write(`retries=${retries}`);\n",
      },
      "main.js",
    );

    expect(result.stderr).toBe("");
    expect(result.status).toBe(0);
    // Not merely "it did not fail": the awaited value is the resolved one, so
    // this is 3 and not `[object Promise]`.
    expect(result.stdout).toBe("retries=3");
  });

  it("runs a top-level `for await`", () => {
    const result = runModule(
      {
        "rows.js":
          "// @flow\n" +
          "export async function* rows(): AsyncGenerator<string, void, void> {\n" +
          '  yield "a";\n' +
          '  yield "b";\n' +
          "}\n",
        "main.js":
          "// @flow\n" +
          'import { rows } from "./rows.js";\n' +
          "for await (const row of rows()) {\n" +
          "  process.stdout.write(row);\n" +
          "}\n",
      },
      "main.js",
    );

    expect(result.stderr).toBe("");
    expect(result.status).toBe(0);
    expect(result.stdout).toBe("ab");
  });

  it("is refused, in uf's own words, where the language still refuses it", () => {
    // No `import` and no `export`, so this is a script and `await` is an
    // ordinary identifier. The transform says so rather than letting the
    // parser point at the operand, which is a token that is fine.
    const result = runModule(
      { "main.js": "// @flow\nconst retries: number = await Promise.resolve(3);\n" },
      "main.js",
    );

    expect(result.status).not.toBe(0);
    expect(result.stderr).toContain("only allowed at the top level of a module");
  });
});
