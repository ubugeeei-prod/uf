// @flow
//
// The suite the testing guide's "Where it stands" table is measured on,
// written out in three idiomatic copies.
//
//   node tools/bench/testing/suite.js /tmp/uf-bench-testing
//
// # Why this exists
//
// ubugeeei-prod/uf#944. The table compared `uf test`, `bun test` and
// `vitest run` on "50 files, 1,000 tests, 2,000 assertions", and nothing in
// the repository said what those files were — so the numbers could not be
// reproduced, only re-quoted. This writes them.
//
// Three copies, because each runner is measured on the input a project would
// actually give it, and a copy is the only honest way to do that: uf's is Flow
// and imports `@uniflowed/test`, Bun's is plain JavaScript and imports
// `bun:test`, Vitest's is plain JavaScript and imports `vitest`. Only the
// import line and the type annotations differ; every assertion is the same
// assertion, so the three runs do the same work.
//
//   <outDir>/uf       uf test
//   <outDir>/bun      bun test
//   <outDir>/vitest   vitest run
//
// # What is deliberately true of the files
//
// *Deterministic*: the same bytes every run, so a number that moves is uf
// moving and not the fixture.
//
// *No two alike*: each module and each case is keyed off its file number, so
// nothing here flatters a content-addressed transform cache by handing it the
// same module fifty times.
//
// *Exactly two assertions a case*: 50 x 20 x 2, which is what "1,000 tests,
// 2,000 assertions" means. `bun test` reports the `expect()` count, so the
// claim is checkable rather than asserted.
//
// # What it does not do
//
// It does not run anything, and it installs nothing. Vitest is not a
// dependency of this repository and Bun is not a dependency of anything, so
// the three commands are the reader's to run, against whatever versions they
// have — which the guide asks them to state beside the numbers.

import fs from "node:fs";
import path from "node:path";

import { refuseInsideRepository } from "../toolchain/fixture.js";

export type Dialect = "uf" | "bun" | "vitest";

export type SuiteFile = { readonly path: string, readonly contents: string };

export type Preset = { readonly files: number, readonly cases: number };

/** The shape the guide's table names. */
export const GUIDE_PRESET: Preset = { files: 50, cases: 20 };

type Copy = { readonly name: Dialect, readonly importFrom: string, readonly flow: boolean };

/** In the order they are written. */
export const COPIES: $ReadOnlyArray<Copy> = [
  { name: "uf", importFrom: "@uniflowed/test", flow: true },
  { name: "bun", importFrom: "bun:test", flow: false },
  { name: "vitest", importFrom: "vitest", flow: false },
];

function pad(n: number): string {
  return String(n).padStart(3, "0");
}

/** The two grade boundaries of module `n`, shared with its test file. */
function bounds(n: number): { readonly low: number, readonly high: number } {
  return { low: 30 + (n % 7) * 3, high: 60 + (n % 5) * 4 };
}

function clamp(score: number): number {
  if (score < 0) {
    return 0;
  }
  return score > 100 ? 100 : score;
}

function labelOf(score: number, n: number): string {
  const { low, high } = bounds(n);
  const graded = clamp(score);
  if (graded < low) {
    return "low";
  }
  return graded < high ? "mid" : "high";
}

/**
 * The module under test.
 *
 * Five small functions with no dependencies: the point of the suite is the
 * runner's overhead per file and per case, so the work inside a case has to be
 * small enough not to be what is measured.
 */
function sourceModule(n: number, flow: boolean): string {
  const id = pad(n);
  const { low, high } = bounds(n);
  const t = (annotation: string): string => (flow ? annotation : "");
  return `${flow ? "// @flow\n" : ""}// Module ${id} of the testing benchmark suite.

const LOW = ${String(low)};
const HIGH = ${String(high)};

export function summarize${id}(values${t(": Array<number>")})${t(
    ": { count: number, total: number, min: number, max: number }",
  )} {
  let total = 0;
  let min = Infinity;
  let max = -Infinity;
  for (const value of values) {
    total += value;
    if (value < min) {
      min = value;
    }
    if (value > max) {
      max = value;
    }
  }
  const empty = values.length === 0;
  return { count: values.length, total, min: empty ? 0 : min, max: empty ? 0 : max };
}

export function grade${id}(score${t(": number")})${t(": number")} {
  if (score < 0) {
    return 0;
  }
  return score > 100 ? 100 : score;
}

export function label${id}(score${t(": number")})${t(": string")} {
  const graded = grade${id}(score);
  if (graded < LOW) {
    return "low";
  }
  return graded < HIGH ? "mid" : "high";
}

export function slugify${id}(title${t(": string")})${t(": string")} {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+/, "")
    .replace(/-+$/, "");
}

export async function retry${id}(
  attempt${t(": () => Promise<number>")},
  times${t(": number")},
)${t(": Promise<number>")} {
  let last${t(": mixed")} = null;
  for (let at = 0; at < times; at += 1) {
    try {
      return await attempt();
    } catch (error) {
      last = error;
    }
  }
  throw last instanceof Error ? last : new Error("every attempt failed");
}
`;
}

