// @flow

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
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

describe("browser substitutions", () => {
  it("cover every Node builtin module-mocks imports", () => {
    const manifest = readJson(path.join(here, "package.json"));
    const browser = manifest.browser;
    if (browser == null || typeof browser !== "object" || Array.isArray(browser)) {
      throw new Error("package.json#browser must be an object");
    }

    const source = fs.readFileSync(path.join(here, "module-mocks.js"), "utf8");
    for (const specifier of nodeImports(source)) {
      const target = (browser: $FlowFixMe)[specifier];
      expect(typeof target).toBe("string");
      expect(fs.existsSync(path.join(here, target))).toBe(true);
    }
  });
});
