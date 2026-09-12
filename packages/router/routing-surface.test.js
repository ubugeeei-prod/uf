// @flow
//
// The React-free router surface RSC/server code can import.

import fs from "node:fs";

import {
  RedirectError,
  buildRoute,
  matchRoute,
  redirect,
  routeErrorStatus,
} from "@uniflowed/router/routing";
import { describe, expect, it } from "@uniflowed/test";

function source(relative: string): string {
  return fs.readFileSync(new URL(relative, import.meta.url), "utf8");
}

describe("@uniflowed/router/routing", () => {
  it("is the route helper surface without the React runtime", () => {
    const joined = source("./routing.js") + "\n" + source("./internal/routing.js");

    expect(joined).not.toContain('from "react"');
    expect(joined).not.toContain('from "react-dom"');
    expect(joined).not.toContain("@uniflowed/hooks");
    expect(joined).not.toContain("./internal/runtime.js");
    expect(joined).not.toContain("./runtime.js");
  });

  it("matches, builds and redirects through the pure subpath", () => {
    const routes = [
      { path: "/", params: [], mdx: false, file: "app/$page.js", layouts: [] },
      {
        path: "/posts/:slug",
        params: [],
        mdx: false,
        file: "app/posts/[slug]/$page.js",
        layouts: [],
      },
    ];

    const href = buildRoute("/posts/:slug", { slug: "hello-world" });
    const matched = matchRoute(routes, href);
    expect(href).toBe("/posts/hello-world");
    expect(matched?.route.path).toBe("/posts/:slug");
    expect(matched?.params).toEqual({ slug: "hello-world" });

    let thrown: ?RedirectError = null;
    try {
      redirect("/login");
    } catch (error) {
      if (error instanceof RedirectError) {
        thrown = error;
      }
    }
    expect(thrown instanceof RedirectError).toBe(true);
    expect(thrown?.to).toBe("/login");
    expect(routeErrorStatus({ kind: "forbidden" })).toBe(403);
  });
});
