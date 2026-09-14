// @flow
//
// The application `bench.js` measures, written out from a preset.
//
// # Why generated
//
// A benchmark of every uf command is a benchmark of a project, and each number
// it prints is a statement about a project of some size. So the size is a
// parameter rather than an accident: `PRESETS` names two, and every file below
// follows from one of them — the same bytes on every machine and on every run,
// which is what makes two runs of this benchmark the same benchmark.
//
// A checked-in application would be one size, would drift from what `uf new`
// writes without anybody noticing, and would live inside this repository, where
// `uf fmt`, `uf lint` and `uf test` at the root would walk into it and report it
// as this project's code.
//
// # What is in it
//
// The shape `uf new` writes — `app.js`, a layout, a home page, the same
// `uf.config.js` and the same manifest — grown along the axes a toolchain's
// cost grows with:
//
//   * `routes` pages, `app/rNNN/$page.js`, each rendering three components and
//     calling into a library module. Routes are what `uf dev` and `uf build`
//     scan, analyse for Server Components, prerender and split.
//   * Three components a route: a card and a panel that stay on the server, and
//     a toggle marked `"use client"` that is shipped to the browser. Plus
//     `components/HotCounter.js`, the client component the home page renders
//     and the HMR stage edits.
//   * `modules` library modules, `lib/mNNN.js`, which are what a linter and a
//     type checker spend their time on: loops, early returns, a generic, an
//     enum read through `match`, a `try`, an `async` retry. Not a file of
//     constants nobody reads, which a linter would be through in a millisecond.
//   * One test file a module, `lib/mNNN.test.js`, of `testsPerModule` cases
//     against `@uniflowed/test` — synchronous and asynchronous, eight shapes in
//     rotation.
//
// No two files are byte-identical: each module's number is in its names and in
// its constants. uf's transform cache is content-addressed, and a fixture of
// copies would be a fixture that flattered it.
//
// Every file has to pass `uf lint` and `uf check` with no errors, and
// `bench.js` refuses to time a fixture that does not: the modern syntax is
// used where uf's Flow asks for it (`readonly` rather than the `+` variance
// sigil, which it reports as deprecated).
//
// # What is not in it
//
// Styles, images, MDX, server actions and route handlers. Each has a cost of
// its own and belongs here when a row is added that would report it — not
// before, because a fixture that exercises what no row reports is a slower
// benchmark saying nothing more.
//
// # Where its dependencies come from
//
// This checkout. The fixture's `node_modules` is a real directory of links into
// the repository's own, so `@uniflowed/*` is the code a benchmark of this
// checkout has to run, rather than whichever alpha the registry last served.
// A directory of links rather than one link to the directory, because Vite
// writes its dependency cache to `node_modules/.vite`: through a single link
// that cache would land inside this repository, shared by every fixture and by
// every run, and a "cold" start would be nothing of the kind.

import { Buffer } from "node:buffer";
import fs from "node:fs";
import path from "node:path";

/** How big a fixture is. Every count in `FixtureSummary` follows from these. */
export type Preset = {
  readonly name: string,
  readonly routes: number,
  readonly modules: number,
  readonly testsPerModule: number,
};

export const PRESETS: $ReadOnlyArray<Preset> = [
  // About the application `uf new` grows into in its first week: enough routes
  // that a per-route cost is visible, small enough to run on every change.
  { name: "small", routes: 10, modules: 20, testsPerModule: 10 },
  // Ten times the routes and modules, which is where a cost that grows with the
  // project stops hiding behind the cost of starting a process.
  { name: "large", routes: 100, modules: 200, testsPerModule: 25 },
];

/** The preset called `name`, or a sentence naming the ones there are. */
export function presetNamed(name: string): Preset {
  const found = PRESETS.find((preset) => preset.name === name);
  if (found == null) {
    const names = PRESETS.map((preset) => preset.name).join(", ");
    throw new Error(
      `there is no fixture preset called "${name}"; the presets are ${names}, or all`,
    );
  }
  return found;
}

/** The versions the fixture's manifest names, read from this checkout. */
export type Versions = { readonly uniflowed: string, readonly react: string };

export type FixtureFile = { readonly path: string, readonly contents: string };

/** What a generated fixture has, as generated — before `uf fmt` reprints it. */
export type FixtureSummary = {
  readonly preset: string,
  readonly routes: number,
  readonly components: number,
  readonly clientComponents: number,
  readonly modules: number,
  readonly testFiles: number,
  readonly tests: number,
  readonly files: number,
  readonly lines: number,
  readonly bytes: number,
};

