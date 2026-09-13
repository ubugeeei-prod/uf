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
  summarizeResolvedRoute,
} from "@uniflowed/router/routing";
import { describe, expect, it } from "@uniflowed/test";

function source(relative: string): string {
  return fs.readFileSync(new URL(relative, import.meta.url), "utf8");
}

describe("@uniflowed/router/routing", () => {
  it("is the route helper surface without the React runtime", () => {
    const joined =
      source("./routing.js") +
      "\n" +
      source("./internal/routing.js") +
      "\n" +
      source("./internal/boundary-data.js") +
      "\n" +
      source("./internal/resolved-summary.js");

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

  it("summarizes a resolved route without carrying render modules", () => {
    const page = { marker: "page module should not cross" };
    const layout = { marker: "layout module should not cross" };
    const fallback = { marker: "fallback module should not cross" };
    const error = { marker: "error module should not cross" };
    const resolved = {
      pathname: "/posts/hello",
      search: "?tab=comments",
      path: "/posts/:slug",
      params: { slug: "hello" },
      searchParams: { tab: "comments" },
      page,
      layouts: [layout],
      data: { title: "Hello" },
      deferred: null,
      metadata: { title: "Hello" },
      viewTransition: "post",
      status: 200 as 200,
      error: null,
      errorBoundary: { module: error, above: 1 },
      loading: [{ above: 0, module: fallback }],
      templates: [{ above: 1, module: { marker: "template module should not cross" } }],
      slots: [
        {
          name: "team",
          above: 1,
          page,
          params: { member: "ada" },
          layouts: [layout],
          loading: [{ above: 1, module: fallback }],
          templates: [],
          errorBoundary: null,
          slots: [],
        },
      ],
    };

    const summary = summarizeResolvedRoute(resolved, "app/posts/$error.js");

    expect(summary).toMatchObject({
      pathname: "/posts/hello",
      path: "/posts/:slug",
      layoutCount: 1,
      data: { title: "Hello" },
      deferred: false,
      metadata: { title: "Hello" },
      viewTransition: "post",
      status: 200,
      error: null,
      errorBoundary: { above: 1, custom: true, rendered: true },
      loading: [{ id: "suspense:0", above: 0 }],
      templates: [{ above: 1 }],
      slots: [
        {
          name: "team",
          active: true,
          layoutCount: 1,
          loading: [{ id: "suspense:0", above: 1 }],
        },
      ],
      boundaries: [
        { id: "error:root", kind: "error", above: 0, source: "@uniflowed/router" },
        { id: "error:route", kind: "error", above: 1, source: "app/posts/$error.js" },
        { id: "suspense:0", kind: "suspense", above: 0, source: null },
      ],
    });
    expect(JSON.stringify(summary)).not.toContain("module should not cross");
  });

  it("keeps thrown errors out of the route summary", () => {
    const summary = summarizeResolvedRoute({
      pathname: "/broken",
      search: "",
      path: "*",
      params: {},
      searchParams: {},
      page: { marker: "error page module should not cross" },
      layouts: [],
      data: undefined,
      deferred: null,
      metadata: { title: "Something went wrong" },
      viewTransition: null,
      status: 500 as 500,
      error: { kind: "thrown", error: new Error("secret") },
      errorBoundary: { module: null, above: 0 },
      loading: [],
      templates: [],
      slots: [],
    });

    expect(summary.error).toEqual({ kind: "thrown" });
    expect(summary.errorBoundary.rendered).toBe(false);
    expect(JSON.stringify(summary)).not.toContain("secret");
  });
});
