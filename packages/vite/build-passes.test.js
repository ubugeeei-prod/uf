// @flow
//
// What `uf build`'s passes cost the transform, as the `uf:flow` plugin sees
// them.
//
// `uf build` runs the rsc graph, the client and the server as three Vite
// passes with the same plugin objects. Two things made each pass pay for the
// one before it, and each case below is one of them:
//
// * the plugin closed its `uf transform` at every `buildEnd`, so the next pass
//   started a new one with an empty memory and compiled `@uniflowed/router`
//   and every shared module again;
// * the server pass imports the rsc graph's bundle from `.uf/build/rsc/`, a
//   `.js` file under the project root, and the plugin compiled it as though it
//   were source — 210 kB of already-compiled JavaScript, every build.
//
// The compiler here is a stand-in that speaks `uf transform`'s protocol and
// writes one line to a log each time it is started, which is all either
// question needs. What a real build ships is `crates/uf_cli/tests/vite.rs`.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";

import uniflowed from "./index.js";

const NODE = String(
  spawnSync("node", ["-p", "process.execPath"], { encoding: "utf8" }).stdout,
).trim();

/** A `uf transform` that notes its own start and answers every module. */
function standIn(root: string): { readonly command: string, readonly starts: () => number } {
  const log = path.join(root, "starts.log");
  const command = path.join(root, "uf");
  fs.writeFileSync(
    command,
    `#!${NODE}
require("node:fs").appendFileSync(${JSON.stringify(log)}, "started\\n");
let rest = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  rest += chunk;
  let at = rest.indexOf("\\n");
  while (at !== -1) {
    const request = JSON.parse(rest.slice(0, at));
    rest = rest.slice(at + 1);
    process.stdout.write(JSON.stringify({ id: request.id, code: request.code }) + "\\n");
    at = rest.indexOf("\\n");
  }
});
`,
  );
  fs.chmodSync(command, 0o755);
  return {
    command,
    starts: () => {
      try {
        return fs.readFileSync(log, "utf8").split("\n").filter(Boolean).length;
      } catch {
        return 0;
      }
    },
  };
}

function inAProject(body: (root: string) => Promise<void>): Promise<void> {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-build-passes-")));
  return body(root).finally(() => fs.rmSync(root, { recursive: true, force: true }));
}

/**
 * The `uf:flow` plugin of one `uniflowed()` call. Untyped, as `./index.js` is:
 * it is plain JavaScript Vite loads before any transform exists.
 */
function flowPlugin(
  root: string,
  command: string,
  shareTransformAcrossBuilds: boolean = true,
): $FlowFixMe {
  return uniflowed({ root, config: {}, command, shareTransformAcrossBuilds }).find(
    (plugin) => plugin.name === "uf:flow",
  );
}

/** Three passes over one module, the way `uf build` runs them. */
async function threePasses(flow: $FlowFixMe, module: string): Promise<void> {
  for (const environment of ["rsc", "client", "ssr"]) {
    const context = pass(environment);
    flow.buildStart.call(context);
    const out = await flow.transform.call(context, "export const page = 1;\n", module);
    expect(out?.code).toContain("export const page = 1;");
    flow.buildEnd.call(context);
  }
}

/** What Vite hands a hook as `this` in a build's pass for `environment`. */
function pass(environment: string): $FlowFixMe {
  return { environment: { name: environment }, warn() {}, info() {} };
}

describe("uf build's passes", () => {
  it("share one uf transform, rather than starting one per pass", async () => {
    await inAProject(async (root) => {
      const uf = standIn(root);
      await threePasses(flowPlugin(root, uf.command), path.join(root, "app", "page.js"));

      // One process for the three passes: the second and third ask the one
      // that has already answered for the same module.
      expect(uf.starts()).toBe(1);
    });
  });

  it("close the service after each build for a caller that did not ask to share it", async () => {
    await inAProject(async (root) => {
      const uf = standIn(root);
      await threePasses(flowPlugin(root, uf.command, false), path.join(root, "app", "page.js"));

      // A process that builds many projects in turn keeps no idle `uf` behind
      // for each one.
      expect(uf.starts()).toBe(3);
    });
  });

  it("leaves what an earlier pass wrote under .uf/build to the bundler", async () => {
    await inAProject(async (root) => {
      const uf = standIn(root);
      const flow = flowPlugin(root, uf.command);
      const context = pass("ssr");
      const bundle = path.join(root, ".uf", "build", "rsc", "index.js");

      expect(await flow.transform.call(context, "export const rendered = 1;\n", bundle)).toBe(null);
      // Not declined by the compiler after a round trip: never asked at all.
      expect(uf.starts()).toBe(0);
    });
  });
});
