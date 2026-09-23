// @flow
//
// The API reference, written from the packages themselves.
//
//   UF_BIN=./target/release/uf node --import @uniflowed/host/register tools/docs/api.js
//
// `tools/docs/build.sh` runs this before `uf build#docs`. For every published
// `@uniflowed/*` package it asks `uf doc --json` for the exported declarations
// — name, kind, signature and the doc comment above it — keeps the ones a
// program can import, and writes one JSON file per package to
// `docs/.generated/api/`. `docs/app/reference/api/[name]/$page.js` renders
// them. Nothing here is checked in: the reference is a function of the source,
// so it cannot describe a signature the package no longer has.
//
// "A program can import" is decided from `package.json#exports`, not from the
// directory. A module that is an export target is public as a whole. A module
// that is not — `internal/client.js` — contributes only the names an export
// target re-exports from it, under the specifier that re-exports them. That is
// the rule a reader's import obeys, so it is the rule the reference follows.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const REPO = path.resolve(path.dirname(fileURLToPath(String(import.meta.url))), "..", "..");
const OUT = path.join(REPO, "docs/.generated/api");

/** One exported declaration, as `uf doc` reports it. */
export type ApiEntry = {|
  readonly name: string,
  readonly kind: string,
  readonly signature: string,
  readonly description: string,
|};

/** An export with no doc comment: `uf doc` reports only documented ones. */
export type BareExport = {|
  readonly name: string,
  readonly kind: string,
  /** The file it is declared in, from the package's root, and its line. */
  readonly file: string,
  readonly line: number,
|};

/** The declarations one import specifier gives a program. */
export type ApiModule = {|
  /** What a program writes after `from`: `@uniflowed/query/cache`. */
  readonly specifier: string,
  /** The files they are declared in, from the package's root. */
  readonly files: $ReadOnlyArray<string>,
  readonly entries: $ReadOnlyArray<ApiEntry>,
  /** Exports that carry no doc comment, so have no entry above. */
  readonly bare: $ReadOnlyArray<BareExport>,
  /** Packages this module re-exports whole, with `export * from`. */
  readonly everythingFrom: $ReadOnlyArray<string>,
|};

export type ApiPackage = {|
  readonly name: string,
  /** The URL segment: the name without its scope. */
  readonly slug: string,
  /** Its directory under `packages/`. */
  readonly dir: string,
  readonly version: string,
  readonly description: string,
  readonly modules: $ReadOnlyArray<ApiModule>,
|};

/** A module `uf doc` reported: its path from the package root, and its entries. */
export type DocModule = {|
  readonly path: string,
  readonly entries: $ReadOnlyArray<ApiEntry>,
|};

/**
 * The file an `exports` value resolves to for an ordinary import, or `null`.
 * A condition object is read in the order a bundler would try for this site:
 * `import`, then `default`, then whatever else names a file.
 */
export function exportTarget(value: mixed): string | null {
  if (typeof value === "string") {
    return value;
  }
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  for (const condition of ["import", "default", "node", "browser"]) {
    const found = exportTarget(value[condition]);
    if (found != null) {
      return found;
    }
  }
  for (const key of Object.keys(value)) {
    const found = exportTarget(value[key]);
    if (found != null) {
      return found;
    }
  }
  return null;
}

/**
 * Every importable specifier and the JavaScript file behind it, from a
 * manifest's `exports`. `./package.json`, non-JavaScript targets and
 * wildcard patterns are not modules a reader imports declarations from.
 */