/** The string the HMR stage rewrites, and the file it is in. */
export const HOT_MARKER = "hmr-0";
export const HOT_FILE = "components/HotCounter.js";

const pad = (n: number): string => String(n).padStart(3, "0");

// Byte for byte what `uf new` writes, so the fixture's configuration is a
// user's configuration and not one tuned for the benchmark.
const CONFIG = `// @flow
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  tasks: {
    dev: { command: "uf dev" },
    build: { command: "uf build" },
    check: { command: "uf check" },
    lint: { command: "uf lint" },
    fmt: { command: "uf fmt" },
    test: { command: "uf test" },
  },
});
`;

const APP = `// @flow
import { routerView } from "@uniflowed/router";

export default routerView("./app");
`;

const LAYOUT = `// @flow

export component Layout(children: mixed) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
`;

const HOT_COUNTER = `"use client";
// @flow
import { useCallback, useState } from "@uniflowed/react";

// The benchmark's HMR stage rewrites this string, and only this string, once an
// edit: the smallest change a person makes, in a component the home page shows.
const MARKER = "${HOT_MARKER}";

export component HotCounter(initial: number) {
  const [count, setCount] = useState(initial);
  const increment = useCallback(() => setCount((value) => value + 1), []);
  return (
    <button type="button" data-marker={MARKER} onClick={increment}>
      count: {count}
    </button>
  );
}
`;

function manifest(preset: Preset, versions: Versions): string {
  const contents = {
    name: `uf-bench-${preset.name}`,
    private: true,
    type: "module",
    dependencies: {
      "@uniflowed/config": versions.uniflowed,
      "@uniflowed/react": versions.uniflowed,
      "@uniflowed/router": versions.uniflowed,
      "@uniflowed/vite": versions.uniflowed,
      react: versions.react,
      "react-dom": versions.react,
    },
    devDependencies: {
      "@uniflowed/test": versions.uniflowed,
    },
  };
  return `${JSON.stringify(contents, null, 2)}\n`;
}

function home(preset: Preset): string {
  const links = [];
  for (let n = 1; n <= preset.routes; n += 1) {
    links.push(`          <li>
            <a href="/r${pad(n)}">Route ${n}</a>
          </li>`);
  }
  return `// @flow
import { HotCounter } from "../components/HotCounter.js";

export component Page() {
  return (
    <main>
      <h1>uf toolchain benchmark</h1>
      <HotCounter initial={0} />
      <nav>
        <ul>
${links.join("\n")}
        </ul>
      </nav>
    </main>
  );
}
`;
}

function routePage(n: number): string {
  const id = pad(n);
  const sample = [1, 2, 3, 4, 5, 6, 7, 8].map((k) => ((n * 7 + k * 13) % 50) + 1);
  return `// @flow
import { Card${id} } from "../../components/Card${id}.js";
import { Panel${id} } from "../../components/Panel${id}.js";
import { Toggle${id} } from "../../components/Toggle${id}.js";
import { grade${id}, summarize${id} } from "../../lib/m${id}.js";

const SAMPLE: $ReadOnlyArray<number> = [${sample.join(", ")}];

export component Page() {
  const stats = summarize${id}(SAMPLE);
  return (
    <main>
      <h1>Route ${n}</h1>
      <Card${id} title="Route ${n} summary" total={stats.total} tags={["alpha", "beta", "r${id}"]} />
      <Panel${id} grade={grade${id}(stats.mean)} />
      <Toggle${id} label="More about route ${n}" />
    </main>
  );
}
`;
}

function card(n: number): string {
  const id = pad(n);
  return `// @flow

export component Card${id}(title: string, total: number, tags: $ReadOnlyArray<string>) {
  const heading = total > ${40 + (n % 30)} ? \`\${title} (a large total)\` : title;
  return (
    <section className="card">
      <h2>{heading}</h2>
      <p>Total: {total}</p>
      <ul>
        {tags.map((tag) => (
          <li key={tag}>{tag}</li>
        ))}
      </ul>
    </section>
  );
}
`;
}

function panel(n: number): string {
  const id = pad(n);
  return `// @flow
import { Grade${id}, label${id} } from "../lib/m${id}.js";

export component Panel${id}(grade: Grade${id}) {
  const label = label${id}(grade);
  return (
    <aside className={\`panel panel-\${label}\`}>
      <strong>{label}</strong>
      <span>panel ${n}</span>
    </aside>
  );
}
`;
}

function toggle(n: number): string {
  const id = pad(n);
  return `"use client";
// @flow
import { useCallback, useState } from "@uniflowed/react";

export component Toggle${id}(label: string) {
  const [open, setOpen] = useState(false);
  const flip = useCallback(() => setOpen((value) => !value), []);
  return (
    <div className="toggle">
      <button type="button" aria-expanded={open} onClick={flip}>
        {label}
      </button>
      {open ? <p>Details for route ${n}.</p> : null}
    </div>
  );
}
`;
}

