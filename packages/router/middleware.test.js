// @flow
//
// `@uniflowed/router/middleware`.
//
// A middleware is the file an application puts an authorisation check in, so
// "it did not run" has to be a thing this suite would notice. It was not: the
// name was reserved in two languages, accumulated per directory and printed by
// `uf inspect`, and the generated route table dropped it — an unenforced auth
// check with no diagnostic anywhere. See ubugeeei-prod/uf#260.
//
// Like the dispatcher, the runner takes a table and a `Request` and returns a
// `Response`, so every decision it makes is testable without a server, a port
// or a build. Not without a *request*, though, and that is the one thing that
// changed: the host owns the context now, so every call below goes through
// `hosted` — see `request-lifecycle.test.js` for what that buys and
// ubugeeei-prod/uf#389 for what it cost to leave it here.

import { describe, expect, it } from "@uniflowed/test";
import { createMiddlewareRunner } from "@uniflowed/router/middleware";
import { beginRequest } from "@uniflowed/router/server";
import { cookies, headers } from "@uniflowed/server";

/** A table entry whose module is given inline. */
const record = (path, module) => ({
  path,
  file: `app${path === "/" ? "" : path}/$middleware.js`,
  load: async () => module,
});

const get = (url, init) => new Request(`http://localhost${url}`, init);

/**
 * The runner, as a host calls it: inside a request the host owns and settles.
 *
 * Every test below goes through this rather than calling the runner directly,
 * because that is now the only way it runs at all — the runner refuses outside
 * a request, and `cookies()` in a guard reads the host's context rather than
 * one the runner built for itself. What that is for is
 * `request-lifecycle.test.js`; here it is scaffolding, and the point of having
 * it in one line is that no test below has to think about it.
 */
const hosted = (runner) => async (request) => {
  const { run, settle } = beginRequest(request);
  try {
    return await run(() => runner(request));
  } finally {
    await settle();
  }
};

describe("matching", () => {
  it("runs for the path it guards", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/dashboard", { default: () => new Response("no", { status: 401 }) })],
      }),
    );

    const response = await run(get("/dashboard"));
    expect(response?.status).toBe(401);
  });

  it("runs for everything under the path it guards", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/dashboard", { default: () => new Response("no", { status: 401 }) })],
      }),
    );

    // The subtree, not the one path: a guard on `/dashboard` that only ran for
    // `/dashboard` itself would leave every page under it open.
    expect((await run(get("/dashboard/settings")))?.status).toBe(401);
    expect((await run(get("/dashboard/reports/2026/q1")))?.status).toBe(401);
  });

  it("runs for a path under it that matches no route at all", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/dashboard", { default: () => new Response("no", { status: 401 }) })],
      }),
    );

    // `/dashboard/typo` is a 404, and a 404 rendered without the guard having
    // run is how a per-route middleware array leaks: the router has no record
    // for it, so there would have been no array to read.
    expect((await run(get("/dashboard/typo")))?.status).toBe(401);
  });

  it("does not run for a sibling path", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/dashboard", { default: () => new Response("no", { status: 401 }) })],
      }),
    );

    // `null` rather than a response: the caller carries on to the page or the
    // handler, which is how a middleware gets out of the way.
    expect(await run(get("/about"))).toBe(null);
    // And a path that merely starts with the same characters is not under it.
    expect(await run(get("/dashboards"))).toBe(null);
  });

  it("guards the whole application from the root", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/", { default: () => new Response("no", { status: 401 }) })],
      }),
    );

    expect((await run(get("/")))?.status).toBe(401);
    expect((await run(get("/anything/at/all")))?.status).toBe(401);
  });

  it("captures the parameters of the directory it guards", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/:org", {
            default: (request, context) => Response.json({ org: context.params.org }),
          }),
        ],
      }),
    );

    // Which is what an authorisation check needs: the tenant is in the path.
    expect(await (await run(get("/acme/settings")))?.json()).toEqual({ org: "acme" });
  });

  it("hands the query string over parsed", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: (request, context) => new Response(context.searchParams.get("token") ?? ""),
          }),
        ],
      }),
    );

    expect(await (await run(get("/private?token=abc")))?.text()).toBe("abc");
  });
});

