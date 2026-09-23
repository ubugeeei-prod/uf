// @flow
//
// The manual's table of contents, checked against itself and against the tree.
//
// `docs/app/_design/nav.js` is not decoration: its own comment says the list
// *is* the navigation model — the sidebar renders it, "next page" walks it,
// the home page offers its sections as paths, and each landing page lists its
// section from it. That makes three properties load-bearing, and none of them
// is visible by reading the list.
//
// The first is that an `href` appears once. A page listed twice is listed
// twice in the sidebar, and "next page" — which finds the *first* match —
// turns the run between the two entries into a cycle: a reader following
// "next" through it arrives back where they were and never reaches the pages
// after the second entry at all. `/guide/state` was listed twice, so
// Effects pointed back at State, State pointed forward to Forms, and Terminal
// UI and everything after it was unreachable by reading the manual in order.
//
// The second is that reading ends. The last page of a section points at the
// landing page of the section its reader goes to next, so the same cycle can
// be made out of sections instead of pages: "Why uf" sending its reader to
// "Start" is right, and "Start" sending them back to "Why uf" would be a loop
// that no single page shows.
//
// The third is that every entry names a page that exists and every page is
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

import {
  currentHref,
  entryFor,
  featuredIn,
  nextAfter,
  openSectionFor,
  pages,
  previousBefore,
  sections,
  sourceFor,
} from "../../docs/app/_design/nav.js";
import type { Section } from "../../docs/app/_design/nav.js";

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
      // A segment in brackets is a parameter: its pages are the ones its
      // `generateStaticParams` names, reached from the page above it, and
      // no single entry in the sidebar could stand for them.
      if (String(item.name).startsWith("[")) {
        continue;
      }
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

  it("walks each section from its landing page to its last page, and then on", () => {
    // The property a duplicate breaks, stated as the reader experiences it:
    // open a section's landing page and follow "next page". Every page in the
    // section is visited once, in the order it is listed, and the step after
    // the last one leaves for the landing page the section names — or, at the
    // end of the manual, for nothing.
    for (const section of sections) {
      const listed = [section.landing, ...section.pages].map((page) => page.href);
      const visited = [];
      let at: ?string = listed[0];
      while (at != null && visited.length < listed.length) {
        visited.push(at);
        at = nextAfter(at)?.href;
      }
      expect({ section: section.title, visited }).toEqual({
        section: section.title,
        visited: listed,
      });
      expect({ section: section.title, then: at ?? null }).toEqual({
        section: section.title,
        then: section.then,
      });
    }
  });

  it("sends every section's reader somewhere that ends", () => {
    // `then` names a landing page, and following it from any section reaches
    // the end of the manual without passing through a section twice — the
    // cycle #586 made out of pages, made out of sections instead.
    const byLanding = new Map(sections.map((section) => [section.landing.href, section]));
    const dangling = sections
      .filter((section) => section.then != null && !byLanding.has(section.then))
      .map((section) => section.title);
    expect(dangling).toEqual([]);

    for (const section of sections) {
      const route: Array<string> = [];
      let at: ?Section = section;
      while (at != null && !route.includes(at.title)) {
        route.push(at.title);
        at = at.then == null ? null : byLanding.get(at.then);
      }
      expect({ from: section.title, loops: at != null }).toEqual({
        from: section.title,
        loops: false,
      });
    }
  });

  it("names only pages that exist", () => {
    expect(pages.filter((page) => pageFile(page.href) == null).map((p) => p.href)).toEqual([]);
  });

  it("names every page that exists", () => {
    // `/` is the home page and deliberately outside the manual: it is not a
    // step in any section's reading order, and `entryFor` returns `null` for
    // it.
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

  // The home page and each landing page lead with a section's featured pages
  // (#1420). A feature that is not one of the section's own pages would offer
  // a link the sidebar puts somewhere else; more than five is a second list,
  // not a lead; and a section with pages that features none leads with nothing.
  it("features three to five of each section's own pages, once each", () => {
    for (const section of sections) {
      const own = new Set(section.pages.map((page) => page.href));
      const featured = section.featured;
      expect({
        section: section.title,
        foreign: featured.filter((href) => !own.has(href)),
        repeated: featured.length !== new Set(featured).size,
        count:
          section.pages.length === 0
            ? featured.length === 0
            : featured.length >= Math.min(3, section.pages.length) && featured.length <= 5,
      }).toEqual({ section: section.title, foreign: [], repeated: false, count: true });
      expect(featuredIn(section).map((page) => page.href)).toEqual(featured);
    }
  });

  it("puts every page in exactly one section", () => {
    const total = sections.reduce((sum, section) => sum + 1 + section.pages.length, 0);
    expect(total).toBe(pages.length);
    expect(sections.map((section) => section.title).length).toBe(
      new Set(sections.map((section) => section.title)).size,
    );
  });

  it("names the file each page is written in, for its edit link", () => {
    // `sourceFor` builds the path from the route alone, so it is only right
    // while every listed page is `$page.mdx`. A page written as `$page.js`
    // would get an edit link to a file that does not exist.
    const wrong = pages
      .filter((page) => sourceFor(page.href) !== path.relative(REPO, pageFile(page.href) ?? ""))
      .map((page) => page.href);
    expect(wrong).toEqual([]);
    expect(sourceFor("/")).toBe(null);
  });

  it("steps back through each section to its landing page, and no further", () => {
    for (const section of sections) {
      const listed = [section.landing, ...section.pages].map((page) => page.href);
      const visited = [];
      let at: ?string = listed[listed.length - 1];
      while (at != null && visited.length < listed.length) {
        visited.push(at);
        at = previousBefore(at)?.href;
      }
      expect({ section: section.title, visited, then: at ?? null }).toEqual({
        section: section.title,
        visited: [...listed].reverse(),
        then: null,
      });
    }
  });

  it("marks the listed page a pathname is, or else the deepest listed page it is under", () => {
    expect(currentHref("/guide/routing")).toBe("/guide/routing");
    expect(currentHref("/guide/routing/requests/")).toBe("/guide/routing/requests");
    expect(currentHref("/guide/routing/somewhere-unlisted")).toBe("/guide/routing");
    expect(currentHref("/elsewhere")).toBe(null);
  });

  // The sidebar opens one section and closes the rest (#1404). The one it opens
  // has to be the one holding the entry it marks, or the marked link is inside
  // a closed disclosure and the reader cannot see where they are.
  it("opens the section holding the marked entry, and only for pages in the manual", () => {
    for (const section of sections) {
      for (const page of [section.landing, ...section.pages]) {
        expect(openSectionFor(page.href)?.title).toBe(section.title);
        expect(openSectionFor(`${page.href}/`)?.title).toBe(section.title);
      }
    }
    expect(openSectionFor("/guide/routing/somewhere-unlisted")?.title).toBe(
      openSectionFor("/guide/routing")?.title,
    );
    expect(openSectionFor("/")).toBe(null);
    expect(openSectionFor("/elsewhere")).toBe(null);
  });
});