/** The two grade boundaries of module `n`, shared with its test file. */
function bounds(n: number): { readonly low: number, readonly high: number } {
  return { low: 20 + (n % 10), high: 60 + (n % 20) };
}

function libModule(n: number): string {
  const id = pad(n);
  const { low, high } = bounds(n);
  return `// @flow
//
// Library module ${n} of the generated benchmark fixture.

export enum Grade${id} {
  Low,
  Mid,
  High,
}

export type Stats${id} = {
  readonly count: number,
  readonly total: number,
  readonly mean: number,
  readonly min: number,
  readonly max: number,
};

const LOW = ${low};
const HIGH = ${high};

export function summarize${id}(values: $ReadOnlyArray<number>): Stats${id} {
  if (values.length === 0) {
    return { count: 0, total: 0, mean: 0, min: 0, max: 0 };
  }
  let total = 0;
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const value of values) {
    total += value;
    min = Math.min(min, value);
    max = Math.max(max, value);
  }
  return { count: values.length, total, mean: total / values.length, min, max };
}

export function grade${id}(score: number): Grade${id} {
  if (score < LOW) {
    return Grade${id}.Low;
  }
  return score < HIGH ? Grade${id}.Mid : Grade${id}.High;
}

export function label${id}(grade: Grade${id}): string {
  return match (grade) {
    Grade${id}.Low => "low",
    Grade${id}.Mid => "mid",
    Grade${id}.High => "high",
  };
}

export function slugify${id}(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function groupBy${id}<T>(
  items: $ReadOnlyArray<T>,
  key: (item: T) => string,
): Map<string, Array<T>> {
  const groups: Map<string, Array<T>> = new Map();
  for (const item of items) {
    const name = key(item);
    const group = groups.get(name);
    if (group == null) {
      groups.set(name, [item]);
    } else {
      group.push(item);
    }
  }
  return groups;
}

export function parseQuery${id}(query: string): Map<string, string> {
  const entries: Map<string, string> = new Map();
  for (const pair of query.replace(/^\\?/, "").split("&")) {
    if (pair === "") {
      continue;
    }
    const at = pair.indexOf("=");
    const name = at === -1 ? pair : pair.slice(0, at);
    const value = at === -1 ? "" : pair.slice(at + 1);
    try {
      entries.set(decodeURIComponent(name), decodeURIComponent(value));
    } catch {
      entries.set(name, value);
    }
  }
  return entries;
}

export async function retry${id}<T>(attempt: () => Promise<T>, times: number): Promise<T> {
  let failure: mixed = null;
  for (let tries = 0; tries < times; tries += 1) {
    try {
      return await attempt();
    } catch (error) {
      failure = error;
    }
  }
  throw failure instanceof Error ? failure : new Error(\`m${id} gave up after \${times} attempts\`);
}
`;
}

function gradeLabel(score: number, low: number, high: number): string {
  if (score < low) {
    return "low";
  }
  return score < high ? "mid" : "high";
}

function testCase(id: string, k: number, v: number, n: number): string {
  const { low, high } = bounds(n);
  switch (k % 8) {
    case 0:
      return `  it("adds up a sample (case ${k})", () => {
    expect(summarize${id}([${v}, ${v + 1}, ${v + 2}]).total).toBe(${3 * v + 3});
  });`;
    case 1:
      return `  it("finds the extremes of a sample (case ${k})", () => {
    const stats = summarize${id}([${v + 5}, ${v}, ${v + 9}]);
    expect(stats.min).toBe(${v});
    expect(stats.max).toBe(${v + 9});
  });`;
    case 2:
      return `  it("grades a score of ${v} (case ${k})", () => {
    expect(label${id}(grade${id}(${v}))).toBe("${gradeLabel(v, low, high)}");
  });`;
    case 3:
      return `  it("slugifies a title (case ${k})", () => {
    expect(slugify${id}("  Hello, World ${v}! ")).toBe("hello-world-${v}");
  });`;
    case 4:
      return `  it("groups words by their first letter (case ${k})", () => {
    const groups = groupBy${id}(["apple", "avocado", "banana", "item${v}"], (word) => word.slice(0, 1));
    expect(groups.get("a")?.length).toBe(2);
    expect(groups.get("i")?.length).toBe(1);
  });`;
    case 5:
      return `  it("reads a query string (case ${k})", () => {
    const query = parseQuery${id}("?page=${v}&title=two%20words&flag");
    expect(query.get("page")).toBe("${v}");
    expect(query.get("title")).toBe("two words");
    expect(query.get("flag")).toBe("");
  });`;
    case 6:
      return `  it("retries until an attempt succeeds (case ${k})", async () => {
    let calls = 0;
    const value = await retry${id}(async () => {
      calls += 1;
      if (calls < 2) {
        throw new Error("not yet");
      }
      return ${v};
    }, 3);
    expect(value).toBe(${v});
    expect(calls).toBe(2);
  });`;
    default:
      return `  it("summarises an empty sample (case ${k})", () => {
    expect(summarize${id}([]).count).toBe(0);
  });`;
  }
}

