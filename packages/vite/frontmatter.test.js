// @flow
//
// Front matter, as `@uniflowed/vite` reads it: YAML, exported as
// `frontmatter`, and no parser for a format it never reads.
//
// Both halves are #1009. `remark-mdx-frontmatter`, which this replaced,
// imported `toml` 3.0.0 whether or not a document held TOML, so every project
// uf scaffolded installed it and failed its first `uf audit`. The first test
// walks what installing `@uniflowed/vite` installs, and fails on a `toml` the
// advisories name. The other two hold that YAML front matter — every page of
// this repository's documentation — still arrives as the value it was.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "@uniflowed/test";
import { valueToEstree } from "estree-util-value-to-estree";
// By path, as `highlight.test.js` explains: `internal/` is not public surface.
import remarkFrontmatterExport from "./internal/frontmatter.js";

const here = path.dirname(fileURLToPath(import.meta.url));

type LockEntry = {
  readonly version?: string,
  readonly link?: boolean,
  readonly resolved?: string,
  readonly dependencies?: { readonly [string]: string },
  readonly optionalDependencies?: { readonly [string]: string },
};

type Lock = { readonly packages: { readonly [string]: LockEntry } };

/**
 * Where npm installed `name` for the package at `from`: the nearest
 * `node_modules` on the way up, which is where Node looks for it too.
 */
function installedAt(lock: Lock, from: string, name: string): ?string {
  let directory = from;
  for (;;) {
    const candidate = `${directory === "" ? "" : `${directory}/`}node_modules/${name}`;
    if (lock.packages[candidate] != null) return candidate;
    if (directory === "") return null;
    const parent = directory.lastIndexOf("/node_modules/");
    directory = parent === -1 ? "" : directory.slice(0, parent);
  }
}

/** Everything installing the package at `start` installs, across workspace links. */
function installs(lock: Lock, start: string): Set<string> {
  const seen = new Set<string>();
  const pending = [start];
  while (pending.length > 0) {
    const at = pending.pop();
    if (at == null) break;
    const entry = lock.packages[at];
    if (entry == null || seen.has(at)) continue;
    seen.add(at);
    if (entry.link === true && entry.resolved != null) {
      pending.push(entry.resolved);
      continue;
    }
    const names = [
      ...Object.keys(entry.dependencies ?? {}),
      ...Object.keys(entry.optionalDependencies ?? {}),
    ];
    for (const name of names) {
      const found = installedAt(lock, at, name);
      if (found != null) pending.push(found);
    }
  }
  return seen;
}

/** Whether a `toml` is one GHSA-82x6-q7mm-w9cf and GHSA-v5mp-jgw5-2x6j name: 4.1.2 or older. */
function advised(version: string): boolean {
  const [major = 0, minor = 0, patch = 0] = version
    .split(".")
    .map((part) => Number.parseInt(part, 10));
  return major < 4 || (major === 4 && (minor < 1 || (minor === 1 && patch <= 2)));
}

/** The initialiser of the variable called `name`, anywhere under `node`. */
function declared(node: mixed, name: string): mixed {
  if (Array.isArray(node)) {
    for (const item of node) {
      const found = declared(item, name);
      if (found !== undefined) return found;
    }
    return undefined;
  }
  if (node == null || typeof node !== "object") return undefined;
  const id = node.id;
  if (
    node.type === "VariableDeclarator" &&
    id != null &&
    typeof id === "object" &&
    id.name === name
  ) {
    return node.init;
  }
  for (const value of Object.values(node)) {
    const found = declared(value, name);
    if (found !== undefined) return found;
  }
  return undefined;
}

/** What the plugin defines as `frontmatter` for a document of `children`. */
function frontmatterOf(children: Array<{ type: string, value: string }>): mixed {
  const tree = { type: "root", children: [...children] };
  const file = {
    message(reason: string) {
      throw new Error(reason);
    },
  };
  remarkFrontmatterExport()(tree, file);
  return declared(tree.children, "frontmatter");
}

describe("front matter", () => {
  it("installs no toml the advisories name, through anything @uniflowed/vite depends on", () => {
    const lock: Lock = JSON.parse(
      fs.readFileSync(path.join(here, "..", "..", "package-lock.json"), "utf8"),
    );
    const named = [...installs(lock, "packages/vite")]
      .filter((at) => at === "node_modules/toml" || at.endsWith("/node_modules/toml"))
      .map((at) => `${at}@${lock.packages[at].version ?? "?"}`)
      .filter((installed) => advised(installed.slice(installed.lastIndexOf("@") + 1)));

    expect(named).toEqual([]);
  });

  it("exports a document's YAML front matter as the value it parses to", () => {
    const exported = frontmatterOf([
      { type: "yaml", value: 'title: "Agents · uf"\norder: 3\ntags: [a, b]' },
    ]);

    expect(exported).toEqual(
      valueToEstree(
        { title: "Agents · uf", order: 3, tags: ["a", "b"] },
        { preserveReferences: true },
      ),
    );
  });

  it("exports undefined for a document with no front matter", () => {
    const exported = frontmatterOf([{ type: "paragraph", value: "" }]);

    expect(exported).toEqual(valueToEstree(undefined, { preserveReferences: true }));
  });
});
