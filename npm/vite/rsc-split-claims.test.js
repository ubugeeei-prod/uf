// @flow
//
// The route split, held to the sentences of
// `docs/app/guide/server-components/$page.mdx` that `rsc-split.test.js` did
// not reach (ubugeeei-prod/uf#1501):
//
//   "`uf build` drops a route's page from the *client* route table when no
//   client boundary is reachable from that route's page, its layouts, its
//   `$loading.js` and `$template.js` modules, or the not-found and error
//   boundaries that cover it."
//
// and
//
//   "a manifest that is missing, unreadable, or written by an older uf removes
//   nothing at all."
//
// That file proves the slot versions of the first sentence and the missing and
// older halves of the second. These are the route's own boundaries, and the
// manifest that is there but cannot be read.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, describe, expect, it } from "@uniflowed/test";

import { clientRouteFilter, readRscManifest } from "./internal/rsc.js";
import { scanRoutes } from "./internal/routes.js";

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

const MODULE = "// @flow\nexport default function Module() {}\n";

/** A project with a root page and `/docs`, plus `extra` under `app/docs/`. */
function project(extra: $ReadOnlyArray<string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-rsc-split-claims-"));
  roots.push(root);
  for (const relative of ["app/$layout.js", "app/$page.js", "app/docs/$page.js", ...extra]) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, MODULE);
  }
  return root;
}

/** The manifest's entry for one module; `reaches` is whether a client boundary is below it. */
function entry(modulePath: string, reaches: boolean) {
  return {
    path: modulePath,
    environment: "server",
    reachability: "server-only",
    proximity: reaches ? "reaches-boundary" : "isolated",
    imports: [],
    externalImports: [],
    exports: ["default"],
  };
}

/** Write the manifest for `root`, where only `reaching` reaches a boundary, and read it back. */
function manifest(root: string, files: $ReadOnlyArray<string>, reaching: string): mixed {
  const file = path.join(root, ".uf", "rsc", "uf-rsc-manifest.json");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(
    file,
    JSON.stringify({
      version: 3,
      engine: "uf-native",
      buildFingerprint: "0".repeat(64),
      modules: files.map((module) => entry(module, module === reaching)),
      clientBoundaries: [],
      clientBundleRoots: [],
      serverActions: [],
      diagnostics: [],
    }),
  );
  return readRscManifest(file);
}

/** Which routes ship their page, as `[path, ships]`. */
function shipped(root: string, read: mixed): Array<[string, boolean]> {
  const table = scanRoutes(path.join(root, "app"));
  const shipsPage = clientRouteFilter(read, root, table);
  return table.routes.map((route) => [route.path, shipsPage(route)]);
}

describe("a route's own boundaries keep its page in the client bundle", () => {
  for (const boundary of ["$loading.js", "$template.js", "$error.js", "$not-found.js"]) {
    it(`when its only client boundary is below its ${boundary}`, () => {
      const extra = `app/docs/${boundary}`;
      const root = project([extra]);
      const files = ["app/$layout.js", "app/$page.js", "app/docs/$page.js", extra];

      // `/docs` is the route the boundary covers, and the one that has to hydrate
      // it; `/` is covered by nothing that reaches the browser, and is dropped.
      expect(shipped(root, manifest(root, files, extra))).toEqual([
        ["/", false],
        ["/docs", true],
      ]);
    });
  }

  it("and drops the route when the same boundary reaches nothing", () => {
    const root = project(["app/docs/$loading.js"]);
    const files = ["app/$layout.js", "app/$page.js", "app/docs/$page.js", "app/docs/$loading.js"];
    expect(shipped(root, manifest(root, files, "none"))).toEqual([
      ["/", false],
      ["/docs", false],
    ]);
  });
});

describe("a manifest that cannot be read removes nothing", () => {
  for (const [what, contents] of [
    ["is not JSON", "{ this is not json"],
    ["is JSON of the wrong shape", '{"version": 3, "modules": "everything"}'],
    ["is empty", ""],
  ]) {
    it(`when the file ${what}`, () => {
      const root = project([]);
      const file = path.join(root, ".uf", "rsc", "uf-rsc-manifest.json");
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, contents);

      expect(shipped(root, readRscManifest(file))).toEqual([
        ["/", true],
        ["/docs", true],
      ]);
    });
  }

  it("when the file named is not there", () => {
    const root = project([]);
    const missing = path.join(root, ".uf", "rsc", "uf-rsc-manifest.json");
    expect(shipped(root, readRscManifest(missing))).toEqual([
      ["/", true],
      ["/docs", true],
    ]);
  });
});