function testFile(n: number, count: number): string {
  const id = pad(n);
  const cases = [];
  for (let k = 0; k < count; k += 1) {
    cases.push(testCase(id, k, (n * 31 + k * 17) % 97, n));
  }
  return `// @flow
import { describe, expect, it } from "@uniflowed/test";

import {
  grade${id},
  groupBy${id},
  label${id},
  parseQuery${id},
  retry${id},
  slugify${id},
  summarize${id},
} from "./m${id}.js";

describe("m${id}", () => {
${cases.join("\n\n")}
});
`;
}

/** Every file of a fixture, in the order they are written. Pure. */
export function fixtureFiles(preset: Preset, versions: Versions): Array<FixtureFile> {
  if (preset.modules < preset.routes) {
    throw new Error(
      `preset "${preset.name}" has ${preset.routes} routes and ${preset.modules} modules; ` +
        "every route calls into a module of its own, so there have to be at least as many modules",
    );
  }
  const files: Array<FixtureFile> = [
    { path: "package.json", contents: manifest(preset, versions) },
    { path: "uf.config.js", contents: CONFIG },
    { path: "app.js", contents: APP },
    { path: "app/$layout.js", contents: LAYOUT },
    { path: "app/$page.js", contents: home(preset) },
    { path: HOT_FILE, contents: HOT_COUNTER },
  ];
  for (let n = 1; n <= preset.routes; n += 1) {
    const id = pad(n);
    files.push({ path: `app/r${id}/$page.js`, contents: routePage(n) });
    files.push({ path: `components/Card${id}.js`, contents: card(n) });
    files.push({ path: `components/Panel${id}.js`, contents: panel(n) });
    files.push({ path: `components/Toggle${id}.js`, contents: toggle(n) });
  }
  for (let n = 1; n <= preset.modules; n += 1) {
    const id = pad(n);
    files.push({ path: `lib/m${id}.js`, contents: libModule(n) });
    files.push({ path: `lib/m${id}.test.js`, contents: testFile(n, preset.testsPerModule) });
  }
  return files;
}

/** What `files` amounts to, in the terms a reader of a result needs. */
export function describeFixture(
  preset: Preset,
  files: $ReadOnlyArray<FixtureFile>,
): FixtureSummary {
  let lines = 0;
  let bytes = 0;
  for (const file of files) {
    lines += file.contents.split("\n").length - 1;
    bytes += Buffer.byteLength(file.contents, "utf8");
  }
  return {
    preset: preset.name,
    routes: preset.routes,
    components: preset.routes * 3 + 1,
    clientComponents: preset.routes + 1,
    modules: preset.modules,
    testFiles: preset.modules,
    tests: preset.modules * preset.testsPerModule,
    files: files.length,
    lines,
    bytes,
  };
}

/**
 * Write a fixture into `root`, which must be empty or absent.
 *
 * Empty rather than cleared here: a generator that deleted what it found would
 * be one wrong argument away from deleting something that was not a fixture.
 * The caller removes the previous run's directory, by name, first.
 */
export function generateFixture(root: string, preset: Preset, versions: Versions): FixtureSummary {
  if (fs.existsSync(root) && fs.readdirSync(root).length > 0) {
    throw new Error(
      `${root} is not empty. A fixture is written into a fresh directory, so that nothing ` +
        "an earlier run left behind is measured along with it",
    );
  }
  const files = fixtureFiles(preset, versions);
  for (const file of files) {
    const target = path.join(root, file.path);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, file.contents);
  }
  return describeFixture(preset, files);
}

/** This checkout's `node_modules`, or a sentence saying how to get one. */
export function requireInstall(repoRoot: string): string {
  const installed = path.join(repoRoot, "node_modules");
  if (!fs.existsSync(path.join(installed, "@uniflowed", "vite", "package.json"))) {
    throw new Error(
      `${installed} has no @uniflowed/vite. The fixture runs this checkout's packages ` +
        "through the repository's own install, so run `npm ci` at the repository root first",
    );
  }
  return installed;
}

