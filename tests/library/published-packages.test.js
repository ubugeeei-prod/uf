// @flow
//
// What a package publishes has to cover what it says it exports.
//
// `@uniflowed/host` named `./write-atomically` in `exports` and left
// `write-atomically.js` out of `files`, so npm packed the package without it.
// `internal/node-hooks.js` imports that file at module scope to register the
// loader hooks, so in an installed project every test worker exited 1 before
// running anything:
//
//     Error [ERR_MODULE_NOT_FOUND]: Cannot find module
//       '…/node_modules/@uniflowed/host/write-atomically.js'
//       imported from '…/node_modules/@uniflowed/host/internal/node-hooks.js'
//
// `uf create app react` writes a test file, so create → install → test — the
// first thing anyone does — failed on the first try. See ubugeeei-prod/uf#409.
//
// This repository's own suite never saw it: `@uniflowed/host` resolves through
// the workspace here, where the file is present. Only the tarball is missing
// it, and nothing looked at the tarball.
//
// So this test looks at the tarball. `npm pack --dry-run --json` reports the
// file list npm would publish, from npm's own packing rules rather than a
// reimplementation of them, and it neither writes anything nor talks to the
// registry.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";

/** This checkout, found by a file that only it has. */
const repository: string = (() => {
  const wanted = path.join("tools", "release", "published-packages.txt");
  let directory = process.env.UF_PROJECT_ROOT ?? process.cwd();
  for (let up = 0; up < 8; up += 1) {
    if (fs.existsSync(path.join(directory, wanted))) return directory;
    directory = path.dirname(directory);
  }
  throw new Error(`could not find ${wanted} above ${process.cwd()}`);
})();

/** The packages that go to npm, in the order the release publishes them. */
const published: Array<string> = fs
  .readFileSync(path.join(repository, "tools/release/published-packages.txt"), "utf8")
  .split("\n")
  .map((line) => line.trim())
  .filter((line) => line !== "" && !line.startsWith("#"));

/**
 * The paths npm would publish for the package in `directory`, as npm sees them.
 *
 * `--dry-run` is what keeps this offline and side-effect free: npm reports the
 * list and writes no tarball.
 */
const packedPaths = (directory: string): Set<string> => {
  const stdout = execFileSync("npm", ["pack", "--dry-run", "--json"], {
    cwd: directory,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  const [report] = JSON.parse(stdout);
  return new Set(report.files.map((file) => file.path));
};

/** Every string an `exports` map can end at, however deeply it nests. */
const exportTargets = (node: mixed, into: Array<string>): Array<string> => {
  if (typeof node === "string") into.push(node);
  else if (node !== null && typeof node === "object") {
    for (const value of Object.values(node)) exportTargets(value, into);
  }
  return into;
};

/**
 * The relative specifiers `source` imports.
 *
 * A regular expression, not a parser, because the packages are ordinary ESM
 * and the question is only which relative paths appear in an import position.
 * `the scan finds the imports it is looking for` below holds it to that: a
 * pattern that quietly stopped matching would make every assertion here pass
 * over nothing, which is the one way this test could rot without failing.
 */
const relativeImports = (source: string): Array<string> => {
  const pattern = /(?:\bfrom|\bimport|\brequire)\s*\(?\s*["'](\.[^"']*)["']/g;
  const found = [];
  let match;
  while ((match = pattern.exec(source)) !== null) found.push(match[1]);
  return found;
};

/**
 * Where a relative specifier lands, as Node resolves it: an exact path, and
 * failing that the two extensions a directory or extensionless import means.
 */
const resolvesTo = (packed: Set<string>, from: string, specifier: string): string | null => {
  const base = path.posix.normalize(path.posix.join(path.posix.dirname(from), specifier));
  for (const candidate of [base, `${base}.js`, `${base}/index.js`]) {
    if (packed.has(candidate)) return candidate;
  }
  return null;
};

describe("what the published packages pack", () => {
  it("the scan finds the imports it is looking for", () => {
    // A guard on the guard. `internal/node-hooks.js` is the file whose import
    // #409 was about, so if the scanner ever stops seeing that one, both
    // assertions below are passing over an empty list.
    const hooks = fs.readFileSync(
      path.join(repository, "packages/host/internal/node-hooks.js"),
      "utf8",
    );
    expect(relativeImports(hooks)).toContain("../write-atomically.js");
    expect(
      relativeImports(`import x from "./a.js";\nconst y = await import('../b/c.js');`),
    ).toEqual(["./a.js", "../b/c.js"]);
  });

  // One `npm pack` per package, read by both assertions. The packages are
  // walked here rather than in a `describe` per package because a `describe`
  // built in a loop is not a declaration `uf test` can expand, and a suite
  // that reports itself as unexpandable is a worse trade than a failure
  // message that names the package itself.
  const packed: Map<string, Set<string>> = new Map(
    published.map((name) => [name, packedPaths(path.join(repository, "packages", name))]),
  );

  it("every package publishes every file it exports", () => {
    const missing = [];
    for (const name of published) {
      const directory = path.join(repository, "packages", name);
      const manifest = JSON.parse(fs.readFileSync(path.join(directory, "package.json"), "utf8"));
      const files = packed.get(name) ?? new Set();
      for (const target of exportTargets(manifest.exports ?? {}, [])) {
        if (!target.startsWith("./")) continue;
        if (!files.has(target.slice(2))) missing.push(`${manifest.name} exports ${target}`);
      }
    }
    expect(missing).toEqual([]);
  });

  it("every package publishes every file its published files import", () => {
    const missing = [];
    for (const name of published) {
      const directory = path.join(repository, "packages", name);
      const files = packed.get(name) ?? new Set();
      for (const file of files) {
        if (!file.endsWith(".js")) continue;
        const source = fs.readFileSync(path.join(directory, file), "utf8");
        for (const specifier of relativeImports(source)) {
          if (resolvesTo(files, file, specifier) === null) {
            missing.push(`@uniflowed/${name}: ${file} imports ${specifier}`);
          }
        }
      }
    }
    expect(missing).toEqual([]);
  });
});
