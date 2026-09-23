// @flow
//
// The tools uf is compared with, and the input each of them is measured on.
//
// # Why every tool gets its own copy of the application
//
// A comparison is only worth reading if every tool is given what a project
// that uses it would give it. Vite+, Next.js, Vitest, Bun, ESLint, Prettier
// and Biome do not read Flow, and a uf application handed to them would be
// measured failing to parse. So this file writes the application `fixture.js`
// writes a second time, in TypeScript and TSX, from the same preset: the same
// routes, the same three components a route, the same library modules doing
// the same work, and the same tests asserting the same values. What changes is
// only what has to — type annotations, an `as const` object where uf has a Flow
// `enum`, a `switch` where it has `match`, and the file layout each framework
// expects:
//
//   <preset>-vite      a Vite application as Vite+ scaffolds one: `index.html`,
//                      `src/main.tsx`, the routes lazily imported from
//                      `src/App.tsx`, tests beside the modules against
//                      `vite-plus/test`. `vp dev`, `vp build`, `vp test`,
//                      `vp lint`, ESLint, Biome's linter and `tsc` run here.
//   <preset>-next      a Next.js App Router application: `app/layout.tsx`,
//                      `app/rNNN/page.tsx`, the toggles marked "use client".
//                      `next dev` and `next build` run here.
//   <preset>-vitest    the library modules and their tests against `vitest`.
//   <preset>-rstest    the same against `@rstest/core`.
//   <preset>-bun       the same against `bun:test`.
//   <preset>-fmt-*     one copy of the Vite application per formatter, each
//                      formatted by that formatter first, so that each one's
//                      `--check` is timed on a tree it passes.
//   <preset>-flow      the uf application itself, already Flow, as official
//                      Flow reads it (`flowFiles` says what that changes), for
//                      `flow full-check`: the foreground whole-project check,
//                      which is what `flow check` was before Flow made `check`
//                      a question put to a server.
//
// # What a tool that is not there does
//
// It is skipped, by name, with the reason, and the report says so. Nothing is
// ever timed in its place. The npm tools are pinned in `rivals/package.json`
// and found in `rivals/node_modules/.bin` (`npm ci --prefix
// tools/bench/toolchain/rivals`); Bun is found on `PATH`, because it is a
// runtime and not a package. A tool found anywhere else is not used: a number
// against an unstated version is not a number anybody can check.
//
// # What is not compared, and why
//
//   * `next dev`'s HMR. Next.js pushes updates over its own socket protocol,
//     and this harness speaks Vite's. `vp dev` is Vite, so its HMR rows are
//     measured the way uf's are.
//   * `vp dev`'s first document is Vite's `index.html`, which renders nothing on
//     the server: the application's modules are requested by the browser
//     afterwards. uf's and Next.js's first document is the rendered page. The
//     row is reported, and the page says what it is.

import fs from "node:fs";
import path from "node:path";

import type { FixtureFile, Preset } from "./fixture.js";

/** Where the pinned npm tools are installed, relative to the repository. */
export const RIVALS_DIR = path.join("tools", "bench", "toolchain", "rivals");

/** The component the HMR stage edits in the Vite copy, and its URL. */
export const VITE_HOT_FILE = "src/components/HotCounter.tsx";
export const HOT_MARKER = "hmr-0";

const pad = (n: number): string => String(n).padStart(3, "0");

/** The two grade boundaries of module `n`: `fixture.js`'s, so the tests agree. */
function bounds(n: number): { readonly low: number, readonly high: number } {
  return { low: 20 + (n % 10), high: 60 + (n % 20) };
}

function gradeLabel(score: number, low: number, high: number): string {
  if (score < low) {
    return "low";
  }
  return score < high ? "mid" : "high";
}