/**
 * Give the fixture a `node_modules` of links into this checkout's.
 *
 * Dot-entries other than `.bin` are left out: they are npm's and Vite's own
 * bookkeeping (`.package-lock.json`, `.vite`), and a link to one would be a
 * fixture writing a cache into this repository.
 */
export function linkDependencies(root: string, repoRoot: string): number {
  const source = requireInstall(repoRoot);
  const target = path.join(root, "node_modules");
  fs.mkdirSync(target, { recursive: true });
  let linked = 0;
  for (const entry of fs.readdirSync(source)) {
    if (entry.startsWith(".") && entry !== ".bin") {
      continue;
    }
    fs.symlinkSync(path.join(source, entry), path.join(target, entry));
    linked += 1;
  }
  return linked;
}

/**
 * Refuse a work directory inside the repository.
 *
 * A fixture there would be walked by `uf fmt`, `uf lint` and `uf test` at the
 * root and reported as this project's code — and deleted by the next run of
 * this benchmark, which is not a thing to do inside a checkout.
 */
export function refuseInsideRepository(directory: string, repoRoot: string): void {
  const relative = path.relative(path.resolve(repoRoot), path.resolve(directory));
  if (relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative))) {
    throw new Error(
      `${directory} is inside this repository. The benchmark generates its fixtures and ` +
        `deletes them again, so pass a --work-dir outside ${repoRoot}`,
    );
  }
}

/** The object `value[key]` holds, or an empty one. */
function objectAt(value: mixed, key: string): { readonly [string]: mixed } {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }
  const field = value[key];
  if (field == null || typeof field !== "object" || Array.isArray(field)) {
    return {};
  }
  return field;
}

function readJson(file: string): mixed {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

/** The packages `uf new` names, which is where the walk below starts. */
const SCAFFOLD_PACKAGES = [
  "@uniflowed/config",
  "@uniflowed/react",
  "@uniflowed/router",
  "@uniflowed/test",
  "@uniflowed/vite",
];

/**
 * The manifest `uf install` is measured on.
 *
 * What an install costs is a function of the dependency graph and not of how
 * many routes an application has, so this is one fixture for every preset. It
 * is the registry half of a scaffolded application: every third-party package
 * the `@uniflowed/*` packages `uf new` names depend on, walked through this
 * repository's workspace and pinned to the version `package-lock.json`
 * resolved. Pinned, so that the number moves when the graph does and not when
 * somebody publishes a patch release overnight; walked rather than listed, so
 * that a dependency added to `@uniflowed/vite` arrives here as the install cost
 * it is.
 *
 * `@uniflowed/*` itself is left out. The registry's copies are the last alpha
 * rather than this checkout, and a benchmark of this checkout that installed
 * them would be timing somebody else's graph.
 */
export function installManifest(repoRoot: string): {
  readonly contents: string,
  readonly dependencies: number,
} {
  const installed = requireInstall(repoRoot);
  const locked = objectAt(readJson(path.join(repoRoot, "package-lock.json")), "packages");
  const external: Set<string> = new Set(["react", "react-dom"]);
  const seen: Set<string> = new Set();
  // A queue walked by index rather than drained: the walk appends to it.
  const queue: Array<string> = [...SCAFFOLD_PACKAGES];
  for (let at = 0; at < queue.length; at += 1) {
    const name = queue[at];
    if (seen.has(name)) {
      continue;
    }
    seen.add(name);
    const packageJson = readJson(path.join(installed, name, "package.json"));
    const optionalPeers = objectAt(packageJson, "peerDependenciesMeta");
    const names = [
      ...Object.keys(objectAt(packageJson, "dependencies")),
      ...Object.keys(objectAt(packageJson, "peerDependencies")).filter(
        (peer) => objectAt(optionalPeers, peer).optional !== true,
      ),
    ];
    for (const dependency of names) {
      if (dependency.startsWith("@uniflowed/")) {
        queue.push(dependency);
      } else {
        external.add(dependency);
      }
    }
  }
  const dependencies: { [string]: string } = {};
  for (const name of [...external].sort()) {
    const version = objectAt(locked, `node_modules/${name}`).version;
    if (typeof version !== "string") {
      throw new Error(
        `package-lock.json pins no version of ${name}, which a scaffolded application ` +
          "depends on; run `npm install` at the repository root so the lock describes the workspace",
      );
    }
    dependencies[name] = version;
  }
  const contents = { name: "uf-bench-install", private: true, type: "module", dependencies };
  return { contents: `${JSON.stringify(contents, null, 2)}\n`, dependencies: external.size };
}