/**
 * One case. Five shapes, each with exactly two assertions.
 *
 * One of the five is asynchronous, because a runner that only ever ran
 * synchronous bodies would not be paying for the part of the protocol that
 * waits.
 */
function testCase(n: number, k: number): string {
  const id = pad(n);
  const v = ((n * 31 + k * 17) % 97) + 1;
  switch (k % 5) {
    case 0:
      return `  it("adds up a sample (case ${String(k)})", () => {
    const stats = summarize${id}([${String(v)}, ${String(v + 1)}, ${String(v + 2)}]);
    expect(stats.total).toBe(${String(3 * v + 3)});
    expect(stats.count).toBe(3);
  });`;
    case 1:
      return `  it("finds the extremes of a sample (case ${String(k)})", () => {
    const stats = summarize${id}([${String(v + 5)}, ${String(v)}, ${String(v + 9)}]);
    expect(stats.min).toBe(${String(v)});
    expect(stats.max).toBe(${String(v + 9)});
  });`;
    case 2:
      return `  it("grades and labels a score of ${String(v)} (case ${String(k)})", () => {
    expect(grade${id}(${String(v)})).toBe(${String(clamp(v))});
    expect(label${id}(${String(v)})).toBe("${labelOf(v, n)}");
  });`;
    case 3:
      return `  it("slugifies a title (case ${String(k)})", () => {
    expect(slugify${id}("  Hello, World ${String(v)}! ")).toBe("hello-world-${String(v)}");
    expect(slugify${id}("A  B${String(v)}")).toBe("a-b${String(v)}");
  });`;
    default:
      return `  it("retries until an attempt succeeds (case ${String(k)})", async () => {
    let calls = 0;
    const value = await retry${id}(async () => {
      calls += 1;
      if (calls < 2) {
        throw new Error("not yet");
      }
      return ${String(v)};
    }, 3);
    expect(value).toBe(${String(v)});
    expect(calls).toBe(2);
  });`;
  }
}

function testFile(n: number, cases: number, copy: Copy): string {
  const id = pad(n);
  const bodies = [];
  for (let k = 0; k < cases; k += 1) {
    bodies.push(testCase(n, k));
  }
  return `${copy.flow ? "// @flow\n" : ""}import { describe, expect, it } from "${copy.importFrom}";

import {
  grade${id},
  label${id},
  retry${id},
  slugify${id},
  summarize${id},
} from "./m${id}.js";

describe("m${id}", () => {
${bodies.join("\n\n")}
});
`;
}

/** Every file of one copy, in the order they are written. Pure. */
export function suiteFiles(preset: Preset, name: Dialect): Array<SuiteFile> {
  const copy = COPIES.find((one) => one.name === name);
  if (copy == null) {
    throw new Error(`there is no copy called "${name}"`);
  }
  const files: Array<SuiteFile> = [
    {
      path: "package.json",
      contents: `${JSON.stringify(
        { name: `uf-bench-testing-${name}`, private: true, type: "module", version: "0.0.0" },
        null,
        2,
      )}\n`,
    },
  ];
  if (name === "uf") {
    files.push({
      path: "uf.config.js",
      contents: `// @flow\nimport { defineConfig } from "@uniflowed/config";\n\nexport default defineConfig({});\n`,
    });
  }
  if (name === "vitest") {
    files.push({
      path: "vitest.config.js",
      contents: `export default { test: { include: ["lib/**/*.test.js"] } };\n`,
    });
  }
  for (let n = 1; n <= preset.files; n += 1) {
    files.push({ path: `lib/m${pad(n)}.js`, contents: sourceModule(n, copy.flow) });
    files.push({ path: `lib/m${pad(n)}.test.js`, contents: testFile(n, preset.cases, copy) });
  }
  return files;
}