describe("composition", () => {
  it("runs root first, then the deeper guard", async () => {
    const order = [];
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/dashboard/admin", {
            default: () => {
              order.push("admin");
            },
          }),
          record("/", {
            default: () => {
              order.push("root");
            },
          }),
          record("/dashboard", {
            default: () => {
              order.push("dashboard");
            },
          }),
        ],
      }),
    );

    // Sorted by the runner, not by the order the table happens to be in: the
    // application-wide check runs before the one guarding a section of it.
    expect(await run(get("/dashboard/admin/users"))).toBe(null);
    expect(order).toEqual(["root", "dashboard", "admin"]);
  });

  it("stops at the first middleware that answers", async () => {
    let reached = false;
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", { default: () => new Response("no", { status: 403 }) }),
          record("/dashboard", {
            default: () => {
              reached = true;
            },
          }),
        ],
      }),
    );

    expect((await run(get("/dashboard")))?.status).toBe(403);
    // A rejected request must not go on running the checks below it.
    expect(reached).toBe(false);
  });

  it("continues when a middleware returns nothing", async () => {
    const seen = [];
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: (request) => {
              seen.push(new URL(request.url).pathname);
            },
          }),
        ],
      }),
    );

    // A logger is a middleware too, and one that had to answer could not be.
    expect(await run(get("/about"))).toBe(null);
    expect(seen).toEqual(["/about"]);
  });

  it("awaits an async middleware", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: async () => {
              await Promise.resolve();
              return new Response("no", { status: 401 });
            },
          }),
        ],
      }),
    );

    expect((await run(get("/")))?.status).toBe(401);
  });

  it("loads a module only when its path is asked for", async () => {
    let loaded = 0;
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          {
            path: "/dashboard",
            file: "app/dashboard/$middleware.js",
            load: async () => {
              loaded += 1;
              return { default: () => new Response("no", { status: 401 }) };
            },
          },
        ],
      }),
    );

    await run(get("/about"));
    expect(loaded).toBe(0);
    await run(get("/dashboard"));
    expect(loaded).toBe(1);
  });
});

describe("inside a request", () => {
  it("reads the request's cookies", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/dashboard", {
            default: () =>
              cookies().get("session") == null
                ? new Response("sign in", { status: 401 })
                : undefined,
          }),
        ],
      }),
    );

    // The check an application actually writes: `cookies()` takes no argument,
    // so it only works if the request the host began is the one this runs in.
    expect((await run(get("/dashboard")))?.status).toBe(401);
    expect(await run(get("/dashboard", { headers: { cookie: "session=abc" } }))).toBe(null);
  });

  it("reads the request's headers", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/api", {
            default: () =>
              headers().get("authorization") == null
                ? new Response(null, { status: 401 })
                : undefined,
          }),
        ],
      }),
    );

    expect((await run(get("/api/things")))?.status).toBe(401);
    expect(await run(get("/api/things", { headers: { authorization: "Bearer t" } }))).toBe(null);
  });

  it("refuses to run outside a request, naming what establishes one", async () => {
    const run = createMiddlewareRunner({
      middleware: [record("/", { default: () => undefined })],
    });

    // Not `cookies()` failing somewhere inside somebody's guard with a message
    // about static prerenders and client components — three places to look,
    // none of them the host that forgot. And not silence: an application with
    // no `cookies()` anywhere would otherwise run its whole request with no
    // context and lose every `after()` on it.
    await expect(run(get("/"))).rejects.toThrow("runMiddleware() was called outside a request");
  });
});

describe("errors", () => {
  it("says so when a middleware module exports no middleware", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/dashboard", { helper: () => new Response("not a middleware") })],
      }),
    );

    // The failure mode this whole module exists to stop: a file named
    // `$middleware.js` that the router quietly ignores.
    await expect(run(get("/dashboard"))).rejects.toThrow(
      "app/dashboard/$middleware.js is a middleware but exports no middleware function",
    );
  });

  it("takes the function from `middleware` when there is no default", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/dashboard", { middleware: () => new Response(null, { status: 401 }) }),
        ],
      }),
    );

    expect((await run(get("/dashboard")))?.status).toBe(401);
  });

  it("lets a middleware's error out rather than turning it into a 500", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/dashboard", {
            default: () => {
              throw new Error("bug in the middleware");
            },
          }),
        ],
      }),
    );

    // Same rule as a route handler: swallowing it would hide a bug from the
    // host's own error reporting, and — worse here — turn a guard that
    // crashed into a request that carried on.
    await expect(run(get("/dashboard"))).rejects.toThrow("bug in the middleware");
  });

  it("does nothing at all when an application has no middleware", async () => {
    const run = hosted(createMiddlewareRunner({ middleware: [] }));
    expect(await run(get("/"))).toBe(null);
  });

  it("cannot be skipped by anything the client sends", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/dashboard", { default: () => new Response(null, { status: 401 }) })],
      }),
    );

    // CVE-2025-29927 is the whole reason this test exists: sending
    // `x-middleware-subrequest` made Next skip middleware entirely, and every
    // authorisation check written in one stopped running. Which middleware
    // runs here is decided by the path and by nothing else, so a request that
    // names the header, or any other, is guarded exactly the same.
    for (const header of [
      { "x-middleware-subrequest": "middleware:middleware:middleware" },
      { "x-middleware-prefetch": "1" },
      { "x-invoke-path": "/" },
    ]) {
      expect((await run(get("/dashboard", { headers: header })))?.status).toBe(401);
    }
  });
});
