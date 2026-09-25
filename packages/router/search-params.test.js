// @flow
//
// Typed search params: a page exports a `@uniflowed/validator` schema as
// `searchParams`, and is given the schema's output instead of the string map.
// ubugeeei-prod/uf#1362.
//
// Four claims, a section each. The query is shaped the way the schema says —
// a repeated key is a list where the schema says list, and the last value
// where it does not. A string that spells a number, a boolean or a date is
// that value when the schema wants one. A query the schema refuses is a `400`
// the page's `$error.js` renders, not an exception. And a page with no schema
// is given exactly what it was given before, with the repeats available from
// `parseSearchAll` rather than lost. The type half — that the prop *is* the
// schema's output — is `tests/type-tests/search-params.js`, run at the end.

import path from "node:path";

import * as React from "@uniflowed/react";
import { resolveMatch, routerView } from "@uniflowed/router";
import type { PageModule, RouteError, RouteRecord, RouteTable } from "@uniflowed/router";
import { parseSearch, parseSearchAll } from "@uniflowed/router/routing";
import { createRenderer } from "@uniflowed/router/server";
import { describe, expect, it } from "@uniflowed/test";
import {
  array,
  boolean,
  date,
  literal,
  looseObject,
  number,
  object,
  optional,
  pipe,
  string,
  transform,
  union,
  withDefault,
  integer,
  min,
} from "@uniflowed/validator";
import type { Schema } from "@uniflowed/validator";

import { pageSearchParams } from "./internal/resolve.js";
import { everyMisuseIsReported } from "../../tests/library/type-tests.js";

/** A one-route table whose page exports `module`, under a root `$error.js`. */
function tableFor(module: PageModule): RouteTable {
  const route: RouteRecord = {
    path: "/search",
    params: [],
    mdx: false,
    file: "app/search/$page.js",
    page: () => Promise.resolve(module),
    layouts: [],
  };
  return {
    routes: [route],
    notFound: [],
    errors: [
      {
        path: "/",
        file: "app/$error.js",
        module: () => Promise.resolve({ default: SearchError }),
        layouts: [],
      },
    ],
  };
}

component SearchError(error: RouteError, reset: () => void) {
  return match (error) {
    {kind: "badRequest", issues: const issues} =>
      <p>
        bad query:{" "}
        {issues.map((issue) => `${(issue.path ?? []).join(".")} ${issue.code}`).join(", ")}
      </p>,
    _ => <p>something else</p>,
  };
}

/** What the page at `/search?<query>` is given as `searchParams`. */
async function given(schema: Schema<mixed, mixed>, query: string): Promise<mixed> {
  const resolved = await resolveMatch(tableFor({ searchParams: schema }), `/search?${query}`);
  expect(resolved.error).toBe(null);
  return pageSearchParams(resolved);
}

describe("parsing the query against the page's schema", () => {
  it("hands the page the schema's output", async () => {
    const schema = object({ q: string(), sort: optional(string()) });

    expect(await given(schema, "q=flow")).toEqual({ q: "flow" });
    expect(await given(schema, "q=flow&sort=new")).toEqual({ q: "flow", sort: "new" });
  });

  it("runs the schema's own steps, a transform included", async () => {
    const schema = object({
      q: pipe(
        string(),
        transform((value: string) => value.trim().toUpperCase()),
      ),
      limit: withDefault(number(), 20),
    });

    expect(await given(schema, "q=%20flow%20")).toEqual({ q: "FLOW", limit: 20 });
  });

  it("keeps a key the schema does not name only when the schema keeps it", async () => {
    expect(await given(object({ q: string() }), "q=a&utm=x")).toEqual({ q: "a" });
    expect(await given(looseObject({ q: string() }), "q=a&utm=x&utm=y")).toEqual({
      q: "a",
      utm: ["x", "y"],
    });
  });

  it("leaves the string map alone for a page that declares no schema", async () => {
    const resolved = await resolveMatch(tableFor({}), "/search?q=a&tag=x&tag=y");

    expect(pageSearchParams(resolved)).toEqual({ q: "a", tag: "y" });
    expect(resolved.searchParams).toEqual({ q: "a", tag: "y" });
    expect(resolved.parsedSearchParams).toBe(undefined);
  });
});

