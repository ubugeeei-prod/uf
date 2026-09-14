// @flow
//
// What a directory name means to the route path, and the spellings uf reserves
// inside the router root.
//
// Next.js spells a parallel route `@team` and an intercepting route
// `(.)photo`. Neither router had an opinion about either, so both fell through
// to "an ordinary URL segment": `app/@team/$page.js` served `/@team`,
// `app/feed/(.)photo/$page.js` served `/feed/(.)photo` — the test for a
// `(group)` is that the segment *ends* in `)` — and the generated `RoutePath`
// union contained both, so `route("/@team", …)` type checked. A person who
// wrote one got no route and no error, which is worse than not supporting it:
// the project looks like it works. See ubugeeei-prod/uf#267.
//
// Both routers serve both now: `@team` anywhere, and `(.)photo` inside a slot —
// `tests/library/parallel-routes.test.js` is what they do. What this file keeps
// is the grammar: neither is a URL segment, an interception is refused outside
// a slot, and what is spelled like an interception without being one is refused
// everywhere. `crates/uf_router/tests/reserved_names.rs` holds the two routers
// to the same lists; this file is the build router's half of the behaviour,
// over real directories, because `scanRoutes` is `readdirSync` and `statSync`
// and a fake tree would prove nothing about which directory is walked into.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, describe, expect, it } from "@uniflowed/test";

import {
  INTERCEPTION_SEGMENTS,
  UNSUPPORTED_SEGMENTS,
  classifyRouteSegment,
  interceptionClimb,
  resolveRouteTarget,
  routeFromSegments,
  scanRoutes,
} from "./internal/routes.js";

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

