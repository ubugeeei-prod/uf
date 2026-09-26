// @flow
//
// The packages reference says, for every package, whether a release sends it
// to npm. A reader decides whether to type `uf add @uniflowed/form` from that
// column, so it is checked against the files the release reads rather than
// trusted to be edited when they change:
//
//   * `tools/release/published-packages.txt` — the closure every release sends;
//   * `tools/release/pending-packages.txt` — implemented, waiting on trusted
//     publishing (#1314);
//   * anything else — a workspace package, not meant to be installed.
//
// And every package directory is named in a row, so a new package cannot
// arrive without saying which of the three it is.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

const REPO = path.resolve(path.dirname(fileURLToPath(String(import.meta.url))), "..", "..");
const PAGE = path.join(REPO, "docs/app/reference/packages/$page.mdx");

function listed(file: string): Set<string> {
  return new Set(
    fs
      .readFileSync(path.join(REPO, "tools/release", file), "utf8")
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line !== "" && !line.startsWith("#")),
  );
}

/** Every package in the repository: its name, directory and `private` flag. */
function packages(): Array<{| name: string, dir: string, private: boolean |}> {
  const found = [];
  for (const dir of fs.readdirSync(path.join(REPO, "npm")).map(String).sort()) {
    const manifest = path.join(REPO, "npm", dir, "package.json");
    if (!fs.existsSync(manifest)) {
      continue;
    }
    const parsed: mixed = JSON.parse(fs.readFileSync(manifest, "utf8"));
    if (parsed != null && typeof parsed === "object" && typeof parsed.name === "string") {
      found.push({ name: parsed.name, dir, private: parsed.private === true });
    }
  }
  return found;
}

/** The *Released* cell of every row that names packages, by package name. */
function releasedColumn(): Map<string, string> {
  const cells = new Map<string, string>();
  for (const line of fs.readFileSync(PAGE, "utf8").split("\n")) {
    if (!line.startsWith("| `@uniflowed/")) {
      continue;
    }
    const columns = line.split(" | ");
    if (columns.length < 3) {
      continue;
    }
    const released = columns[columns.length - 1].replace(/\s*\|\s*$/, "");
    for (const match of columns[0].matchAll(/`(@uniflowed\/[\w-]+)`/g)) {
      cells.set(match[1], released);
    }
  }
  return cells;
}

describe("the packages reference's Released column", () => {
  const published = listed("published-packages.txt");
  const pending = listed("pending-packages.txt");
  const column = releasedColumn();

  it("names every package in the repository", () => {
    expect(
      packages()
        .filter((item) => !column.has(item.name))
        .map((item) => item.name),
    ).toEqual([]);
  });

  it("says what a release does with each one", () => {
    const wrong = [];
    for (const item of packages()) {
      const expected = published.has(item.dir)
        ? "On npm"
        : pending.has(item.dir)
          ? "Not yet"
          : "In this repository only";
      const cell = column.get(item.name) ?? "";
      if (!cell.startsWith(expected)) {
        wrong.push({ name: item.name, expected, page: cell });
      }
    }
    expect(wrong).toEqual([]);
  });

  it("never lists a private package as on npm", () => {
    const released = packages().filter((item) => item.private && published.has(item.dir));
    expect(released.map((item) => item.name)).toEqual([]);
  });
});