describe("coercing the strings a query is made of", () => {
  it("reads a number, a boolean and a date where the schema wants one", async () => {
    const schema = object({
      page: pipe(number(), integer(), min(1)),
      draft: boolean(),
      since: date(),
    });

    const value = await given(schema, "page=2&draft=true&since=2026-09-25");

    expect(value).toEqual({ page: 2, draft: true, since: new Date("2026-09-25") });
  });

  it("reads a literal and a union of literals as the value they spell", async () => {
    const schema = object({
      per: union([literal(10), literal(50)]),
      view: union([literal("all"), number()]),
    });

    expect(await given(schema, "per=50&view=all")).toEqual({ per: 50, view: "all" });
    expect(await given(schema, "per=10&view=3")).toEqual({ per: 10, view: 3 });
  });

  it("leaves a string the schema reads as a string, digits and all", async () => {
    expect(await given(object({ code: string() }), "code=007")).toEqual({ code: "007" });
  });

  it("does not read an empty value as zero", async () => {
    const resolved = await resolveMatch(
      tableFor({ searchParams: object({ page: number() }) }),
      "/search?page=",
    );

    expect(resolved.status).toBe(400);
  });
});

describe("a repeated key", () => {
  it("is every value, in order, where the schema says array", async () => {
    const schema = object({ tag: array(string()), page: optional(number()) });

    expect(await given(schema, "tag=a&tag=b&tag=c")).toEqual({ tag: ["a", "b", "c"] });
    // One occurrence is still a list, and none is an empty one.
    expect(await given(schema, "tag=a")).toEqual({ tag: ["a"] });
    expect(await given(schema, "page=1")).toEqual({ tag: [], page: 1 });
  });

  it("coerces each item of a list", async () => {
    expect(await given(object({ id: array(number()) }), "id=1&id=22")).toEqual({ id: [1, 22] });
  });

  it("is the last value where the schema says one, as the string map has it", async () => {
    expect(await given(object({ q: string() }), "q=first&q=last")).toEqual({ q: "last" });
  });

  it("is absent rather than empty for an optional list", async () => {
    expect(await given(object({ tag: optional(array(string())) }), "")).toEqual({});
  });

  it("is kept by `parseSearchAll` for a page with no schema", () => {
    expect(parseSearchAll("?tag=a&tag=b&q=x")).toEqual({ tag: ["a", "b"], q: ["x"] });
    // And `parseSearch` is unchanged: the last value.
    expect(parseSearch("?tag=a&tag=b")).toEqual({ tag: "b" });
    // A key a visitor chose is a key, not a way into `Object.prototype`.
    const hostile = parseSearchAll("?__proto__=x&constructor=y");
    expect(Object.getPrototypeOf(hostile)).toBe(Object.prototype);
    expect(Object.keys(hostile).sort()).toEqual(["__proto__", "constructor"]);
  });
});

describe("a query the schema refuses", () => {
  const schema = object({ page: pipe(number(), integer(), min(1)), tag: array(string()) });

  it("is a 400 route error carrying the issues, not an exception", async () => {
    let loaderRan = false;
    const resolved = await resolveMatch(
      tableFor({
        searchParams: schema,
        loader: () => {
          loaderRan = true;
          return null;
        },
      }),
      "/search?page=zero",
    );

    expect(resolved.status).toBe(400);
    expect(resolved.error?.kind).toBe("badRequest");
    const issues = resolved.error?.kind === "badRequest" ? resolved.error.issues : [];
    expect(issues.map((issue) => issue.path)).toEqual([["page"]]);
    // The page said it cannot answer this request, so nothing of it runs.
    expect(loaderRan).toBe(false);
  });

  it("renders the page's $error.js, which can match on it", async () => {
    const table = tableFor({ searchParams: schema, default: () => <p>results</p> });
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: table.routes,
      notFound: table.notFound,
      errors: table.errors,
    });

    const result = await prerender("/search?page=0&tag=a", {
      scripts: [],
      styles: [],
      preloads: [],
    });

    expect(result.status).toBe(400);
    // The boundary is the page's own, and it read the issue: which parameter,
    // and what about it was wrong.
    expect(result.html).toContain("bad query:");
    expect(result.html).toContain("page min");
    expect(result.html).not.toContain("results");
  });

  it("renders the page when the query fits", async () => {
    component Results(
      searchParams: { readonly page: number, readonly tag: $ReadOnlyArray<string> },
    ) {
      return (
        <p>
          page {searchParams.page + 1} of {searchParams.tag.join("+")}
        </p>
      );
    }
    const table = tableFor({ searchParams: schema, default: Results });
    const { prerender } = createRenderer({
      App: routerView("./app"),
      routes: table.routes,
      notFound: table.notFound,
      errors: table.errors,
    });

    const result = await prerender("/search?page=1&tag=a&tag=b", {
      scripts: [],
      styles: [],
      preloads: [],
    });

    expect(result.status).toBe(200);
    expect(result.html).toContain("page <!-- -->2<!-- --> of <!-- -->a+b");
  });
});

describe("the type a page is given", () => {
  it("is the schema's output, and a misuse of it is reported", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "search-params.js"),
      alongside: ["packages/router", "packages/validator"],
      atLeast: 2,
    });
  });
});
