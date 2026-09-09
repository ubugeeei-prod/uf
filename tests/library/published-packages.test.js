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
import os from "node:os";
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
 * A cache directory of this test's own, removed when the process exits.
 *
 * See the comment inside `packedPaths` for why it is not npm's default.
 */
const cache: string = (() => {
  const made = fs.mkdtempSync(path.join(os.tmpdir(), "uf-pack-cache-"));
  process.on("exit", () => {
    fs.rmSync(made, { recursive: true, force: true });
  });
  return made;
})();

/**
 * The paths npm would publish for the package in `directory`, as npm sees them.
 *
 * `--dry-run` is what keeps this offline and side-effect free: npm reports the
 * list and writes no tarball.
 */
const packedPaths = (directory: string): Set<string> => {
  let stdout;
  try {
    stdout = execFileSync("npm", ["pack", "--dry-run", "--json"], {
      cwd: directory,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      // npm's default cache is `~/.npm`, and it writes there even for a dry
      // run that fetches nothing. A machine whose cache holds root-owned files
      // — a real state, and one npm's own error tells you to fix with `sudo` —
      // then fails this test with `EPERM ... /Users/you/.npm/_cacache`, which
      // reads as a packaging fault and is not one. A cache of our own removes
      // the question: it costs a directory and depends on nothing a person did
      // to their machine before today.
      env: { ...process.env, npm_config_cache: cache },
    });
  } catch (error) {
    const said = String(error.stderr ?? "").trim();
    throw new Error(
      `\`npm pack --dry-run\` failed in ${directory}. This test asks npm what it ` +
        `would publish, so npm has to be able to run:\n${said || String(error)}`,
    );
  }
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
 * `source` with its comments replaced by spaces, and everything else — the
 * string, template and regular-expression literals included — left where it
 * was.
 *
 * Scanning raw source for import specifiers reads the ones inside comments
 * too, and a package whose comment shows `await import("./client.js")` as an
 * example was reported as publishing a file it does not have. Deleting the
 * comments first is the whole fix, and doing it correctly means knowing when a
 * `/` opens one: inside a string it does not, and after a value a `/` is
 * division rather than the start of a regular expression whose body could
 * contain `//`.
 *
 * Comments become spaces rather than nothing so that every offset, and so
 * every line, is the one the file has.
 */
const withoutComments = (source: string): string => {
  const out = source.split("");
  const blank = (from: number, to: number) => {
    for (let at = from; at < to; at += 1) if (out[at] !== "\n") out[at] = " ";
  };
  // The last thing that could end a value. A `/` after one is division; a `/`
  // anywhere else opens a regular expression.
  let afterValue = false;
  let at = 0;
  while (at < source.length) {
    const char = source[at];
    if (char === "/" && source[at + 1] === "/") {
      let end = source.indexOf("\n", at);
      if (end === -1) end = source.length;
      blank(at, end);
      at = end;
    } else if (char === "/" && source[at + 1] === "*") {
      const end = source.indexOf("*/", at + 2);
      const stop = end === -1 ? source.length : end + 2;
      blank(at, stop);
      at = stop;
    } else if (char === '"' || char === "'" || char === "`") {
      at += 1;
      while (at < source.length && source[at] !== char) {
        at += source[at] === "\\" ? 2 : 1;
      }
      at += 1;
      afterValue = true;
    } else if (char === "/" && !afterValue) {
      at += 1;
      let inClass = false;
      while (at < source.length && (inClass || source[at] !== "/")) {
        if (source[at] === "\\") at += 1;
        else if (source[at] === "[") inClass = true;
        else if (source[at] === "]") inClass = false;
        at += 1;
      }
      at += 1;
      afterValue = true;
    } else {
      if (!/\s/.test(char)) afterValue = /[\w$)\].]/.test(char);
      at += 1;
    }
  }
  return out.join("");
};

