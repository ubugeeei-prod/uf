// @flow
//
// The link builder behind the generated `router.js`.
//
// `uf` writes a `router.js` naming every route and the parameters it takes,
// and the guide tells people to import `route` from it. That was a
// `declare export function` — a declaration with nothing behind it — so the
// call type checked and returned `undefined` at run time, and a route with no
// parameters could not even be typed, because its parameters were `empty` and
// nothing inhabits that. See ubugeeei-prod/uf#653.
//
// `buildRoute` is the runtime the generated `route` delegates to. The property
// that matters is not that it concatenates strings: it is that it and the
// matcher agree, which is why the round trip below is the centre of this file.
// A builder with its own idea of the pattern grammar would produce links that
// 404, and the 404 is the only place anybody would find out.

import { buildRoute, matchRoute } from "@uniflowed/router/routing";
import { describe, expect, it } from "@uniflowed/test";

/** The smallest record `matchRoute` will rank. */
function record(routePath: string) {
  return {
    path: routePath,
    params: [],
    mdx: false,
    file: `app${routePath}/$page.js`,
    layouts: [],
  };
}

describe("a route pattern and its parameters become a URL", () => {
  it("builds a static route from the pattern alone", () => {
    expect(buildRoute("/")).toBe("/");
    expect(buildRoute("/about")).toBe("/about");
    // The parameters are optional rather than merely ignorable: this is the
    // call the generated `route("/about")` makes.
    expect(buildRoute("/about", {})).toBe("/about");
  });

  it("substitutes a single parameter", () => {
    expect(buildRoute("/posts/:slug", { slug: "hello-world" })).toBe("/posts/hello-world");
    expect(buildRoute("/users/:id/edit", { id: "42" })).toBe("/users/42/edit");
  });

  it("spreads a catch-all over every remaining segment", () => {
    expect(buildRoute("/docs/:path*", { path: ["guide", "routing"] })).toBe("/docs/guide/routing");
    expect(buildRoute("/docs/:path*?", { path: ["guide", "routing"] })).toBe("/docs/guide/routing");
    // Nothing to spread is the parent path, not a trailing slash — which an
    // optional catch-all serves, and a required one does not.
    expect(buildRoute("/docs/:path*?", { path: [] })).toBe("/docs");
    expect(buildRoute("/:path*?", { path: [] })).toBe("/");
  });

  it("encodes each segment, so a value cannot become two", () => {
    // The failure this prevents: a slug with a slash in it silently becoming
    // an extra path segment, and matching a different route.
    expect(buildRoute("/posts/:slug", { slug: "a/b" })).toBe("/posts/a%2Fb");
    expect(buildRoute("/posts/:slug", { slug: "ünïcödé" })).toBe(
      `/posts/${encodeURIComponent("ünïcödé")}`,
    );
    expect(buildRoute("/posts/:slug", { slug: "a b" })).toBe("/posts/a%20b");
  });
});

describe("a built URL matches the route it was built from", () => {
  // The one property that makes the builder worth having rather than a second
  // implementation of the pattern grammar to keep in step by hand.
  const routes = [
    record("/"),
    record("/about"),
    record("/posts/:slug"),
    record("/users/:id/edit"),
    record("/docs/:path*"),
    record("/guide/:path*?"),
  ];

  it("round-trips every kind of segment", () => {
    for (const [pattern, params] of [
      ["/", {}],
      ["/about", {}],
      ["/posts/:slug", { slug: "hello-world" }],
      ["/users/:id/edit", { id: "42" }],
      ["/docs/:path*", { path: ["guide", "routing"] }],
      ["/guide/:path*?", { path: [] }],
      ["/guide/:path*?", { path: ["one"] }],
      ["/guide/:path*?", { path: ["one", "two", "three"] }],
    ]) {
      const url = buildRoute(pattern, params);
      const matched = matchRoute(routes, url);
      expect(matched?.route.path).toBe(pattern);
      expect(matched?.params).toEqual(params);
    }
  });

  it("round-trips a value that had to be encoded", () => {
    // `a/b` is one segment on the way out and one parameter on the way back,
    // which is the whole point of encoding it.
    const url = buildRoute("/posts/:slug", { slug: "a/b" });
    const matched = matchRoute(routes, url);
    expect(matched?.route.path).toBe("/posts/:slug");
    expect(matched?.params).toEqual({ slug: "a/b" });
  });
});

