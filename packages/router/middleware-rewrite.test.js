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

// An intercepted navigation's payload renders two pages: the one its URL names,
// in a slot, and the one `uf-intercepted-from` names, underneath. The runner
// asks the guards of both, and a page underneath that its guards would not
// serve is taken off the request rather than rendered.
describe("a payload rendered over another page", () => {
  const FROM = "uf-intercepted-from";
  const signedIn = (request: Request) => request.headers.get("cookie") === "session=1";

  const feedGuarded = () =>
    hosted(
      createMiddlewareRunner({
        middleware: [
          record("/feed", {
            default: (request: Request) =>
              signedIn(request) ? undefined : new Response("sign in", { status: 401 }),
          }),
        ],
      }),
    );

  const intercepted = (headers: { [string]: string }) =>
    get("/photo/1/__uf.flight", { headers: { [FROM]: "/feed", ...headers } });

  const carriedFrom = (answer: Response | Request | null, request: Request): string | null => {
    const carried = answer instanceof Request ? answer : answer == null ? request : null;
    return carried == null ? "answered" : carried.headers.get(FROM);
  };

  it("keeps the page underneath when its guards admit the request", async () => {
    const request = intercepted({ cookie: "session=1" });
    const answer = await feedGuarded()(request);
    expect(carriedFrom(answer, request)).toBe("/feed");
  });

  it("drops the page underneath when its guards would not serve it", async () => {
    const request = intercepted({});
    const answer = await feedGuarded()(request);
    // Not the guard's 401: the URL asked for is not one it covers.
    expect(answer instanceof Request).toBe(true);
    expect(carriedFrom(answer, request)).toBe(null);
    expect(answer instanceof Request ? answer.url : null).toBe(
      "http://localhost/photo/1/__uf.flight",
    );
  });

  it("asks a guard that already ran for the URL again, about the page underneath", async () => {
    const asked = [];
    const run = hosted(
      createMiddlewareRunner({
        middleware: [
          record("/", {
            default: (request: Request) => {
              const { pathname } = new URL(request.url);
              asked.push(pathname);
              return pathname.startsWith("/feed") && !signedIn(request)
                ? new Response(null, { status: 401 })
                : undefined;
            },
          }),
        ],
      }),
    );
    const request = intercepted({});
    const answer = await run(request);
    expect(asked).toEqual(["/photo/1", "/feed"]);
    expect(carriedFrom(answer, request)).toBe(null);
  });

  it("treats a rewrite of the page underneath as not serving it", async () => {
    const run = hosted(
      createMiddlewareRunner({
        middleware: [record("/feed", { default: () => rewrite("/welcome") })],
      }),
    );
    const request = intercepted({});
    expect(carriedFrom(await run(request), request)).toBe(null);
  });

  it("drops a header that does not name a path on this origin", async () => {
    const run = hosted(
      createMiddlewareRunner({ middleware: [record("/", { default: () => {} })] }),
    );
    for (const value of ["//elsewhere.example/feed", "https://elsewhere.example/feed", "feed"]) {
      const request = get("/photo/1/__uf.flight", { headers: { [FROM]: value } });
      expect(carriedFrom(await run(request), request)).toBe(null);
    }
  });

  it("leaves a document request's header alone, which nothing renders from", async () => {
    const request = get("/photo/1", { headers: { [FROM]: "/feed" } });
    expect(await feedGuarded()(request)).toBe(null);
  });
});
