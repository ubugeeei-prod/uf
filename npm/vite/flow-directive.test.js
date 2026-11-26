"use flow";
//
// `"use flow"` and `"use js"`, as a build bundles them.
//
// They are the module's source mode — what the type checker reads, the way
// `// @flow` was — and nothing at runtime does. Rolldown does not know them,
// so a module that still opened with one after `uf transform` cost a
// `MODULE_LEVEL_DIRECTIVE` warning per module and left the string at the top
// of the chunk — an entry's prologue is the chunk's, so a CommonJS bundle
// opened with `"use flow";`. The transform drops them; every other directive
// stays, in the order it was written. This is a real Vite build through the
// real `uf transform` — `uf test` names itself in `UF_BINARY` — because both
// the warning and the chunk are the bundler's, and only a bundle produces them.
//
// Rolldown has warned about a directive it does not know since 1.2.18. Under
// an older one the warning assertion has nothing to see, and the chunk's
// prologue is what this test still holds the build to.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";
import { build } from "vite";

import { flowTransform } from "./index.js";

/** An entry, and a module in each of the shapes a prologue can take. */
const FILES = {
  "main.js": `"use client";
"use flow";
import { note } from "./note.js";
import { save } from "./save.js";
import { plain } from "./plain.js";

export const words: string = "use flow";
console.log(note, save, plain, words);
`,
  "note.js": `"use client";
"use flow";
export const note: string = "note";
`,
  "save.js": `'use flow';
'use client';
export const save = (id: string): string => id;
`,
  "plain.js": `"use js";
export const plain = 1;
`,
};

async function inAProject(body: (root: string) => Promise<void>): Promise<void> {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-flow-directive-")));
  for (const [file, source] of Object.entries(FILES)) {
    fs.writeFileSync(path.join(root, file), String(source));
  }
  try {
    await body(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

/** Every chunk the build wrote, in the order of its formats. */
function chunks(output: $FlowFixMe): Array<string> {
  const outputs = Array.isArray(output) ? output.flatMap((each) => each.output) : output.output;
  return outputs.filter((file) => file.type === "chunk").map((file) => file.code);
}

/** The directives a chunk opens with, as it spells them. */
function prologue(code: string): Array<string> {
  const lines = [];
  for (const line of code.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('"') && !trimmed.startsWith("'")) break;
    lines.push(trimmed);
  }
  return lines;
}

/** The directive a `MODULE_LEVEL_DIRECTIVE` warning is about. */
function directiveOf(log: $FlowFixMe): string {
  return /["'](use [^"']+)["']/.exec(String(log.message))?.[1] ?? "";
}

describe("a Flow module's source-mode directive", () => {
  it("is gone from the bundle, without a warning, and use client is still first", async () => {
    await inAProject(async (root) => {
      const directives: Array<string> = [];
      const output = await build({
        root,
        configFile: false,
        logLevel  : "silent",
        plugins   : [flowTransform({ root })],
        build: {
          write : false,
          minify: false,
          rolldownOptions: {
            input : path.join(root, "main.js"),
            output: [{ format: "es" }, { format: "cjs" }],
            onLog(level, log, handler) {
              if (log.code === "MODULE_LEVEL_DIRECTIVE") directives.push(directiveOf(log));
              handler(level, log);
            },
          },
        },
      });

      expect(directives).not.toContain("use flow");
      expect(directives).not.toContain("use js");

      const written = chunks(output);
      expect(written.length).toBe(2);
      for (const code of written) {
        expect(prologue(code)).toEqual(['"use client";']);
        expect(code).not.toMatch(/^\s*["']use (?:flow|js)["']/m);
        // A string that only spells the words is a string, and it is kept:
        // the bundler inlines the constant into the call that logs it.
        expect(code).toContain(', "use flow");');
      }
    });
  });
});
