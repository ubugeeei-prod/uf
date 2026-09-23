// @flow
//
// `tools/docs/trees.js`, the check that every directory tree in the
// documentation is sorted by code point — the order `LC_ALL=C ls` gives, which
// is the order that puts `$page.js` and `$layout.js` first in their directory.
//
// The first half runs the check's parts on fixtures small enough to see: which
// fences it reads as trees, in each of the three shapes, and that it passes a
// sorted one and names the row to move in an unsorted one. The last test runs
// it over every `.md` and `.mdx` file the repository owns, which is what holds
// the manual to the rule in CI.

import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

import {
  check,
  compareNames,
  comparePaths,
  describe as describeDisorder,
  disorders,
  listings,
} from "../../tools/docs/trees.js";

const REPO = path.resolve(path.dirname(fileURLToPath(String(import.meta.url))), "..", "..");

/** A page holding one fence, so a fixture reads as the listing it is. */
function page(lang: string, rows: $ReadOnlyArray<string>): string {
  return ["# A page", "", `\`\`\`${lang}`, ...rows, "```", ""].join("\n");
}

/** Every disorder in a page, as the lines the check would print. */
function problems(markdown: string): Array<string> {
  return listings("docs/x.mdx", markdown).listings.flatMap((listing) =>
    disorders(listing).map(describeDisorder),
  );
}

describe("compareNames", () => {
  it("is code-point order: `$` first, uppercase before lowercase, a prefix before its extension", () => {
    const names = [
      "useCounter.js",
      "app.js",
      "Counter.js",
      "app",
      "(group)",
      "@slot",
      "$page.js",
      ".gitignore",
    ];
    expect([...names].sort(compareNames)).toEqual([
      "$page.js",
      "(group)",
      ".gitignore",
      "@slot",
      "Counter.js",
      "app",
      "app.js",
      "useCounter.js",
    ]);
  });

  it("orders by code point above the Basic Multilingual Plane, where UTF-16 order differs", () => {
    // U+FF21 (fullwidth A) is one UTF-16 unit; U+1F600 is a surrogate pair
    // starting 0xD83D, so `<` on strings would put the emoji first.
    expect(compareNames("Ａ", "\u{1F600}")).toBeLessThan(0);
  });
});

describe("comparePaths", () => {
  it("compares segment by segment, so a directory lists above a sibling that extends its name", () => {
    // As whole strings `a-b/x.js` < `a/x.js`, because `-` is below `/`.
    expect(comparePaths("a/x.js", "a-b/x.js")).toBeLessThan(0);
    expect(comparePaths("app/$page.js", "app/@team/$page.js")).toBeLessThan(0);
    expect(comparePaths("app/@team/members/$page.js", "app/members/$page.js")).toBeLessThan(0);
  });
});

describe("indented listings", () => {
  const sorted = page("text", [
    "my-site/",
    "  .gitignore         generated files",
    "  AGENTS.md",
    "  app/",
    "    $layout.js       document layout",
    "    $page.js",
    "    Counter.js",
    "  app.js",
    "  uf.config.js",
  ]);

  it("reads a plain fence of names under `name/` directories as one tree", () => {
    const found = listings("docs/x.mdx", sorted).listings;
    expect(found.map((listing) => [listing.kind, listing.line])).toEqual([["indented", 4]]);
    const site = found[0].root.children[0];
    expect(site.name).toBe("my-site");
    expect(site.children.map((entry) => entry.name)).toEqual([
      ".gitignore",
      "AGENTS.md",
      "app",
      "app.js",
      "uf.config.js",
    ]);
  });

  it("passes a sorted tree", () => {
    expect(problems(sorted)).toEqual([]);
  });

  it("names the row to move in an unsorted one, at every level", () => {
    const unsorted = page("text", [
      "app/",
      "  settings/",
      "    $page.native.js",
      "    $page.android.js",
      "  $layout.js",
    ]);
    expect(problems(unsorted)).toEqual([
      "docs/x.mdx:7: `$page.android.js` sorts before `$page.native.js` in `app/settings/`",
      "docs/x.mdx:8: `$layout.js` sorts before `settings` in `app/`",
    ]);
  });

  it("does not put directories first", () => {
    expect(problems(page("", ["app/", "  components/", "    Button.js", "  $page.js"]))).toEqual([
      "docs/x.mdx:7: `$page.js` sorts before `components` in `app/`",
    ]);
  });

  it("leaves alone a block that indents under something that is not a directory", () => {
    expect(listings("docs/x.mdx", page("text", ["name", "  nested", "other/"])).listings).toEqual(
      [],
    );
  });

  it("leaves alone YAML, and any language that is not plain text", () => {
    expect(listings("docs/x.mdx", page("text", ["a:", "  b: 1"])).listings).toEqual([]);
    expect(listings("docs/x.mdx", page("js", ["app/", "  z.js", "  a.js"])).listings).toEqual([]);
  });
});

