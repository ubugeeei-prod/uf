// @flow
//
// The directory names uf reserves inside the router root without serving.
//
// Next.js spells a parallel route `@team` and an intercepting route
// `(.)photo`. uf has neither, and until now neither router had an opinion
// about the spellings: both fell through to "an ordinary URL segment", so
// `app/@team/_uf.page.js` served `/@team`, `app/feed/(.)photo/_uf.page.js`
// served `/feed/(.)photo` — the test for a `(group)` is that the segment
// *ends* in `)` — and the generated `RoutePath` union contained both, so
// `route("/@team", …)` type checked. A person who wrote one got no route and
// no error, which is worse than not supporting it: the project looks like it
// works. See ubugeeei-prod/uf#267.
//
// Both routers refuse them now. `crates/uf_router/tests/reserved_names.rs`
// holds the two to the same list of spellings; this file is the build router's
// half of the behaviour, over real directories, because `scanRoutes` is
// `readdirSync` and `statSync` and a fake tree would prove nothing about which
// directory is walked into.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, describe, expect, it } from "@uniflowed/test";

import {
  UNSUPPORTED_SEGMENTS,
  classifyRouteSegment,
  routeFromSegments,
  scanRoutes,
} from "../../packages/vite/internal/routes.js";

const roots: Array<string> = [];

afterAll(() => {
  for (const root of roots) {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

/** A router root holding each named file. */
function appRoot(files: $ReadOnlyArray<string>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-segments-"));
  roots.push(root);
  for (const relative of files) {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "// @flow\nexport default function Page() {}\n");
  }
  return root;
}

describe("classifying a directory name", () => {
  it("reads the three kinds a route path is built from", () => {
    expect(classifyRouteSegment("(marketing)")).toEqual({ kind: "group" });
    expect(classifyRouteSegment("[slug]")).toEqual({ kind: "param", name: "slug" });
    expect(classifyRouteSegment("[...path]")).toEqual({ kind: "catchAll", name: "path" });
    expect(classifyRouteSegment("posts")).toEqual({ kind: "literal", name: "posts" });
  });

  it("names a slot rather than reading it as a URL segment", () => {
    expect(classifyRouteSegment("@team")).toEqual({ kind: "slot", name: "team" });
    // The `@` has to start the segment: a name that merely contains one is an
    // ordinary literal, and a rule that read it otherwise would refuse it.
    expect(classifyRouteSegment("mail@example")).toEqual({
      kind: "literal",
      name: "mail@example",
    });
  });

  it("names every interception marker Next.js defines", () => {
    for (const [segment, marker] of [
      ["(.)photo", "(.)"],
      ["(..)photo", "(..)"],
      ["(...)photo", "(...)"],
      ["(..)(..)photo", "(..)(..)"],
    ]) {
      expect(classifyRouteSegment(segment)).toEqual({
        kind: "interception",
        marker,
        route: "photo",
      });
    }
  });

  it("still reads a route group as a group", () => {
    // The near-miss that made `(.)photo` a literal: a group *ends* in `)`, and
    // reading the opening parenthesis alone would turn every group into an
    // interception. A marker with nothing after it names no route, so `(.)` on
    // its own stays the group it has always been.
    for (const name of ["(marketing)", "(shop)", "(.)", "(..)"]) {
      expect(classifyRouteSegment(name)).toEqual({ kind: "group" });
    }
  });

  it("leaves the route path unchanged for everything it serves", () => {
    expect(routeFromSegments(["(marketing)", "posts", "[slug]"]).path).toBe("/posts/:slug");
    expect(routeFromSegments(["docs", "[...path]"]).path).toBe("/docs/:path*");
    expect(routeFromSegments([]).path).toBe("/");
  });
});

describe("scanning a router root that holds one", () => {
  it("refuses every spelling, naming the directory and what it is", () => {
    for (const segment of UNSUPPORTED_SEGMENTS) {
      const root = appRoot([path.join("feed", segment, "_uf.page.js")]);

      // The build serving `/feed/@team` is the bug. Throwing is the fix, and
      // the message has to say the directory is *refused* — "unsupported"
      // reads as "ignored", and ignored is what it used to be.
      let thrown = null;
      try {
        scanRoutes(root);
      } catch (error) {
        thrown = error;
      }
      expect(thrown instanceof Error).toBe(true);
      const message = thrown instanceof Error ? thrown.message : "";
      expect(message).toContain(segment);
      expect(message).toContain("refused");
      expect(message).toContain("267");
    }
  });

  it("refuses a slot that holds no page, because it is still a slot", () => {
    const root = appRoot(["_uf.page.js", path.join("@team", "_uf.layout.js")]);

    expect(() => scanRoutes(root)).toThrow("@team");
  });

  it("leaves a private directory alone, because no router walks into it", () => {
    // A leading `.` or `_` means the directory is a place to put things rather
    // than a route, so `app/_drafts/@team/` was never going to be served and
    // refusing it would be a rule about a place the router does not look.
    const root = appRoot(["_uf.page.js", path.join("_drafts", "@team", "notes.js")]);

    expect(scanRoutes(root).routes.map((route) => route.path)).toEqual(["/"]);
  });

  it("still serves a route group, which is the spelling next to it", () => {
    const root = appRoot([path.join("(marketing)", "about", "_uf.page.js")]);

    expect(scanRoutes(root).routes.map((route) => route.path)).toEqual(["/about"]);
  });
});
