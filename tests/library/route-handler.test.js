// @flow
//
// `@uniflowed/router/handler`.
//
// The dispatcher is where every decision about a route handler is made: which
// path wins, which method runs, what happens when neither does. It takes a
// table and a `Request` and returns a `Response`, so all of that is testable
// without a server, a port or a build — which is also why it is a separate
// module rather than something inside the dev server's middleware.

import { describe, expect, it } from "@uniflowed/test";
import { createDispatcher } from "@uniflowed/router/handler";
import { beginRequest } from "@uniflowed/router/server";
import { cookies, headers } from "@uniflowed/server";

/** A table entry whose module is given inline. */
const record = (path, module) => ({
  path,
  params: [],
  file: `${path}/_uf.route.js`,
  load: async () => module,
});

const get = (url, init) => new Request(`http://localhost${url}`, init);

/**
 * The dispatcher, as a host calls it: inside a request the host owns and
 * settles.
 *
 * The same scaffolding `middleware.test.js` has, and for the same reason: the
 * dispatcher no longer builds a context of its own, so it does not run outside
 * one. What the lifecycle is for is `request-lifecycle.test.js`.
 */
const hosted = (dispatch) => async (request) => {
  const { run, settle } = beginRequest(request);
  try {
    return await run(() => dispatch(request));
  } finally {
    await settle();
  }
};

describe("matching", () => {
  it("answers a literal path", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [record("/api/health", { GET: () => new Response("ok") })],
      }),
    );

    const response = await dispatch(get("/api/health"));
    expect(response?.status).toBe(200);
    expect(await response?.text()).toBe("ok");
  });

  it("declines a path it does not have, so the caller can render a page", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [record("/api/health", { GET: () => new Response("ok") })],
      }),
    );

    // `null` rather than a 404: `/about` is a page, and the dispatcher saying
    // nothing is how it gets out of the way.
    expect(await dispatch(get("/about"))).toBe(null);
  });

  it("passes the path parameters", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/api/users/:id", {
            GET: (request, context) => Response.json({ id: context.params.id }),
          }),
        ],
      }),
    );

    const response = await dispatch(get("/api/users/42"));
    expect(await response?.json()).toEqual({ id: "42" });
  });

  it("gives a catch-all the rest of the path", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/files/:path*", {
            GET: (request, context) => Response.json(context.params.path),
          }),
        ],
      }),
    );

    expect(await (await dispatch(get("/files/a/b/c")))?.json()).toEqual(["a", "b", "c"]);
    // Zero segments as well as many: `/files` is the collection.
    expect(await (await dispatch(get("/files")))?.json()).toEqual([]);
  });

  it("prefers the more specific path", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/api/users/:id", { GET: () => new Response("by id") }),
          record("/api/users/new", { GET: () => new Response("new") }),
          record("/api/:rest*", { GET: () => new Response("catch all") }),
        ],
      }),
    );

    // A literal beats a parameter and a parameter beats a catch-all, whatever
    // order the table happens to be in.
    expect(await (await dispatch(get("/api/users/new")))?.text()).toBe("new");
    expect(await (await dispatch(get("/api/users/42")))?.text()).toBe("by id");
    expect(await (await dispatch(get("/api/anything/else")))?.text()).toBe("catch all");
  });

  it("does not match a longer path against a shorter route", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [record("/api/users", { GET: () => new Response("list") })],
      }),
    );
    expect(await dispatch(get("/api/users/42"))).toBe(null);
  });

  it("hands the query string over parsed", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/api/search", {
            GET: (request, context) => new Response(context.searchParams.get("q") ?? ""),
          }),
        ],
      }),
    );
    expect(await (await dispatch(get("/api/search?q=flow")))?.text()).toBe("flow");
  });
});

