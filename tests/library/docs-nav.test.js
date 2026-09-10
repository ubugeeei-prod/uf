// @flow
//
// The manual's table of contents, checked against itself and against the tree.
//
// `docs/app/_design/nav.js` is not decoration: its own comment says one list
// in reading order *is* the navigation model — the sidebar renders it, the
// masthead highlights a section from it, and `nextAfter` walks it to produce
// the "next page" link at the foot of every page. That makes two properties
// load-bearing, and neither is visible by reading the list.
//
// The first is that an `href` appears once. A page listed twice is listed
// twice in the sidebar, and `nextAfter` — which finds the *first* match —
// turns the run between the two entries into a cycle: a reader following
// "next" through it arrives back where they were and never reaches the pages
// after the second entry at all. `/guide/state` was listed twice, so
// Effects pointed back at State, State pointed forward to Forms, and Terminal
// UI and everything after it was unreachable by reading the manual in order.
//
// The second is that every entry names a page that exists and every page is
// named — a nav that has drifted from `docs/app` either links to a 404 or
// hides a page nobody can navigate to.
//
// The blurbs are checked too, and that is the check that decided which of the
// two State entries to keep: a nav blurb is the same sentence as its page's
// `description` frontmatter, on all of them, so the entry whose blurb matched
// no page was the accident.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

import { entryFor, nextAfter, pages, sections } from "../../docs/app/_design/nav.js";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
const APP = path.join(REPO, "docs/app");

/** The file behind a route, or `null` when the route has no page. */
function pageFile(href: string): string | null {
  const dir = path.join(APP, href.replace(/^\//, ""));
  for (const name of ["$page.mdx", "$page.js"]) {
    const file = path.join(dir, name);
    if (fs.existsSync(file)) {
      return file;
    }
  }
  return null;
}

/** Every route under `docs/app` that has a page, in no particular order. */
function routesOnDisk(): Array<string> {
  const found = [];
  const walk = (dir: string) => {
    for (const item of fs.readdirSync(dir, { withFileTypes: true })) {
      if (!item.isDirectory()) {
        continue;
      }
      const child = path.join(dir, item.name);
      const href = `/${path.relative(APP, child)}`;
      if (pageFile(href) != null) {
        found.push(href);
      }
      walk(child);
    }
  };
  walk(APP);
  return found;
}

/** The `description` a page declares in its frontmatter, or `null`. */
function description(file: string): string | null {
  const head = fs.readFileSync(file, "utf8").slice(0, 2048);
  const match = head.match(/^description:\s*"(.*)"\s*$/m);
  return match == null ? null : match[1];
}

describe("the manual's navigation", () => {
  it("lists each page once", () => {
    const seen = new Set<string>();
    const twice = [];
    for (const page of pages) {
      if (seen.has(page.href)) {
        twice.push(page.href);
      }
      seen.add(page.href);
    }
    expect(twice).toEqual([]);
  });

  it("walks every page exactly once and stops", () => {
    // The property a duplicate breaks, stated as the reader experiences it:
    // start at the top and follow "next page" to the end. There are as many
    // steps as there are pages, no page is visited twice, and the last page
    // has no next.
    const visited = [];
    let at = pages[0].href;
    for (let step = 0; step < pages.length * 2; step += 1) {
      visited.push(at);
      const next = nextAfter(at);
      if (next == null) {
        break;
      }
      at = next.href;
    }
    expect(visited).toEqual(pages.map((page) => page.href));
    expect(nextAfter(pages[pages.length - 1].href)).toBe(null);
  });

  it("names only pages that exist", () => {
    expect(pages.filter((page) => pageFile(page.href) == null).map((p) => p.href)).toEqual([]);
  });

  it("names every page that exists", () => {
    // `/` is the landing page and deliberately outside the manual: it is not a
    // step in the reading order, and `entryFor` returns `null` for it.
    const listed = new Set(pages.map((page) => page.href));
    const missing = routesOnDisk()
      .filter((href) => !listed.has(href))
      .sort();
    expect(missing).toEqual([]);
    expect(entryFor("/")).toBe(null);
  });

  it("blurbs each page with the sentence that page describes itself with", () => {
    const disagreeing = [];
    for (const page of pages) {
      const file = pageFile(page.href);
      if (file == null) {
        continue;
      }
      const declared = description(file);
      if (declared !== page.blurb) {
        disagreeing.push({ href: page.href, nav: page.blurb, page: declared });
      }
    }
    expect(disagreeing).toEqual([]);
  });

  it("puts every page in exactly one section", () => {
    const total = sections.reduce((sum, section) => sum + section.pages.length, 0);
    expect(total).toBe(pages.length);
    expect(sections.map((section) => section.title).length).toBe(
      new Set(sections.map((section) => section.title)).size,
    );
  });
});