export function publicModules(
  name: string,
  exportsField: mixed,
): Array<{| specifier: string, file: string |}> {
  const out = [];
  const add = (subpath: string, value: mixed) => {
    const target = exportTarget(value);
    if (target == null || subpath.includes("*") || !/\.(c|m)?js$/.test(target)) {
      return;
    }
    out.push({
      specifier: subpath === "." ? name : `${name}/${subpath.replace(/^\.\//, "")}`,
      file: path.posix.normalize(target.replace(/^\.\//, "")),
    });
  };
  if (typeof exportsField === "string") {
    add(".", exportsField);
  } else if (
    exportsField != null &&
    typeof exportsField === "object" &&
    !Array.isArray(exportsField)
  ) {
    const keys = Object.keys(exportsField);
    if (keys.length > 0 && keys.every((key) => !key.startsWith("."))) {
      // A bare condition object is the package root.
      add(".", exportsField);
    } else {
      for (const key of keys) {
        add(key, exportsField[key]);
      }
    }
  }
  return out;
}

/**
 * The names a module re-exports from other files: `export { a, b as c } from
 * "./x.js"` gives `x.js` → `{ a: "a", b: "c" }` (declared name to public name).
 * `export type { … } from` counts the same. Paths are resolved against the
 * re-exporting file and returned from the package root.
 */
export function reexports(source: string, file: string): Map<string, Map<string, string>> {
  const found = new Map<string, Map<string, string>>();
  const pattern = /^export\s+(?:type\s+)?\{([^}]*)\}\s*from\s*["']([^"']+)["']/gm;
  for (const match of source.matchAll(pattern)) {
    const from = match[2];
    // Another package's names keep that package's specifier as their key.
    const target = from.startsWith(".")
      ? path.posix.normalize(path.posix.join(path.posix.dirname(file), from))
      : from;
    const names = found.get(target) ?? new Map<string, string>();
    for (const part of match[1].split(",")) {
      const cleaned = part
        .replace(/\/\/.*$/gm, "")
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .replace(/^\s*type\s+/, "")
        .trim();
      if (cleaned === "") {
        continue;
      }
      const [declared, alias] = cleaned.split(/\s+as\s+/);
      names.set(declared.trim(), (alias ?? declared).trim());
    }
    found.set(target, names);
  }
  return found;
}

/**
 * The names a module declares and exports itself, with their kind and line:
 * `export function f`, `export type T`, `export component C`, `export const
 * x`, and the local `export { a, b as c }`. Re-exports from another file are
 * `reexports`'s, not this.
 */
export function declaredExports(
  source: string,
): Array<{| name: string, kind: string, line: number |}> {
  const out = [];
  const lines = source.split("\n");
  const declaration =
    /^export\s+(?:default\s+)?(?:declare\s+)?(?:async\s+)?(function\*?|const|let|var|class|opaque\s+type|type|interface|component|hook|enum)\s+([A-Za-z_$][\w$]*)/;
  lines.forEach((text, index) => {
    const match = text.match(declaration);
    if (match != null) {
      out.push({
        name: match[2],
        kind: match[1].replace(/\s+/, " ").replace("*", ""),
        line: index + 1,
      });
    }
  });
  const local = /^export\s+(type\s+)?\{([^}]*)\}(?!\s*from)/gm;
  for (const match of source.matchAll(local)) {
    const line = source.slice(0, match.index ?? 0).split("\n").length;
    for (const part of match[2].split(",")) {
      const cleaned = part
        .replace(/\/\/.*$/gm, "")
        .replace(/^\s*type\s+/, "")
        .trim();
      if (cleaned === "") {
        continue;
      }
      const [declared, alias] = cleaned.split(/\s+as\s+/);
      out.push({
        name: (alias ?? declared).trim(),
        kind: match[1] != null ? "type" : "binding",
        line,
      });
    }
  }
  return out;
}

/** The specifiers a module re-exports whole: `export * from "react"`. */
export function everythingFrom(source: string): Array<string> {
  return [...source.matchAll(/^export\s+(?:type\s+)?\*\s+from\s*["']([^"']+)["']/gm)]
    .map((match) => match[1])
    .filter((specifier, index, all) => all.indexOf(specifier) === index);
}

/**
 * The reference for one package: each public specifier and what a program can
 * import from it. `readFile` reads a file from the package root; `docs` is
 * what `uf doc --json` reported for the package.
 */
