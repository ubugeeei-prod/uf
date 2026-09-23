// @flow
//
// The API reference's two halves: `tools/docs/api.js`, which decides what a
// package exports and under which specifier, and the doc-comment reader in
// `docs/app/_design/doc-comment.js`. The site build runs the first over every package;
// these pin the rules it follows on inputs small enough to read.

import { describe, expect, it } from "@uniflowed/test";

import { blocksOf } from "../../docs/app/_design/doc-comment.js";
import {
  assemble,
  declaredExports,
  everythingFrom,
  exportTarget,
  publicModules,
  reexports,
} from "../../tools/docs/api.js";

describe("publicModules", () => {
  it("names each subpath by the specifier a program writes, and skips what is not a module", () => {
    expect(
      publicModules("@uniflowed/query", {
        ".": "./index.js",
        "./cache": "./cache.js",
        "./package.json": "./package.json",
        "./styles/*": "./styles/*.js",
        "./metro": { require: "./metro.cjs", default: "./metro.js" },
      }),
    ).toEqual([
      { specifier: "@uniflowed/query", file: "index.js" },
      { specifier: "@uniflowed/query/cache", file: "cache.js" },
      { specifier: "@uniflowed/query/metro", file: "metro.js" },
    ]);
  });

  it("reads a string or a bare condition object as the package root", () => {
    expect(publicModules("@s/a", "./main.js")).toEqual([{ specifier: "@s/a", file: "main.js" }]);
    expect(publicModules("@s/a", { import: "./esm.js", require: "./cjs.cjs" })).toEqual([
      { specifier: "@s/a", file: "esm.js" },
    ]);
    expect(exportTarget(null)).toBe(null);
  });
});

describe("reading a module's exports", () => {
  const source = [
    "// export function inAComment() {}",
    'export type { A, B as C } from "./internal/types.js";',
    'export { run } from "./internal/run.js";',
    'export { describe, it } from "@uniflowed/test";',
    'export * from "react";',
    "export function make(): void {}",
    "export component Button() { return null; }",
    "export opaque type Id = string;",
    "const local = 1;",
    "export { local as renamed };",
  ].join("\n");

  it("finds what the module declares, with its kind and line, and not what a comment says", () => {
    expect(declaredExports(source)).toEqual([
      { name: "make", kind: "function", line: 6 },
      { name: "Button", kind: "component", line: 7 },
      { name: "Id", kind: "opaque type", line: 8 },
      { name: "renamed", kind: "binding", line: 10 },
    ]);
  });

  it("maps re-exports to the file or package they come from, with their public names", () => {
    const found = reexports(source, "index.js");
    expect([...(found.get("internal/types.js") ?? new Map())]).toEqual([
      ["A", "A"],
      ["B", "C"],
    ]);
    expect([...(found.get("internal/run.js") ?? new Map())]).toEqual([["run", "run"]]);
    expect([...(found.get("@uniflowed/test") ?? new Map())]).toEqual([
      ["describe", "describe"],
      ["it", "it"],
    ]);
  });

  it("names a package re-exported whole", () => {
    expect(everythingFrom(source)).toEqual(["react"]);
  });
});

describe("assemble", () => {
  const files: { [string]: string } = {
    "index.js": [
      'export { createFetch } from "./internal/client.js";',
      'export { hidden as shown } from "./internal/client.js";',
      "export function helper() {}",
    ].join("\n"),
    "internal/client.js":
      "export function createFetch() {}\nexport function hidden() {}\nexport function never() {}",
  };
  const api = assemble(
    {
      name: "@uniflowed/fetch",
      version: "1.0.0",
      description: "Typed HTTP.",
      exports: { ".": "./index.js" },
      dir: "fetch",
    },
    [
      {
        path: "internal/client.js",
        entries: [
          {
            name: "createFetch",
            kind: "function",
            signature: "export function createFetch()",
            description: "A client.",
          },
          {
            name: "never",
            kind: "function",
            signature: "export function never()",
            description: "Not public.",
          },
        ],
      },
    ],
    (file) => files[file] ?? null,
  );

  it("keeps what an internal module declares only where an export target re-exports it", () => {
    expect(api.modules.map((module) => module.specifier)).toEqual(["@uniflowed/fetch"]);
    expect(api.modules[0].entries.map((entry) => entry.name)).toEqual(["createFetch"]);
  });

  it("lists exports without a doc comment, under their public names, with where they are", () => {
    expect(api.modules[0].bare).toEqual([
      { name: "helper", kind: "function", file: "index.js", line: 3 },
      { name: "shown", kind: "function", file: "internal/client.js", line: 2 },
    ]);
  });

  it("takes its slug from the name and keeps the directory it came from", () => {
    expect([api.slug, api.dir, api.version]).toEqual(["fetch", "fetch", "1.0.0"]);
  });
});

describe("blocksOf", () => {
  it("reads paragraphs, fenced code and lists out of a doc comment", () => {
    expect(
      blocksOf(
        [
          "One paragraph",
          "over two lines.",
          "",
          "```js",
          "const a = 1;",
          "```",
          "",
          "- first",
          "  continued",
          "- second",
        ].join("\n"),
      ),
    ).toEqual([
      { kind: "paragraph", text: "One paragraph over two lines." },
      { kind: "code", text: "const a = 1;" },
      { kind: "list", items: ["first continued", "second"] },
    ]);
  });
});