/**
 * The `node_modules` this checkout's third-party packages are installed in.
 *
 * Usually `<repoRoot>/node_modules`. In a git worktree it is often the parent
 * checkout's: a worktree links `@uniflowed/*` locally and finds everything
 * else by Node walking up the tree. A suite generated under the system
 * temporary directory is not inside that tree and so cannot walk up to it, so
 * the walk happens here and the suite gets real links either way.
 */
function installRoot(repoRoot: string): string {
  let at = path.resolve(repoRoot);
  for (;;) {
    const installed = path.join(at, "node_modules");
    if (fs.existsSync(path.join(installed, "react", "package.json"))) {
      return installed;
    }
    const parent = path.dirname(at);
    if (parent === at) {
      throw new Error(
        `found no node_modules with react in or above ${repoRoot}. The suite runs this ` +
          "checkout's packages through the repository's own install, so run `npm ci` at " +
          "the repository root first",
      );
    }
    at = parent;
  }
}

/**
 * Give the uf copy a `node_modules` of links into this checkout's.
 *
 * The same policy as the toolchain benchmark's: every `@uniflowed/*` points at
 * this checkout's `packages/`, so the suite measures the working tree rather
 * than a published alpha, and everything else points at the install found
 * above. Dot-entries are skipped — they are npm's and Vite's own bookkeeping,
 * and the suite runs each runner by path rather than out of `.bin`.
 */
export function linkDependencies(root: string, repoRoot: string): number {
  const packages = path.join(repoRoot, "packages");
  const installed = installRoot(repoRoot);
  const target = path.join(root, "node_modules");
  fs.mkdirSync(path.join(target, "@uniflowed"), { recursive: true });
  let linked = 0;
  for (const entry of fs.readdirSync(packages)) {
    const link = path.join(target, "@uniflowed", entry);
    fs.rmSync(link, { force: true, recursive: true });
    fs.symlinkSync(path.join(packages, entry), link);
    linked += 1;
  }
  for (const entry of fs.readdirSync(installed)) {
    if (entry.startsWith(".") || entry === "@uniflowed") {
      continue;
    }
    const link = path.join(target, entry);
    fs.rmSync(link, { force: true, recursive: true });
    fs.symlinkSync(path.join(installed, entry), link);
    linked += 1;
  }
  return linked;
}

/** This file is `tools/bench/testing/suite.js`. */
function repositoryRoot(): string {
  const url = import.meta.url;
  if (typeof url !== "string") {
    throw new Error(
      "import.meta.url is not a string here, so the suite cannot find its repository",
    );
  }
  return path.resolve(path.dirname(new URL(url).pathname), "..", "..", "..");
}

export function generateSuites(outDir: string, preset: Preset, repoRoot: string): void {
  refuseInsideRepository(outDir, repoRoot);
  for (const copy of COPIES) {
    const root = path.join(outDir, copy.name);
    fs.rmSync(root, { recursive: true, force: true });
    for (const file of suiteFiles(preset, copy.name)) {
      const target = path.join(root, file.path);
      fs.mkdirSync(path.dirname(target), { recursive: true });
      fs.writeFileSync(target, file.contents);
    }
    if (copy.name === "uf") {
      linkDependencies(root, repoRoot);
    }
  }
}

function main(): void {
  const outDir = process.argv[2];
  if (outDir == null) {
    throw new Error(
      "usage: node tools/bench/testing/suite.js <outDir>\n\n" +
        "  Writes the uf, bun and vitest copies of the suite the testing guide measures.\n" +
        "  <outDir> must be outside this repository.",
    );
  }
  const repoRoot = repositoryRoot();
  const resolved = path.resolve(outDir);
  generateSuites(resolved, GUIDE_PRESET, repoRoot);
  const tests = GUIDE_PRESET.files * GUIDE_PRESET.cases;
  process.stdout.write(
    `wrote ${String(COPIES.length)} copies of ${String(GUIDE_PRESET.files)} files ` +
      `x ${String(GUIDE_PRESET.cases)} cases (${String(tests)} tests, ` +
      `${String(tests * 2)} assertions) under ${resolved}\n`,
  );
}

if (process.argv[1] != null && process.argv[1].endsWith(path.join("testing", "suite.js"))) {
  main();
}