function libModule(n: number): string {
  const id = pad(n);
  const { low, high } = bounds(n);
  return `// Library module ${n} of the generated benchmark fixture.

export const Grade${id} = { Low: 0, Mid: 1, High: 2 } as const;
export type Grade${id} = (typeof Grade${id})[keyof typeof Grade${id}];

export type Stats${id} = {
  readonly count: number;
  readonly total: number;
  readonly mean: number;
  readonly min: number;
  readonly max: number;
};

const LOW = ${low};
const HIGH = ${high};

export function summarize${id}(values: readonly number[]): Stats${id} {
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
  switch (grade) {
    case Grade${id}.Low:
      return "low";
    case Grade${id}.Mid:
      return "mid";
    default:
      return "high";
  }
}

export function slugify${id}(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function groupBy${id}<T>(items: readonly T[], key: (item: T) => string): Map<string, T[]> {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const name = key(item);
    const group = groups.get(name);
    if (group === undefined) {
      groups.set(name, [item]);
    } else {
      group.push(item);
    }
  }
  return groups;
}

export function parseQuery${id}(query: string): Map<string, string> {
  const entries = new Map<string, string>();
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
  let failure: unknown = null;
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

/** One case: `fixture.js`'s eight shapes, with the same values and assertions. */
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

function testFile(n: number, count: number, runner: string): string {
  const id = pad(n);
  const cases = [];
  for (let k = 0; k < count; k += 1) {
    cases.push(testCase(id, k, (n * 31 + k * 17) % 97, n));
  }
  return `import { describe, expect, it } from "${runner}";

import {
  grade${id},
  groupBy${id},
  label${id},
  parseQuery${id},
  retry${id},
  slugify${id},
  summarize${id},
} from "./m${id}";

describe("m${id}", () => {
${cases.join("\n\n")}
});
`;
}

const HOT_COUNTER = `"use client";

import { useCallback, useState } from "react";

// The benchmark's HMR stage rewrites this string, and only this string, once an
// edit: the smallest change a person makes, in a component the home page shows.
const MARKER = "${HOT_MARKER}";

export function HotCounter({ initial }: { readonly initial: number }) {
  const [count, setCount] = useState(initial);
  const increment = useCallback(() => setCount((value) => value + 1), []);
  return (
    <button type="button" data-marker={MARKER} onClick={increment}>
      count: {count}
    </button>
  );
}
`;

function home(preset: Preset): string {
  const links = [];
  for (let n = 1; n <= preset.routes; n += 1) {
    links.push(`          <li>
            <a href="/r${pad(n)}">Route ${n}</a>
          </li>`);
  }
  return `import { HotCounter } from "../components/HotCounter";

export default function Page() {
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
  return `import { Card${id} } from "../../components/Card${id}";
import { Panel${id} } from "../../components/Panel${id}";
import { Toggle${id} } from "../../components/Toggle${id}";
import { grade${id}, summarize${id} } from "../../lib/m${id}";

const SAMPLE: readonly number[] = [${sample.join(", ")}];

export default function Page() {
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
  return `type Props = {
  readonly title: string;
  readonly total: number;
  readonly tags: readonly string[];
};

export function Card${id}({ title, total, tags }: Props) {
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
  return `import { type Grade${id}, label${id} } from "../lib/m${id}";

export function Panel${id}({ grade }: { readonly grade: Grade${id} }) {
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

import { useCallback, useState } from "react";

export function Toggle${id}({ label }: { readonly label: string }) {
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

function manifest(name: string, dependencies: { [string]: string }): string {
  return `${JSON.stringify({ name, private: true, type: "module", dependencies }, null, 2)}\n`;
}

const TSCONFIG = `${JSON.stringify(
  {
    compilerOptions: {
      target: "ES2022",
      lib: ["ES2022", "DOM", "DOM.Iterable"],
      module: "ESNext",
      moduleResolution: "bundler",
      jsx: "react-jsx",
      strict: true,
      noEmit: true,
      isolatedModules: true,
      skipLibCheck: true,
    },
    include: ["src"],
  },
  null,
  2,
)}\n`;

// The configuration `npm init @eslint/config` writes for a TypeScript project:
// ESLint's recommended rules and typescript-eslint's.
const ESLINT_CONFIG = `import js from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import tseslint from "typescript-eslint";

export default defineConfig([
  globalIgnores(["dist"]),
  js.configs.recommended,
  tseslint.configs.recommended,
]);
`;

const VITE_CONFIG = `import react from "@vitejs/plugin-react";
import { defineConfig } from "vite-plus";

export default defineConfig({
  plugins: [react()],
});
`;

const INDEX_HTML = `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>uf toolchain benchmark</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
`;

const MAIN = `import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";

const root = document.getElementById("root");
if (root === null) {
  throw new Error("index.html has no #root to render into");
}
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
`;

function viteApp(preset: Preset): string {
  const routes = [];
  for (let n = 1; n <= preset.routes; n += 1) {
    routes.push(`  "/r${pad(n)}": lazy(() => import("./app/r${pad(n)}/page")),`);
  }
  return `import { type ComponentType, Suspense, lazy } from "react";

import Home from "./app/page";

const routes: Record<string, ComponentType> = {
${routes.join("\n")}
};

export function App() {
  const Route = routes[window.location.pathname] ?? Home;
  return (
    <Suspense fallback={null}>
      <Route />
    </Suspense>
  );
}
`;
}

const NEXT_LAYOUT = `import type { ReactNode } from "react";

export default function RootLayout({ children }: { readonly children: ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
`;

const NEXT_TSCONFIG = `${JSON.stringify(
  {
    compilerOptions: {
      target: "ES2022",
      lib: ["DOM", "DOM.Iterable", "ES2022"],
      allowJs: true,
      skipLibCheck: true,
      strict: true,
      noEmit: true,
      esModuleInterop: true,
      module: "esnext",
      moduleResolution: "bundler",
      resolveJsonModule: true,
      isolatedModules: true,
      jsx: "react-jsx",
      incremental: true,
      plugins: [{ name: "next" }],
    },
    include: [
      "next-env.d.ts",
      "**/*.ts",
      "**/*.tsx",
      ".next/types/**/*.ts",
      ".next/dev/types/**/*.ts",
    ],
    exclude: ["node_modules"],
  },
  null,
  2,
)}\n`;

/** The shapes of copy there are, which `rivalFiles` writes. */
export type RivalFixture = "vite" | "next" | "vitest" | "rstest" | "bun";

/** Every file of one TypeScript copy of `preset`, in the order written. Pure. */
export function rivalFiles(preset: Preset, kind: RivalFixture): Array<FixtureFile> {
  const files: Array<FixtureFile> = [];
  const libraries = (prefix: string, runner: string | null) => {
    for (let n = 1; n <= preset.modules; n += 1) {
      const id = pad(n);
      files.push({ path: `${prefix}lib/m${id}.ts`, contents: libModule(n) });
      if (runner != null) {
        files.push({
          path: `${prefix}lib/m${id}.test.ts`,
          contents: testFile(n, preset.testsPerModule, runner),
        });
      }
    }
  };
  const application = (prefix: string, page: string) => {
    files.push({ path: `${prefix}components/HotCounter.tsx`, contents: HOT_COUNTER });
    for (let n = 1; n <= preset.routes; n += 1) {
      const id = pad(n);
      files.push({ path: `${prefix}app/r${id}/${page}`, contents: routePage(n) });
      files.push({ path: `${prefix}components/Card${id}.tsx`, contents: card(n) });
      files.push({ path: `${prefix}components/Panel${id}.tsx`, contents: panel(n) });
      files.push({ path: `${prefix}components/Toggle${id}.tsx`, contents: toggle(n) });
    }
  };
  const react = { react: "19.3.0", "react-dom": "19.3.0" };
  switch (kind) {
    case "vite":
      files.push(
        { path: "package.json", contents: manifest(`uf-bench-${preset.name}-vite`, react) },
        { path: "tsconfig.json", contents: TSCONFIG },
        { path: "vite.config.ts", contents: VITE_CONFIG },
        { path: ".gitignore", contents: "node_modules\ndist\n" },
        { path: "eslint.config.js", contents: ESLINT_CONFIG },
        { path: "index.html", contents: INDEX_HTML },
        { path: "src/main.tsx", contents: MAIN },
        { path: "src/App.tsx", contents: viteApp(preset) },
        { path: "src/app/page.tsx", contents: home(preset) },
      );
      application("src/", "page.tsx");
      libraries("src/", "vite-plus/test");
      break;
    case "next":
      files.push(
        { path: "package.json", contents: manifest(`uf-bench-${preset.name}-next`, react) },
        { path: ".gitignore", contents: "node_modules\n.next\nnext-env.d.ts\n" },
        { path: "tsconfig.json", contents: NEXT_TSCONFIG },
        { path: "app/layout.tsx", contents: NEXT_LAYOUT },
        { path: "app/page.tsx", contents: home(preset) },
      );
      application("", "page.tsx");
      libraries("", null);
      break;
    case "vitest":
      files.push({
        path: "package.json",
        contents: manifest(`uf-bench-${preset.name}-vitest`, {}),
      });
      libraries("", "vitest");
      break;
    case "rstest":
      files.push({
        path: "package.json",
        contents: manifest(`uf-bench-${preset.name}-rstest`, {}),
      });
      libraries("", "@rstest/core");
      break;
    case "bun":
      files.push({ path: "package.json", contents: manifest(`uf-bench-${preset.name}-bun`, {}) });
      libraries("", "bun:test");
      break;
  }
  return files;
}

// The `.flowconfig` `flow init` writes, with the two options the application
// needs: Flow enums, and the automatic JSX runtime React 17 introduced.
const FLOWCONFIG = `[ignore]

[untyped]

[declarations]

[include]

[libs]
flow-typed

[lints]

[options]
enums=true
react.runtime=automatic

[strict]
`;

// What `flow-typed create-stub` writes for a package that ships no library
// definition: `any`. The `@uniflowed/*` packages are Flow source rather than a
// library definition, and official Flow following them into this checkout would
// be checking uf's own packages rather than the application. `routerView` has a
// signature because official Flow will not export the result of a call to
// `any` without one.
const STUBS: $ReadOnlyArray<FixtureFile> = [
  {
    path: "flow-typed/npm/uniflowed-react.js",
    contents: `declare module "@uniflowed/react" {\n  declare module.exports: any;\n}\n`,
  },
  {
    path: "flow-typed/npm/uniflowed-test.js",
    contents: `declare module "@uniflowed/test" {\n  declare module.exports: any;\n}\n`,
  },
  {
    path: "flow-typed/npm/uniflowed-router.js",
    contents:
      `declare module "@uniflowed/router" {\n` +
      `  declare export function routerView(root: string): mixed;\n}\n`,
  },
];

/**
 * The uf application as official Flow reads it.
 *
 * Three differences, each one official Flow insists on and uf's does not: the
 * utility types `mixed` and `$ReadOnlyArray` are spelled `unknown` and
 * `ReadonlyArray` (official Flow reports the old names as errors), and a module
 * whose default export is a call has to say what the call returns. And
 * `uf.config.js` is left out, because it is uf's configuration and a project
 * checked by `flow` alone would not have it.
 */
export function flowFiles(files: $ReadOnlyArray<FixtureFile>): Array<FixtureFile> {
  const translated = files
    .filter((file) => file.path !== "uf.config.js")
    .map((file) => ({
      path: file.path,
      contents: file.path.endsWith(".js")
        ? file.contents
            .replace(/\bmixed\b/g, "unknown")
            .replace(/\$ReadOnlyArray\b/g, "ReadonlyArray")
            .replace(/^export default (routerView\(.*\));$/m, "export default ($1 as unknown);")
        : file.contents,
    }));
  return [{ path: ".flowconfig", contents: FLOWCONFIG }, ...STUBS, ...translated];
}

/** Write `files` into `root`, which must be empty or absent, as `fixture.js` does. */
export function writeFiles(root: string, files: $ReadOnlyArray<FixtureFile>): void {
  if (fs.existsSync(root) && fs.readdirSync(root).length > 0) {
    throw new Error(`${root} is not empty, and a fixture is written into a fresh directory`);
  }
  for (const file of files) {
    const target = path.join(root, file.path);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, file.contents);
  }
}

/**
 * Give a copy a `node_modules` of links into `installed`.
 *
 * A directory of links rather than a link to the directory, for the reason
 * `fixture.js` gives: Vite and Vitest write their caches into
 * `node_modules/.vite`, and through one link those would be shared.
 */
export function linkInto(root: string, installed: string): void {
  const target = path.join(root, "node_modules");
  fs.mkdirSync(target, { recursive: true });
  for (const entry of fs.readdirSync(installed)) {
    if (entry.startsWith(".") && entry !== ".bin") {
      continue;
    }
    fs.symlinkSync(path.join(installed, entry), path.join(target, entry));
  }
}

/**
 * Give a copy a `node_modules` of hard links to the files in `installed`.
 *
 * For Next.js, which cannot be given links: Turbopack resolves a link to where
 * it points, and refuses to compile a file outside the project it was started
 * in. A hard link is a file in the project. Where the two directories are on
 * different file systems it is a copy instead, which is slower to make and
 * the same to measure. Links inside the tree — `.bin` — are kept as links, and
 * point into the copy because npm writes them relative.
 */
export function mirrorInto(
  root: string,
  installed: string,
  skip: $ReadOnlyArray<string> = [],
): void {
  const walk = (from: string, to: string, top: boolean) => {
    fs.mkdirSync(to, { recursive: true });
    for (const entry of fs.readdirSync(from, { withFileTypes: true })) {
      const name = String(entry.name);
      if (top && (skip.includes(name) || name === ".package-lock.json")) {
        continue;
      }
      const source = path.join(from, name);
      const target = path.join(to, name);
      if (entry.isSymbolicLink()) {
        fs.symlinkSync(fs.readlinkSync(source), target);
      } else if (entry.isDirectory()) {
        walk(source, target, false);
      } else {
        try {
          fs.linkSync(source, target);
        } catch {
          fs.copyFileSync(source, target);
        }
      }
    }
  };
  walk(installed, path.join(root, "node_modules"), true);
}

/**
 * Where a tool comes from: the pinned npm install, or `PATH`.
 *
 * `module`, when there is one, is the program's path inside the pinned
 * `node_modules` rather than `.bin/<bin>`: a tool whose `.bin` entry is a Node
 * script that only starts a native executable is measured as the executable,
 * so its row is not charged for a Node start-up the tool itself does not need.
 */
export type ToolSource = {
  readonly name: string,
  readonly bin: string,
  readonly from: "rivals" | "path",
  readonly module?: string,
};

/**
 * TypeScript 7's native compiler — the Go port, `tsgo` — for this platform.
 *
 * `typescript@7` ships it as an optional dependency per platform and puts a
 * Node launcher in front of it as `bin/tsc`. The launcher `execve`s the
 * executable, so all it adds is a Node start-up; the row times what the
 * launcher starts. It is installed under an alias because `typescript` itself
 * is pinned at 6, the JavaScript compiler, for the `tsc` row and for
 * `typescript-eslint`.
 */
export const NATIVE_TYPESCRIPT: string = path.join(
  "@typescript",
  `typescript-${process.platform}-${process.arch}`,
  "lib",
  "tsc",
);

/** Every tool but uf, in the order the report lists them. */
export const RIVAL_TOOLS: $ReadOnlyArray<ToolSource> = [
  { name: "vp", bin: "vp", from: "rivals" },
  { name: "next", bin: "next", from: "rivals" },
  { name: "vitest", bin: "vitest", from: "rivals" },
  { name: "rstest", bin: "rstest", from: "rivals" },
  { name: "bun", bin: "bun", from: "path" },
  { name: "eslint", bin: "eslint", from: "rivals" },
  { name: "prettier", bin: "prettier", from: "rivals" },
  { name: "biome", bin: "biome", from: "rivals" },
  { name: "flow", bin: "flow", from: "rivals" },
  { name: "tsc", bin: "tsc", from: "rivals" },
  { name: "tsgo", bin: "tsc", from: "rivals", module: NATIVE_TYPESCRIPT },
  // pnpm is a name here, not a command this repository runs: it is one of the
  // tools being measured.
  { name: "pnpm", bin: "pnpm", from: "rivals" },
];

/** Every tool's name, uf first. */
export const TOOL_NAMES: $ReadOnlyArray<string> = ["uf", ...RIVAL_TOOLS.map((tool) => tool.name)];

/** The first executable called `bin` on `pathList`, or null. */
export function onPath(bin: string, pathList: string): string | null {
  for (const directory of pathList.split(path.delimiter)) {
    if (directory === "") {
      continue;
    }
    const candidate = path.join(directory, bin);
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      if (fs.statSync(candidate).isFile()) {
        return candidate;
      }
    } catch {
      // Not here.
    }
  }
  return null;
}

/** The program a tool is, or the sentence saying why it is skipped. */
export function locateTool(
  tool: ToolSource,
  repoRoot: string,
  pathList: string,
):
  | { readonly program: string, readonly skipped: null }
  | { readonly program: null, readonly skipped: string } {
  if (tool.from === "path") {
    const found = onPath(tool.bin, pathList);
    return found == null
      ? { program: null, skipped: `\`${tool.bin}\` is not on PATH` }
      : { program: found, skipped: null };
  }
  const installed = tool.module ?? path.join(".bin", tool.bin);
  const program = path.join(repoRoot, RIVALS_DIR, "node_modules", installed);
  return fs.existsSync(program)
    ? { program, skipped: null }
    : {
        program: null,
        skipped:
          `${RIVALS_DIR}/node_modules has no ${installed}: ` +
          `run \`npm ci --prefix ${RIVALS_DIR}\` to install the pinned comparison tools`,
      };
}

/** The version in what `<tool> --version` printed: `Version: 2.5.12` is `2.5.12`. */
export function versionIn(printed: string): string {
  const found = printed.match(/\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?/);
  return found == null ? "unknown" : found[0];
}

/** A copy of the application, by the directory suffix it is written under. */
export type Copy =
  | "vite"
  | "next"
  | "vitest"
  | "rstest"
  | "bun"
  | "flow"
  | "fmt-vp"
  | "fmt-prettier"
  | "fmt-biome";

/** One command of one tool that runs to completion, like `uf lint`. */
export type OneShotSpec = {
  readonly tool: string,
  /** The stage family, shared with uf's: fmt, lint, check, test, build. */
  readonly stage: string,
  readonly title: string,
  readonly copy: Copy,
  readonly args: $ReadOnlyArray<string>,
  /** What a cold run removes first, relative to the copy. */
  readonly caches: $ReadOnlyArray<string>,
  /** Run once before anything is timed: a formatter formatting its copy. */
  readonly setup: $ReadOnlyArray<string> | null,
  /** How many tests the run has to report passing, for a test runner. */
  readonly tests: boolean,
};

/** Every one-shot comparison, in the order they run. */
export const ONE_SHOT_RIVALS: $ReadOnlyArray<OneShotSpec> = [
  {
    tool: "vp",
    stage: "fmt",
    title: "format check",
    copy: "fmt-vp",
    args: ["fmt", "--check"],
    caches: [],
    setup: ["fmt"],
    tests: false,
  },
  {
    tool: "prettier",
    stage: "fmt",
    title: "format check",
    copy: "fmt-prettier",
    args: ["--check", "."],
    caches: [],
    setup: ["--write", "."],
    tests: false,
  },
  {
    tool: "biome",
    stage: "fmt",
    title: "format check",
    copy: "fmt-biome",
    args: ["format", "."],
    caches: [],
    setup: ["format", "--write", "."],
    tests: false,
  },
  {
    tool: "vp",
    stage: "lint",
    title: "lint",
    copy: "vite",
    args: ["lint"],
    caches: [],
    setup: null,
    tests: false,
  },
  {
    tool: "eslint",
    stage: "lint",
    title: "lint",
    copy: "vite",
    args: ["."],
    caches: [],
    setup: null,
    tests: false,
  },
  {
    tool: "biome",
    stage: "lint",
    title: "lint",
    copy: "vite",
    args: ["lint", "."],
    caches: [],
    setup: null,
    tests: false,
  },
  {
    tool: "flow",
    stage: "check",
    title: "type check",
    copy: "flow",
    args: ["full-check"],
    caches: [],
    setup: null,
    tests: false,
  },
  {
    tool: "tsc",
    stage: "check",
    title: "type check",
    copy: "vite",
    args: ["--noEmit"],
    caches: [],
    setup: null,
    tests: false,
  },
  {
    tool: "tsgo",
    stage: "check",
    title: "type check",
    copy: "vite",
    args: ["--noEmit"],
    caches: [],
    setup: null,
    tests: false,
  },
  {
    tool: "vp",
    stage: "test",
    title: "test suite",
    copy: "vite",
    args: ["test"],
    caches: [path.join("node_modules", ".vite")],
    setup: null,
    tests: true,
  },
  {
    tool: "vitest",
    stage: "test",
    title: "test suite",
    copy: "vitest",
    args: ["run"],
    caches: [path.join("node_modules", ".vite")],
    setup: null,
    tests: true,
  },
  {
    tool: "rstest",
    stage: "test",
    title: "test suite",
    copy: "rstest",
    args: ["run"],
    // What Rstest keeps between runs: the result record `--changed` and the
    // failed-first ordering read. It keeps no transform cache by default.
    caches: [path.join("node_modules", ".cache")],
    setup: null,
    tests: true,
  },
  {
    tool: "bun",
    stage: "test",
    title: "test suite",
    copy: "bun",
    args: ["test"],
    caches: [],
    setup: null,
    tests: true,
  },
  {
    tool: "vp",
    stage: "build",
    title: "production build",
    copy: "vite",
    args: ["build"],
    caches: ["dist", path.join("node_modules", ".vite")],
    setup: null,
    tests: false,
  },
  {
    tool: "next",
    stage: "build",
    title: "production build",
    copy: "next",
    args: ["build"],
    caches: [".next"],
    setup: null,
    tests: false,
  },
];

/** One dev server, timed to its first document and, where it speaks Vite's protocol, its HMR. */
export type DevSpec = {
  readonly tool: string,
  readonly copy: Copy,
  readonly args: (port: number) => $ReadOnlyArray<string>,
  readonly caches: $ReadOnlyArray<string>,
  /** The file the HMR stage edits and the URL it is served at. */
  readonly hmr: { readonly file: string, readonly urlPath: string } | null,
  /** Why there is no `hmr`, when there is not. */
  readonly hmrSkipped: string | null,
};

export const DEV_RIVALS: $ReadOnlyArray<DevSpec> = [
  {
    tool: "vp",
    copy: "vite",
    // The address is given because Vite's default, `localhost`, is `::1` alone
    // on some machines, and the harness asks `127.0.0.1` as it does of uf.
    args: (port) => ["dev", "--port", String(port), "--strictPort", "--host", "127.0.0.1"],
    caches: [path.join("node_modules", ".vite")],
    hmr: { file: VITE_HOT_FILE, urlPath: `/${VITE_HOT_FILE}` },
    hmrSkipped: null,
  },
  {
    tool: "next",
    copy: "next",
    args: (port) => ["dev", "--port", String(port), "--hostname", "127.0.0.1"],
    caches: [".next"],
    hmr: null,
    hmrSkipped:
      "next dev pushes its updates over a socket protocol of its own, and the HMR stage speaks Vite's",
  },
];

/** The package managers timed on the manifest `uf install` is timed on. */
export const INSTALL_RIVALS: $ReadOnlyArray<string> = ["pnpm", "bun"];

/**
 * The number of passing tests a runner reports, or null if it printed none.
 *
 * Vitest and `vp test`: `Tests  200 passed (200)`. Rstest: `Tests 200 passed`.
 * Bun: ` 200 pass`. Colour
 * codes are taken out first: a runner on CI colours its summary whether or not
 * anybody is reading it.
 */
export function testsPassed(output: string): number | null {
  const printed = output.replace(/\u001b\[[0-9;]*m/g, "");
  const vitest = printed.match(/Tests\s+(\d+) passed/);
  if (vitest != null) {
    return Number(vitest[1]);
  }
  const bun = printed.match(/^\s*(\d+) pass$/m);
  return bun == null ? null : Number(bun[1]);
}
