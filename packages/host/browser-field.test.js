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
  const found = new Map();
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

describe("browser substitutions", () => {
  it("cover every Node builtin module-mocks imports", async () => {
    const manifest = readJson(path.join(here, "package.json"));
    const browser = manifest.browser;
    if (browser == null || typeof browser !== "object" || Array.isArray(browser)) {
      throw new Error("package.json#browser must be an object");
    }

    const source = fs.readFileSync(path.join(here, "module-mocks.js"), "utf8");
    const named = nodeNamedImports(source);
    for (const specifier of nodeImports(source)) {
      const target = (browser: $FlowFixMe)[specifier];
      expect(typeof target).toBe("string");
      const file = path.join(here, target);
      expect(fs.existsSync(file)).toBe(true);
      const exports = await import(pathToFileURL(file).href);
      for (const name of named.get(specifier) ?? []) {
        expect(name in exports).toBe(true);
      }
    }
  });
});