describe("a parameter that is not what the route takes is refused", () => {
  // The generated `route` types all of this, so a project written in Flow
  // cannot get here. A value out of JSON, or from a module that opted out of
  // Flow, can — and a link to `/posts/undefined` is worse than an error.
  it("refuses a missing parameter by name", () => {
    expect(() => buildRoute("/posts/:slug", {})).toThrow(/:slug/);
    expect(() => buildRoute("/posts/:slug", {})).toThrow(/nothing/);
  });

  it("refuses a single segment where a catch-all belongs", () => {
    expect(() => buildRoute("/docs/:path*", { path: "guide" })).toThrow(/:path\*/);
    expect(() => buildRoute("/docs/:path*?", { path: "guide" })).toThrow(/:path\*\?/);
  });

  it("refuses an empty list for a catch-all that needs a segment", () => {
    // `/docs` is not a URL `/docs/:path*` serves, so the link would 404; the
    // message names the spelling that would serve it.
    expect(() => buildRoute("/docs/:path*", { path: [] })).toThrow(/\[\[\.\.\.path\]\]/);
  });

  it("refuses an array where a single segment belongs", () => {
    expect(() => buildRoute("/posts/:slug", { slug: ["a", "b"] })).toThrow(/:slug/);
  });
});

describe("zero, one and many segments", () => {
  // #1361: `[...slug]` is one or more segments and `[[...slug]]` zero or more.
  // The matcher used to let `[...slug]` take none, where it outranked the page
  // at its parent path and answered `/docs` in the page's place.
  it("matches an optional catch-all for none, one and many", () => {
    const routes = [record("/docs/:slug*?")];
    expect(matchRoute(routes, "/docs")?.params).toEqual({ slug: [] });
    expect(matchRoute(routes, "/docs/")?.params).toEqual({ slug: [] });
    expect(matchRoute(routes, "/docs/a")?.params).toEqual({ slug: ["a"] });
    expect(matchRoute(routes, "/docs/a/b/c")?.params).toEqual({ slug: ["a", "b", "c"] });
    expect(matchRoute(routes, "/")).toBe(null);
    expect(matchRoute(routes, "/documents")).toBe(null);
  });

  it("matches an optional catch-all at the router root for every path", () => {
    const routes = [record("/:slug*?")];
    expect(matchRoute(routes, "/")?.params).toEqual({ slug: [] });
    expect(matchRoute(routes, "/a/b")?.params).toEqual({ slug: ["a", "b"] });
  });

  it("does not match a required catch-all for none", () => {
    const routes = [record("/docs/:slug*")];
    expect(matchRoute(routes, "/docs")).toBe(null);
    expect(matchRoute(routes, "/docs/a")?.params).toEqual({ slug: ["a"] });
    expect(matchRoute(routes, "/docs/a/b")?.params).toEqual({ slug: ["a", "b"] });
  });

  it("leaves the parent path to its own page beside a required catch-all", () => {
    const routes = [record("/docs"), record("/docs/:slug*")];
    expect(matchRoute(routes, "/docs")?.route.path).toBe("/docs");
    expect(matchRoute(routes, "/docs/a")?.route.path).toBe("/docs/:slug*");
  });

  it("ranks an optional catch-all below everything else that matches", () => {
    const routes = [record("/docs/:slug*?"), record("/docs/:id"), record("/docs/intro")];
    expect(matchRoute(routes, "/docs/intro")?.route.path).toBe("/docs/intro");
    expect(matchRoute(routes, "/docs/other")?.route.path).toBe("/docs/:id");
    expect(matchRoute(routes, "/docs/a/b")?.route.path).toBe("/docs/:slug*?");
    expect(matchRoute(routes, "/docs")?.route.path).toBe("/docs/:slug*?");
  });
});