export function assemble(
  manifest: {|
    readonly name: string,
    readonly version: string,
    readonly description: string,
    readonly exports: mixed,
    readonly dir: string,
  |},
  docs: $ReadOnlyArray<DocModule>,
  readFile: (file: string) => string | null,
): ApiPackage {
  const byFile = new Map(docs.map((module) => [module.path, module]));
  const modules: Array<ApiModule> = [];
  for (const { specifier, file } of publicModules(manifest.name, manifest.exports)) {
    const entries: Array<ApiEntry> = [];
    const files: Array<string> = [];
    const bare: Array<BareExport> = [];
    const own = byFile.get(file);
    if (own != null && own.entries.length > 0) {
      entries.push(...own.entries);
      files.push(file);
    }
    const source = readFile(file);
    const whole = source == null ? [] : everythingFrom(source);
    if (source != null) {
      const documented = new Set((own?.entries ?? []).map((entry) => entry.name));
      for (const found of declaredExports(source)) {
        if (!documented.has(found.name)) {
          bare.push({ ...found, file });
        }
      }
    }
    if (source != null) {
      for (const [target, names] of reexports(source, file)) {
        // A re-export of another export target is documented under that
        // target's own specifier, where a reader will find it by name.
        if (publicModules(manifest.name, manifest.exports).some((m) => m.file === target)) {
          continue;
        }
        if (!target.includes(".js") && !target.startsWith(".")) {
          for (const alias of names.values()) {
            bare.push({ name: alias, kind: `from ${target}`, file, line: 1 });
          }
          continue;
        }
        const declared = byFile.get(target);
        const picked = (declared?.entries ?? [])
          .filter((entry) => names.has(entry.name))
          .map((entry) => ({ ...entry, name: names.get(entry.name) ?? entry.name }));
        if (picked.length > 0) {
          entries.push(...picked);
          files.push(target);
        }
        // A name re-exported from a file where it has no doc comment.
        const pickedNames = new Set(picked.map((entry) => entry.name));
        const targetSource = readFile(target);
        const declaredThere = targetSource == null ? [] : declaredExports(targetSource);
        for (const [original, alias] of names) {
          if (pickedNames.has(alias)) {
            continue;
          }
          const where = declaredThere.find((found) => found.name === original);
          bare.push({
            name: alias,
            kind: where?.kind ?? "binding",
            file: target,
            line: where?.line ?? 1,
          });
        }
      }
    }
    if (entries.length > 0 || bare.length > 0 || whole.length > 0) {
      const seen = new Set<string>();
      const named = new Set(entries.map((entry) => entry.name));
      modules.push({
        specifier,
        files,
        bare: bare.filter(
          (item, index) =>
            !named.has(item.name) && bare.findIndex((other) => other.name === item.name) === index,
        ),
        everythingFrom: whole,
        entries: entries.filter((entry) => {
          const key = `${entry.kind}:${entry.name}`;
          if (seen.has(key)) {
            return false;
          }
          seen.add(key);
          return true;
        }),
      });
    }
  }
  return {
    name: manifest.name,
    slug: manifest.name.replace(/^@[^/]+\//, ""),
    dir: manifest.dir,
    version: manifest.version,
    description: manifest.description,
    modules,
  };
}

function asDocs(value: mixed): Array<DocModule> {
  const report =
    value != null && typeof value === "object" && !Array.isArray(value) ? value.report : null;
  const modules =
    report != null && typeof report === "object" && !Array.isArray(report) ? report.modules : null;
  if (!Array.isArray(modules)) {
    throw new Error("uf doc --json printed no report.modules");
  }
  return modules.flatMap((module) => {
    if (module == null || typeof module !== "object" || typeof module.path !== "string") {
      return [];
    }
    const entries = Array.isArray(module.entries) ? module.entries : [];
    return [
      {
        path: module.path,
        entries: entries.flatMap((entry) =>
          entry != null &&
          typeof entry === "object" &&
          typeof entry.name === "string" &&
          typeof entry.kind === "string" &&
          typeof entry.signature === "string"
            ? [
                {
                  name: entry.name,
                  kind: entry.kind,
                  signature: entry.signature,
                  description: typeof entry.description === "string" ? entry.description : "",
                },
              ]
            : [],
        ),
      },
    ];
  });
}

function main(): void {
  const named = process.env.UF_BIN ?? "uf";
  const uf = named.includes("/") ? path.resolve(named) : named;
  fs.rmSync(OUT, { recursive: true, force: true });
  fs.mkdirSync(OUT, { recursive: true });
  let written = 0;
  let declarations = 0;
  for (const dir of fs.readdirSync(path.join(REPO, "packages")).sort()) {
    const root = path.join(REPO, "packages", dir);
    const manifestFile = path.join(root, "package.json");
    if (!fs.existsSync(manifestFile)) {
      continue;
    }
    const manifest: mixed = JSON.parse(fs.readFileSync(manifestFile, "utf8"));
    if (
      manifest == null ||
      typeof manifest !== "object" ||
      Array.isArray(manifest) ||
      manifest.private === true ||
      typeof manifest.name !== "string"
    ) {
      continue;
    }
    const name = manifest.name;
    const version = typeof manifest.version === "string" ? manifest.version : "";
    const description = typeof manifest.description === "string" ? manifest.description : "";
    const exportsField = manifest.exports ?? "./index.js";
    const out = execFileSync(uf, ["doc", "--json", "--cwd", root], {
      encoding: "utf8",
      env: { ...process.env, NO_COLOR: "1" },
      maxBuffer: 64 * 1024 * 1024,
    });
    const api = assemble(
      {
        name,
        version,
        description,
        exports: exportsField,
        dir,
      },
      asDocs(JSON.parse(out)),
      (file) => {
        const full = path.join(root, file);
        return fs.existsSync(full) ? fs.readFileSync(full, "utf8") : null;
      },
    );
    fs.writeFileSync(path.join(OUT, `${api.slug}.json`), `${JSON.stringify(api)}\n`);
    written += 1;
    declarations += api.modules.reduce((sum, module) => sum + module.entries.length, 0);
  }
  process.stdout.write(`docs-api: ${declarations} declarations from ${written} packages\n`);
}

if (process.argv[1] != null && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