describe("drawn listings", () => {
  it("reads box-drawing branches, with the row above as the root", () => {
    const drawn = page("console", [
      "$ uf create app",
      "  demo",
      "  ├─ app",
      "  │  ├─ $page.js",
      "  │  └─ Counter.js",
      "  ├─ app.js",
      "  └─ uf.config.js",
      "done",
    ]);
    const found = listings("docs/x.mdx", drawn).listings;
    expect(found.map((listing) => [listing.kind, listing.root.name])).toEqual([
      ["branches", "demo"],
    ]);
    expect(problems(drawn)).toEqual([]);
  });

  it("reads uf's ASCII branches and `├──` trees too, and reports each", () => {
    const ascii = page("", ["out", "|- dist", "|  `- index.html", "`- .uf"]);
    expect(problems(ascii)).toEqual(["docs/x.mdx:7: `.uf` sorts before `dist` in `out/`"]);
    const wide = page("text", ["src/", "├── z.js", "│   └── x.js", "└── a.js"]);
    expect(problems(wide)).toEqual(["docs/x.mdx:7: `a.js` sorts before `z.js` in `src/`"]);
  });
});

describe("path listings", () => {
  it("reads a plain fence of relative paths, comments and all", () => {
    const table = page("", [
      "app/$page.js              →  /",
      "app/docs/[...path]/$page.js  →  /docs/a/b/c",
      "app/posts/[slug]/$page.js    →  /posts/anything",
    ]);
    expect(listings("docs/x.mdx", table).listings.map((listing) => listing.kind)).toEqual([
      "paths",
    ]);
    expect(problems(table)).toEqual([]);
  });

  it("names the row that sorts above the one written before it", () => {
    const table = page("text", [
      "app/dashboard/$layout.js",
      "app/dashboard/members/$page.js",
      "app/dashboard/@team/$loading.js",
    ]);
    expect(problems(table)).toEqual([
      "docs/x.mdx:6: `app/dashboard/@team/$loading.js` sorts before `app/dashboard/members/$page.js`",
    ]);
  });

  it("leaves alone package names, URLs and commands", () => {
    expect(
      listings("docs/x.mdx", page("", ["@uniflowed/router", "@uniflowed/config"])).listings,
    ).toEqual([]);
    expect(
      listings("docs/x.mdx", page("", ["https://a.dev/x.js", "https://b.dev/y.js"])).listings,
    ).toEqual([]);
    expect(
      listings("docs/x.mdx", page("", ["uf run docs/x.js", "uf run docs/y.js"])).listings,
    ).toEqual([]);
  });
});

describe("`unsorted`", () => {
  it("opts a tree that is not a file system out, and is counted", () => {
    const components = page("text unsorted", ["App/", "  Header", "  Body/", "    Main"]);
    const found = listings("docs/x.mdx", components);
    expect(found.listings).toEqual([]);
    expect(found.unsorted).toBe(1);
  });
});

describe("the repository", () => {
  it("draws every directory tree in its documentation in code-point order", () => {
    const result = check(REPO);
    // A shape the parser stops recognising would pass by reading nothing;
    // the manual has a dozen listings today, so a collapse to none is a bug.
    expect(result.listings).toBeGreaterThan(5);
    expect(result.disorders.map(describeDisorder)).toEqual([]);
  });
});