/** The message `run` threw, or `null` when it threw nothing. */
function thrownBy(run: () => mixed): ?string {
  try {
    run();
    return null;
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}

describe("classifying a directory name", () => {
  it("reads the three kinds a route path is built from", () => {
    expect(classifyRouteSegment("(marketing)")).toEqual({ kind: "group" });
    expect(classifyRouteSegment("[slug]")).toEqual({ kind: "param", name: "slug" });
    expect(classifyRouteSegment("[...path]")).toEqual({ kind: "catchAll", name: "path" });
    expect(classifyRouteSegment("posts")).toEqual({ kind: "literal", name: "posts" });
  });

  it("names a slot rather than reading it as a URL segment", () => {
    // A slot is a route now — `parallel-routes.test.js` is what it does — and
    // the thing that has not changed is that it is not a *URL* segment.
    expect(classifyRouteSegment("@team")).toEqual({ kind: "slot", name: "team" });
    // The `@` has to start the segment: a name that merely contains one is an
    // ordinary literal, and a rule that read it otherwise would refuse it.
    expect(classifyRouteSegment("mail@example")).toEqual({
      kind: "literal",
      name: "mail@example",
    });
  });

  it("names every interception marker Next.js defines, and how far each climbs", () => {
    for (const [segment, marker, climb] of [
      ["(.)photo", "(.)", 0],
      ["(..)photo", "(..)", 1],
      ["(...)photo", "(...)", "root"],
      ["(..)(..)photo", "(..)(..)", 2],
      ["(..)(..)(..)photo", "(..)(..)(..)", 3],
    ]) {
      expect(classifyRouteSegment(segment)).toEqual({
        kind: "interception",
        marker,
        route: "photo",
      });
      expect(interceptionClimb(marker)).toBe(climb);
    }
  });

  it("reads a miscounted marker as an interception that climbs nowhere", () => {
    // The shape of an interception somebody miscounted. Reading these as
    // literals is how `/(....)photo` becomes a page, so they are interceptions
    // uf refuses by name.
    for (const [segment, marker] of [
      ["(....)photo", "(....)"],
      ["(.)(.)photo", "(.)(.)"],
      ["(...)(..)photo", "(...)(..)"],
    ]) {
      expect(classifyRouteSegment(segment)).toEqual({
        kind: "interception",
        marker,
        route: "photo",
      });
      expect(interceptionClimb(marker)).toBe(null);
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

  it("drops a slot from the path the way it drops a group", () => {
    // The two are different decisions with one consequence here: a group
    // organises files and a slot organises rendering, and neither is a URL.
    expect(routeFromSegments(["dashboard", "@team", "members"]).path).toBe("/dashboard/members");
  });

  it("refuses an interception outside a slot before it can become a helper-built URL", () => {
    const message = thrownBy(() => routeFromSegments(["feed", "(.)photo"])) ?? "";

    expect(message).toContain("(.)photo");
    expect(message).toContain("intercepting route");
    expect(message).toContain("`@slot`");
    expect(message).toContain("refused");
  });

  it("builds the URL an interception stands in for, counting URL segments", () => {
    // A slot and a group are not levels, and `(..)` climbs one of what is left.
    expect(routeFromSegments(["feed", "@modal", "(.)photo", "[id]"]).path).toBe("/feed/photo/:id");
    expect(routeFromSegments(["feed", "@modal", "(..)photo", "[id]"]).path).toBe("/photo/:id");
    expect(routeFromSegments(["feed", "(social)", "@modal", "(..)photo"]).path).toBe("/photo");
    expect(routeFromSegments(["shop", "[category]", "@modal", "(...)photo"]).path).toBe("/photo");

    const standsInFor = routeFromSegments(["shop", "[category]", "@modal", "(.)[id]"]);
    expect(standsInFor.path).toBe("/shop/:category/:id");
    expect(standsInFor.params).toEqual([
      { name: "category", catchAll: false },
      { name: "id", catchAll: false },
    ]);
    // A parameter the climb leaves behind is not one the URL captures.
    expect(routeFromSegments(["shop", "[category]", "@modal", "(..)photo"]).params).toEqual([]);
  });

  it("refuses a climb past the router root rather than stopping at it", () => {
    const message = thrownBy(() => routeFromSegments(["@modal", "(..)photo"])) ?? "";

    expect(message).toContain("(..)photo");
    expect(message).toContain("router root");
    expect(message).toContain("refused");
  });
});

describe("scanning a router root that holds one", () => {
  it("refuses every spelling that cannot be an interception, in a slot or not", () => {
    for (const segment of UNSUPPORTED_SEGMENTS) {
      for (const files of [
        [path.join("feed", segment, "$page.js")],
        ["$layout.js", "$page.js", path.join("@modal", segment, "$page.js")],
      ]) {
        // The build serving `/feed/(....)photo` is the bug. Throwing is the fix,
        // and the message has to say the directory is *refused* — "unsupported"
        // reads as "ignored", and ignored is what it used to be.
        const message = thrownBy(() => scanRoutes(appRoot(files))) ?? "";

        expect(message).toContain(segment);
        expect(message).toContain("refused");
        expect(message).toContain("267");
      }
    }
  });

  it("refuses every interception outside a slot, for its place rather than its spelling", () => {
    for (const segment of INTERCEPTION_SEGMENTS) {
      const message = thrownBy(() => scanRoutes(appRoot([path.join("feed", segment, "$page.js")])));

      expect(message ?? "").toContain(segment);
      expect(message ?? "").toContain("`@slot`");
      expect(message ?? "").not.toContain("is not a marker uf reads");
    }
  });

  it("refuses an interception that holds no page, because it is still one", () => {
    const root = appRoot(["$page.js", path.join("feed", "(.)photo", "$layout.js")]);

    expect(() => scanRoutes(root)).toThrow("(.)photo");
  });

  it("leaves a private directory alone, because no router walks into it", () => {
    // A leading `.` or `_` means the directory is a place to put things rather
    // than a route, so `app/_drafts/(.)photo/` was never going to be served
    // and refusing it would be a rule about a place the router does not look.
    const root = appRoot(["$page.js", path.join("_drafts", "(.)photo", "notes.js")]);

    expect(scanRoutes(root).routes.map((route) => route.path)).toEqual(["/"]);
  });

  it("still serves a route group, which is the spelling next to it", () => {
    const root = appRoot([path.join("(marketing)", "about", "$page.js")]);

    expect(scanRoutes(root).routes.map((route) => route.path)).toEqual(["/about"]);
  });

  it("selects the route files for the requested application target", () => {
    const root = appRoot([
      path.join("guide", "$page.js"),
      path.join("guide", "$page.web.jsx"),
      path.join("guide", "$page.native.mdx"),
      path.join("guide", "$page.ios.js"),
      path.join("guide", "$page.android.jsx"),
    ]);

    expect(scanRoutes(root, { target: "web" }).routes[0].page).toBe(
      path.join(root, "guide", "$page.web.jsx"),
    );
    expect(scanRoutes(root, { target: "native" }).routes[0].page).toBe(
      path.join(root, "guide", "$page.native.mdx"),
    );
    expect(scanRoutes(root, { target: "ios" }).routes[0].page).toBe(
      path.join(root, "guide", "$page.ios.js"),
    );
    expect(scanRoutes(root, { target: "android" }).routes[0].page).toBe(
      path.join(root, "guide", "$page.android.jsx"),
    );
  });

  it("defaults a React Native config to the native route target", () => {
    expect(resolveRouteTarget({ app: { framework: "react-native" } })).toBe("native");
    expect(resolveRouteTarget({ app: { targets: ["react-native"] } }, "ios")).toBe("ios");
    expect(() => resolveRouteTarget({ app: { targets: ["web"] } }, "native")).toThrow(
      "react-native",
    );
  });
});