/**
 * The relative specifiers `source` imports.
 *
 * A regular expression over the comment-free source, not a parser, because
 * the packages are ordinary ESM and the question is only which relative paths
 * appear in an import position. `the scan reads code and not prose` below
 * holds it to that: a pattern that quietly stopped matching would make every
 * assertion here pass over nothing, which is the one way this test could rot
 * without failing.
 */
const relativeImports = (source: string): Array<string> => {
  const pattern = /(?:\bfrom|\bimport|\brequire)\s*\(?\s*["'](\.[^"']*)["']/g;
  const code = withoutComments(source);
  const found = [];
  let match;
  while ((match = pattern.exec(code)) !== null) found.push(match[1]);
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

  it("the scan reads code and not prose", () => {
    // The other half of the same guard, and the one that was missing: a
    // package documenting `import("./client.js")` in a comment was reported
    // as publishing a file it does not have.
    expect(relativeImports(`// see await import("./doc.js")\nimport a from "./real.js";`)).toEqual([
      "./real.js",
    ]);
    expect(
      relativeImports(`/* import x from "./block.js"; */\nimport a from "./real.js";`),
    ).toEqual(["./real.js"]);

    // And the two places a `/` is not the start of a comment. Blanking either
    // would eat the import that follows it.
    expect(relativeImports(`const u = "https://x//y";\nimport a from "./real.js";`)).toEqual([
      "./real.js",
    ]);
    expect(relativeImports(`const r = /a\\/\\/b/;\nimport a from "./real.js";`)).toEqual([
      "./real.js",
    ]);
    expect(relativeImports('const t = `//${x}`;\nimport a from "./real.js";')).toEqual([
      "./real.js",
    ]);

    // Division, so the `/` is not a regular expression that would swallow the
    // rest of the line.
    expect(relativeImports(`const n = a / b / c;\nimport a from "./real.js";`)).toEqual([
      "./real.js",
    ]);
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

  // The suite moved next to the code it tests, so this is no longer a
  // hypothetical: `packages/ui/ui.test.js` is on disk, inside a package that
  // goes to npm. `crates/uf_lib/tests/package_surface.rs` asserts the rule that
  // keeps it out — every manifest's `files` ends `"!*.test.js"`, and the order
  // is load-bearing — and this asserts the consequence, by asking npm.
  //
  // Both are wanted. The Rust one is non-vacuous before a test file exists and
  // names the fix; this one is the only half that could catch npm reading an
  // allowlist differently than we think it does.
  it("no package publishes a test file", () => {
    const shipped = [];
    let beside = 0;
    for (const name of published) {
      for (const file of packed.get(name) ?? new Set()) {
        if (file.endsWith(".test.js")) shipped.push(`@uniflowed/${name} packs ${file}`);
      }
      beside += fs
        .readdirSync(path.join(repository, "packages", name), { recursive: true })
        .filter((entry) => String(entry).endsWith(".test.js")).length;
    }

    expect(shipped).toEqual([]);
    // And there was something to exclude. Without this the assertion above
    // passes on a repository whose suite never moved, which is the state this
    // test was written to stop being silent about.
    expect(beside).toBeGreaterThan(0);
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

/**
 * Every `.js` file `directory` publishes, relative to the repository root.
 *
 * The suite lives beside the packages now, so a `.js` under `packages/` is not
 * necessarily one of them: a `.test.js` is subtracted by the `"!*.test.js"`
 * every manifest's `files` ends with, and never reaches a tarball. The edge
 * below is a rule about what a *published* package imports, so reading a test
 * file would report an edge no consumer can observe — `form.test.js` rendering
 * a `Field.Root` is not `@uniflowed/form` depending on `@uniflowed/ui`.
 */
const sourcesUnder = (directory: string): Array<string> => {
  const found = [];
  const walk = (at: string) => {
    for (const entry of fs.readdirSync(at, { withFileTypes: true })) {
      const full = path.join(at, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== "node_modules") walk(full);
      } else if (entry.name.endsWith(".js") && !entry.name.endsWith(".test.js")) {
        found.push(path.relative(repository, full));
      }
    }
  };
  walk(path.join(repository, directory));
  return found;
};

/**
 * The `@uniflowed/*` specifiers `source` imports, and whether each is a type.
 *
 * Same approach as [`relativeImports`] and for the same reason: the comment-free
 * source, a pattern over import positions, and a test below that the pattern
 * still matches something.
 */
const packageImports = (source: string): Array<{| specifier: string, type: boolean |}> => {
  const pattern = /\bimport\s+(type\s+)?([^;]*?)\bfrom\s*["'](@uniflowed\/[^"']*)["']/g;
  const code = withoutComments(source);
  const found = [];
  let match;
  while ((match = pattern.exec(code)) !== null) {
    // `import { type Foo }` is a type import too, and is the form uf's own
    // formatter produces when a value from the same module is imported beside
    // it. A clause with any binding that is *not* prefixed is a value import.
    const clause = match[2];
    const inlineOnly =
      /\{/.test(clause) &&
      clause
        .replace(/^[^{]*\{|\}[^}]*$/g, "")
        .split(",")
        .map((binding) => binding.trim())
        .filter((binding) => binding !== "")
        .every((binding) => binding.startsWith("type "));
    found.push({ specifier: match[3], type: match[1] != null || inlineOnly });
  }
  return found;
};

describe("the edges between the packages", () => {
  it("the scan finds the imports it is looking for", () => {
    // The same guard `the scan reads code and not prose` gives the other
    // pattern: an assertion over an empty list passes, and would go on passing
    // after the shape of an import changed.
    const found = packageImports(
      'import type { A } from "@uniflowed/ui/field";\n' +
        'import { b } from "@uniflowed/core";\n' +
        'import { type C, d } from "@uniflowed/react";\n' +
        'import { type E } from "@uniflowed/hooks";\n',
    );
    expect(found).toEqual([
      { specifier: "@uniflowed/ui/field", type: true },
      { specifier: "@uniflowed/core", type: false },
      { specifier: "@uniflowed/react", type: false },
      { specifier: "@uniflowed/hooks", type: true },
    ]);
  });

  // `@uniflowed/form` depends on `@uniflowed/ui` for the `FieldSource` type,
  // and the dependency reads backwards: a headless form store should not need
  // a component library. ubugeeei-prod/uf#614 asked for that to be a decision
  // rather than the only thing that compiled, and the decision is to keep the
  // edge and hold it to a type.
  //
  // Kept because the alternative is worse today. Duplicating the type trades a
  // resolvable, documented edge for silent drift — `Field.Root` accepting a
  // shape that `useFieldSource` no longer produces would type-check on both
  // sides and fail only where they meet — and a third package to own the
  // contract costs a name, which #560 is the standing evidence is not free.
  //
  // Held to a type because that is what makes it tolerable: Flow erases it, so
  // nothing of `@uniflowed/ui` is loaded, bundled or run by a project that
  // installs `@uniflowed/form`. The cost is an entry in `package.json` so that
  // `uf check` can resolve it. A *value* crossing this edge would make the
  // component library a runtime dependency of the form store, which is the
  // thing `docs/architecture.md` requires not be true, and is the line this
  // test draws.
  //
  // The trigger to reverse it is #210: once `@uniflowed/form` is on npm,
  // `publishable.sh` allows `ui → form` and the type belongs in the package
  // that produces it. Delete this test then.
  it("the only thing @uniflowed/form takes from @uniflowed/ui is a type", () => {
    const offenders = [];
    let seen = 0;
    for (const file of sourcesUnder("packages/form")) {
      const source = fs.readFileSync(path.join(repository, file), "utf8");
      for (const found of packageImports(source)) {
        if (!found.specifier.startsWith("@uniflowed/ui")) continue;
        seen += 1;
        if (!found.type) offenders.push(`${file} imports a value from ${found.specifier}`);
      }
    }

    expect(offenders).toEqual([]);
    // And the edge is still there to be checked. A rename that made this scan
    // find nothing would leave the assertion above passing over an empty list.
    expect(seen).toBeGreaterThan(0);
  });
});
