// @flow
//
// A Web Worker written in Flow, as a build bundles it.
//
// Vite bundles a worker with `worker.plugins` and never with `plugins`, so the
// driver's `uf:flow` never saw one and a worker written in Flow reached the
// bundler as Flow: "Flow is not supported" (ubugeeei-prod/uf#1676). This is a
// real Vite build through the real `uf transform` — `uf test` names itself in
// `UF_BINARY` — because what failed was the bundler's parse, and a hook called
// by hand never parses.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";
import { build } from "vite";

import { flowTransform } from "./index.js";

/** A page that starts a worker, and the worker, written in Flow. */
const FILES = {
  "main.js": `new Worker(new URL("./worker.js", import.meta.url), { type: "module" });\n`,
  "worker.js": `// @flow
type Doubling = { data: number };

self.onmessage = (event: Doubling) => {
  self.postMessage(event.data * 2);
};
`,
};

async function inAProject(body: (root: string) => Promise<void>): Promise<void> {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-worker-flow-")));
  for (const [file, source] of Object.entries(FILES)) {
    fs.writeFileSync(path.join(root, file), String(source));
  }
  try {
    await body(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

/** Every chunk and asset the build wrote, as text. */
function written(output: $FlowFixMe): string {
  const outputs = Array.isArray(output) ? output.flatMap((each) => each.output) : output.output;
  return outputs
    .map((file) => (file.type === "chunk" ? file.code : String(file.source)))
    .join("\n");
}

describe("a worker written in Flow", () => {
  it("is compiled by the transform the driver hands `worker.plugins`", async () => {
    await inAProject(async (root) => {
      const output = await build({
        root,
        configFile: false,
        logLevel  : "silent",
        worker    : { format: "es", plugins: () => [flowTransform({ root })] },
        build: {
          write        : false,
          minify       : false,
          rollupOptions: { input: path.join(root, "main.js") },
        },
      });

      const code = written(output);
      expect(code).toContain("event.data * 2");
      expect(code).not.toContain("Doubling");
    });
  });
});
