// @flow
//
// `rewrite()` from a middleware: another route served at the address asked for.
//
// The runner hands a host back a `Request` for a rewrite, and the guarantees
// that make that safe are the ones asserted here: the destination's own
// middleware runs, nothing runs twice, and a payload request is the document it
// is for. `tests/library/deploy.test.js` and `crates/uf_cli/tests/vite.rs` ask
// the same thing of every front door.

import { describe, expect, it } from "@uniflowed/test";
import { createMiddlewareRunner, rewrite } from "@uniflowed/router/middleware";
import { beginRequest } from "@uniflowed/router/server";

const record = (path: string, module: mixed) => ({
  path,
  file: `app${path === "/" ? "" : path}/$middleware.js`,
  load: async () => module,
});

const get = (url: string, init?: mixed) => new Request(`http://localhost${url}`, init);

const hosted =
  (runner: (request: Request) => Promise<Response | Request | null>) =>
  async (request: Request): Promise<Response | Request | null> => {
    const { run, settle } = beginRequest(request);
    try {
      return await run(() => runner(request));
    } finally {
      await settle();
    }
  };

const urlOf = (answer: Response | Request | null): string | null =>
  answer instanceof Request ? answer.url : null;

describe("a rewrite from a middleware", () => {
  it("hands the host the same request at the destination", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: (request: Request) =>
              new URL(request.url).pathname === "/pricing" ? rewrite("/beta/pricing") : undefined,
          }),
        ],
      }),
    );

    expect(urlOf(await run(get("/pricing")))).toBe("http://localhost/beta/pricing");
    // And a path it does not rewrite carries on as it came.
    expect(await run(get("/about"))).toBe(null);
  });

  it("keeps the request's query unless the destination names one", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: (request: Request) => {
              const { pathname } = new URL(request.url);
              if (pathname === "/a") return rewrite("/b");
              if (pathname === "/c") return rewrite(new URL("/d?view=grid", request.url));
              return undefined;
            },
          }),
        ],
      }),
    );

    expect(urlOf(await run(get("/a?page=2")))).toBe("http://localhost/b?page=2");
    expect(urlOf(await run(get("/c?page=2")))).toBe("http://localhost/d?view=grid");
  });

  it("runs the destination's middleware, which may still refuse", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: (request: Request) =>
              new URL(request.url).pathname === "/staff" ? rewrite("/admin/panel") : undefined,
          }),
          record("/admin", { default: () => new Response("no", { status: 401 }) }),
        ],
      }),
    );

    // A rewrite into a guarded subtree is not a way around the guard.
    const answer = await run(get("/staff"));
    expect(answer instanceof Response ? answer.status : null).toBe(401);
  });

  it("runs no middleware twice for one request", async () => {
    let calls = 0;
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: () => {
              calls += 1;
              // Unconditional: a runner that started the chain again with this
              // one in it would never stop.
              return rewrite("/elsewhere");
            },
          }),
        ],
      }),
    );

    expect(urlOf(await run(get("/anything")))).toBe("http://localhost/elsewhere");
    expect(calls).toBe(1);
  });

  it("keeps the method and the body", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/api/v1", { default: () => rewrite("/api/v2/items") })],
      }),
    );

    const answer = await run(get("/api/v1/items", { method: "POST", body: "uf" }));
    expect(answer instanceof Request ? answer.method : null).toBe("POST");
    expect(answer instanceof Request ? await answer.text() : null).toBe("uf");
  });

  it("refuses another origin, naming the middleware that asked for it", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/", { default: () => rewrite("https://elsewhere.example/") })],
      }),
    );

    let message = "";
    try {
      await run(get("/"));
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    }
    expect(message).toContain("app/$middleware.js");
    expect(message).toContain("another origin");
  });
});

describe("a payload request", () => {
  it("is matched as, and handed to a middleware as, the document it is for", async () => {
    const seen = [];
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/pricing", {
            default: (request: Request) => {
              seen.push(new URL(request.url).pathname);
              return undefined;
            },
          }),
        ],
      }),
    );

    expect(await run(get("/pricing/__uf.flight"))).toBe(null);
    expect(seen).toEqual(["/pricing"]);
  });

  it("is rewritten into the destination's payload", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: (request: Request) =>
              new URL(request.url).pathname === "/pricing" ? rewrite("/beta/pricing") : undefined,
          }),
        ],
      }),
    );

    expect(urlOf(await run(get("/pricing/__uf.flight?plan=pro")))).toBe(
      "http://localhost/beta/pricing/__uf.flight?plan=pro",
    );
  });
});