describe("methods", () => {
  const table = () =>
    hosted(
      createDispatcher({
        handlers: [
          record("/api/thing", {
            GET: () => new Response("read"),
            POST: async (request) => Response.json(await request.json(), { status: 201 }),
            helper: () => new Response("not a method"),
          }),
        ],
      }),
    );

  it("routes each method to its own export", async () => {
    expect(await (await table()(get("/api/thing")))?.text()).toBe("read");

    const created = await table()(
      get("/api/thing", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: "ada" }),
      }),
    );
    expect(created?.status).toBe(201);
    expect(await created?.json()).toEqual({ name: "ada" });
  });

  it("answers 405 with Allow when the path matches and the method does not", async () => {
    const response = await table()(get("/api/thing", { method: "DELETE" }));
    expect(response?.status).toBe(405);
    // The header is what lets a client tell "you may not do that here" from
    // "there is nothing here", and the specification requires it.
    expect(response?.headers.get("allow")).toBe("GET, HEAD, POST");
  });

  it("does not treat an ordinary export as a method", async () => {
    // `helper` is exported by that module. Treating every export as a method
    // would answer requests with it.
    const response = await table()(get("/api/thing", { method: "HELPER" }));
    expect(response?.status).toBe(405);
  });

  it("does not answer a verb that is not on the list, however the module spells it", async () => {
    // The list was closed in name only: `pick` looked the method up on the
    // module, so an *upper-case* export answered a request named after it, and
    // `HELPER` above only failed because the export is lower-case. A module
    // that happens to export `PURGE` is not a module that speaks `PURGE`.
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/api/thing", {
            GET: () => new Response("read"),
            PURGE: () => new Response("purged"),
          }),
        ],
      }),
    );

    const response = await dispatch(get("/api/thing", { method: "PURGE" }));
    expect(response?.status).toBe(405);
    expect(response?.headers.get("allow")).toBe("GET, HEAD");
  });

  it("answers a QUERY, which is the GET whose parameters did not fit in a URL", async () => {
    // Safe, idempotent, and carrying a body — the thing a search has been
    // faking with a `POST` for twenty years. The dispatcher treats it like any
    // other verb, which is the whole of what it should do; what refuses it is
    // everything between the client and here, and `@uniflowed/router/handler`
    // says so at length.
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/api/search", {
            QUERY: async (request) => Response.json({ asked: await request.json() }),
          }),
        ],
      }),
    );

    const response = await dispatch(
      get("/api/search", {
        method: "QUERY",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ filters: ["a", "b"] }),
      }),
    );

    expect(response?.status).toBe(200);
    expect(await response?.json()).toEqual({ asked: { filters: ["a", "b"] } });
  });

  it("names QUERY in Allow, so a client can tell it is offered", async () => {
    // A client that has to guess whether the origin speaks QUERY is a client
    // that sends a POST instead, which is the reason the method is not already
    // everywhere.
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/api/search", {
            GET: () => new Response("read"),
            QUERY: () => new Response("searched"),
          }),
        ],
      }),
    );

    const response = await dispatch(get("/api/search", { method: "DELETE" }));
    expect(response?.headers.get("allow")).toBe("GET, HEAD, QUERY");
  });

  it("answers HEAD with GET, minus the body", async () => {
    const response = await table()(get("/api/thing", { method: "HEAD" }));
    expect(response?.status).toBe(200);
    expect(await response?.text()).toBe("");
  });

  it("lets a module answer HEAD itself", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/api/thing", {
            GET: () => new Response("body"),
            HEAD: () => new Response(null, { status: 204 }),
          }),
        ],
      }),
    );
    expect((await dispatch(get("/api/thing", { method: "HEAD" })))?.status).toBe(204);
  });

  it("takes the method case-insensitively", async () => {
    const response = await table()(get("/api/thing", { method: "get" }));
    expect(await response?.text()).toBe("read");
  });
});

describe("errors", () => {
  it("lets a handler's error out rather than turning it into a 500", async () => {
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          record("/api/broken", {
            GET: () => {
              throw new Error("bug in the handler");
            },
          }),
        ],
      }),
    );

    // Swallowing it would hide a bug the host's own error reporting should
    // see, and the handler cannot tell the difference between a 500 it meant
    // and one it caused.
    await expect(dispatch(get("/api/broken"))).rejects.toThrow("bug in the handler");
  });

  it("loads a module only when its path is asked for", async () => {
    let loaded = 0;
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          {
            path: "/api/lazy",
            params: [],
            file: "app/api/lazy/_uf.route.js",
            load: async () => {
              loaded += 1;
              return { GET: () => new Response("ok") };
            },
          },
        ],
      }),
    );

    await dispatch(get("/elsewhere"));
    expect(loaded).toBe(0);
    await dispatch(get("/api/lazy"));
    expect(loaded).toBe(1);
  });
});

describe("the request a handler is inside", () => {
  it("lets a handler read the request's headers and cookies with no argument", async () => {
    // The host establishes the context; `headers()` and `cookies()` take
    // nothing and answer about the request being handled.
    const dispatch = hosted(
      createDispatcher({
        handlers: [
          {
            path: "/api/who",
            params: [],
            file: "app/api/who/_uf.route.js",
            load: async () => ({
              GET: () =>
                Response.json({
                  agent: headers().get("x-agent"),
                  session: cookies().get("session"),
                }),
            }),
          },
        ],
      }),
    );

    const response = await dispatch(
      new Request("https://uniflowed.dev/api/who", {
        headers: { "x-agent": "uf", cookie: "session=abc" },
      }),
    );

    expect(await response?.json()).toEqual({ agent: "uf", session: "abc" });
  });

  it("refuses to run outside a request, naming what establishes one", async () => {
    const dispatch = createDispatcher({
      handlers: [record("/api/thing", { GET: () => new Response("ok") })],
    });

    // The same refusal the middleware runner makes, and for the same reason:
    // the failure belongs to whoever wired the host, so it says so there
    // rather than surfacing as a confused `cookies()` further in.
    await expect(dispatch(get("/api/thing"))).rejects.toThrow(
      "dispatch() was called outside a request",
    );
  });
});
