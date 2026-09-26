// @flow

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { describe, expect, it } from "@uniflowed/test";

const here = path.dirname(fileURLToPath(import.meta.url));

function readJson(file: string): { [string]: mixed } {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function nodeImports(source: string): Array<string> {
  const found = new Set<string>();
  const pattern = /\bfrom\s+["'](node:[^"']+)["']/g;
  let match = pattern.exec(source);
  while (match != null) {
    found.add(match[1]);
    match = pattern.exec(source);
  }
  return [...found].sort();
}

function nodeNamedImports(source: string): Map<string, Array<string>> {
  const found = new Map<string, Array<string>>();
  const pattern = /\bimport\s+\{([^}]+)\}\s+from\s+["'](node:[^"']+)["']/g;
  let match = pattern.exec(source);
  while (match != null) {
    const names = match[1]
      .split(",")
      .map((entry) =>
        entry
          .trim()
          .split(/\s+as\s+/u)[0]
          .trim(),
      )
      .filter((entry) => entry !== "");
    const specifier = match[2];
    found.set(specifier, [...(found.get(specifier) ?? []), ...names].sort());
    match = pattern.exec(source);
  }
  return found;
}

/**
 * The modules of this package a browser bundle reaches, and so every file whose
 * `node:` imports the `browser` field has to answer.
 *
 * `module-mocks.js` is reached from `@uniflowed/test`, and it imports
 * `transform.js`. A bundler resolves both before it shakes either out, and a
 * named import the substitute does not export is a build error there rather
 * than a warning: the edge build of a project with an in-source test failed on
 * exactly that when `transform.js` began importing `spawnSync`.
 */
const REACHED_IN_A_BROWSER = ["module-mocks.js", "transform.js"];

describe("browser substitutions", () => {
  it("cover every Node builtin a browser bundle reaches in this package", async () => {
    const manifest = readJson(path.join(here, "package.json"));
    const browser = manifest.browser;
    if (browser == null || typeof browser !== "object" || Array.isArray(browser)) {
      throw new Error("package.json#browser must be an object");
    }

    for (const reached of REACHED_IN_A_BROWSER) {
      const source = fs.readFileSync(path.join(here, reached), "utf8");
      const named = nodeNamedImports(source);
      for (const specifier of nodeImports(source)) {
        const target = browser[specifier];
        if (typeof target !== "string") {
          throw new Error(`package.json#browser has no substitute file for ${specifier}`);
        }
        const file = path.join(here, target);
        expect(fs.existsSync(file)).toBe(true);
        // The substitute named by package.json, known only once it is read;
        // Flow types only a literal specifier, and the names are checked below.
        // $FlowFixMe[unsupported-syntax]
        const exports: { readonly [string]: mixed } = await import(pathToFileURL(file).href);
        const missing = (named.get(specifier) ?? []).filter((name) => !(name in exports));
        // As a sentence, so a failure names the import of the module the
        // substitute lacks rather than reporting that `false` was not `true`.
        expect(`${reached} imports from ${specifier}: ${missing.join(", ")}`).toBe(
          `${reached} imports from ${specifier}: `,
        );
      }
    }
  });

  it("keeps the node:url browser shim compatible with imported helper names", async () => {
    const shim = await import("./internal/browser-module.js");
    const url = shim.pathToFileURL("/tmp/uf module#one.js");
    expect(url.href).toBe("file:///tmp/uf%20module%23one.js");
    expect(shim.fileURLToPath(url)).toBe("/tmp/uf module#one.js");
    expect(shim.fileURLToPath("file:///tmp/uf%20module%23two.js")).toBe("/tmp/uf module#two.js");
  });
});
