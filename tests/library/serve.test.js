// @flow
//
// What a built uf application answers, and in what order.
//
// `uf preview` and `uf start` are two sockets in front of one handler, and
// every decision either of them makes is made here: whether a route handler or
// a file wins, what a `POST` to a path with no handler is, and what a request
// that escapes the output directory gets. All of it takes a `Request` and
// returns a `Response`, so none of it needs a port, a build or a browser —
// which is the same reason `createDispatcher` is its own module and tested
// the same way in `route-handler.test.js`.
//
// The end-to-end half — build the application, start both servers, ask them —
// is `crates/uf_cli/tests/vite.rs`, which is the only place a socket is bound.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";

// Not a package export, deliberately. `internal/serve.js` is the seam a deploy
// adapter will need, and naming it in `exports` before one exists would be
// promising an interface nothing has used yet.
import {
  createApplicationHandler,
  createServeHandler,
  createStaticHandler,
} from "../../packages/vite/internal/serve.js";

const assets = { scripts: ["/assets/client.js"], styles: [], preloads: [] };

/** A server bundle, as `loadBuild` would have imported one. */
function entryWith(options: {
  guard?: (request: Request) => Promise<Response | null> | Response | null,
  handler?: (request: Request) => Promise<Response | null> | Response | null,
  render?: (url: string) => { status: number, html: string, headers?: { [string]: string } },
}) {
  return {
    routes: [],
    handlers: [],
    middleware: [],
    notFound: [],
    errors: [],
    runMiddleware: async (request: Request) => (options.guard ? options.guard(request) : null),
    dispatch: async (request: Request) => (options.handler ? options.handler(request) : null),
    render: async (url: string) =>
      options.render ? options.render(url) : { status: 200, html: `<!doctype html><p>${url}</p>` },
  };
}

const request = (url: string, init?: mixed) => new Request(`http://localhost${url}`, init);

/** A directory holding `files`, removed when the process exits. */
function directoryWith(files: { [string]: string }): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-serve-"));
  for (const [name, contents] of Object.entries(files)) {
    const file = path.join(root, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, String(contents));
  }
  return root;
}

describe("the application handler", () => {
  it("lets a route handler answer a POST", async () => {
    const handle = createApplicationHandler({
      entry: entryWith({ handler: () => Response.json({ ok: true }) }),
      assets,
    });

    const response = await handle(request("/api/health", { method: "POST", body: "{}" }));
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ ok: true });
  });

  it("runs the middleware above the dispatcher and the renderer", async () => {
    let dispatched = false;
    let rendered = false;
    const handle = createApplicationHandler({
      entry: entryWith({
        guard: () => new Response(null, { status: 302, headers: { location: "/sign-in" } }),
        handler: () => {
          dispatched = true;
          return null;
        },
        render: () => {
          rendered = true;
          return { status: 200, html: "<p>secrets</p>" };
        },
      }),
      assets,
    });

    // A build has to guard what a dev server guards. `uf dev` runs the
    // middleware before it resolves anything, and a served build that ran it
    // afterwards — or not at all — would be an application whose auth check
    // passes in development and is absent in production.
    const response = await handle(request("/dashboard"));
    expect(response.status).toBe(302);
    expect(response.headers.get("location")).toBe("/sign-in");
    expect(dispatched).toBe(false);
    expect(rendered).toBe(false);
  });

  it("carries on to the route when every middleware declined", async () => {
    const handle = createApplicationHandler({ entry: entryWith({ guard: () => null }), assets });

    const response = await handle(request("/dashboard"));
    expect(response.status).toBe(200);
    expect(await response.text()).toContain("/dashboard");
  });

  it("renders a page the handlers declined", async () => {
    const handle = createApplicationHandler({ entry: entryWith({}), assets });

    const response = await handle(request("/posts/hello?draft=1"));
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toBe("text/html; charset=utf-8");
    // The query string reaches the renderer: a route's `searchParams` come
    // from the URL, and trimming it here would have silently emptied them.
    expect(await response.text()).toContain("/posts/hello?draft=1");
  });

  it("answers a POST no handler claimed with a 404, not a page", async () => {
    let rendered = false;
    const handle = createApplicationHandler({
      entry: entryWith({
        render: () => {
          rendered = true;
          return { status: 200, html: "<p>home</p>" };
        },
      }),
      assets,
    });

    const response = await handle(request("/api/missing", { method: "POST", body: "{}" }));
    // A page cannot answer a POST, and rendering one would turn a missing
    // handler into a 200 that looks like the application working.
    expect(response.status).toBe(404);
    expect(rendered).toBe(false);
  });

  it("keeps the status and headers the renderer chose", async () => {
    const handle = createApplicationHandler({
      entry: entryWith({
        render: () => ({ status: 307, html: "<p>go</p>", headers: { location: "/elsewhere" } }),
      }),
      assets,
    });

    const response = await handle(request("/old"));
    expect(response.status).toBe(307);
    expect(response.headers.get("location")).toBe("/elsewhere");
  });

  it("gives a HEAD the status and no body", async () => {
    const handle = createApplicationHandler({ entry: entryWith({}), assets });

    const response = await handle(request("/", { method: "HEAD" }));
    expect(response.status).toBe(200);
    expect(await response.text()).toBe("");
  });
});

describe("the static handler", () => {
  it("serves a prerendered document for a directory path", async () => {
    const root = directoryWith({ "guide/index.html": "<p>guide</p>" });
    const serveStatic = createStaticHandler({ root });

    for (const url of ["/guide", "/guide/"]) {
      const response = await serveStatic(request(url));
      // Neither spelling is the one a person types, so both have to be the
      // same document.
      expect(await response?.text()).toBe("<p>guide</p>");
      expect(response?.headers.get("content-type")).toBe("text/html; charset=utf-8");
    }
  });

  it("declines a path that is not a file, so the application can render it", async () => {
    const serveStatic = createStaticHandler({ root: directoryWith({}) });
    expect(await serveStatic(request("/posts/hello"))).toBe(null);
  });

  it("refuses a path that escapes the output directory", async () => {
    const root = directoryWith({ "index.html": "<p>home</p>" });
    fs.writeFileSync(path.join(root, "..", "uf-serve-secret.txt"), "secret");
    const serveStatic = createStaticHandler({ root });

    // `new URL` normalises a literal `..` away, so the interesting spelling is
    // the encoded one it leaves alone: the check has to happen after the
    // decode and after the resolve, never against the raw request string.
    // docs/security.md, rule 2.
    for (const url of ["/%2e%2e/uf-serve-secret.txt", "/..%2fuf-serve-secret.txt"]) {
      expect(await serveStatic(request(url))).toBe(null);
    }
  });

  it("refuses a percent escape that decodes to nothing", async () => {
    const serveStatic = createStaticHandler({ root: directoryWith({ "index.html": "<p>x</p>" }) });
    expect(await serveStatic(request("/%zz"))).toBe(null);
    expect(await serveStatic(request("/%00index.html"))).toBe(null);
  });

  it("ignores a POST, because a file cannot be the answer to one", async () => {
    const root = directoryWith({ "feed.xml": "<rss/>" });
    const serveStatic = createStaticHandler({ root });

    expect(await serveStatic(request("/feed.xml"))).not.toBe(null);
    expect(await serveStatic(request("/feed.xml", { method: "POST", body: "" }))).toBe(null);
  });

  it("names a type it knows and downloads one it does not", async () => {
    const root = directoryWith({ "a.css": "body{}", "b.bin": "x" });
    const serveStatic = createStaticHandler({ root });

    expect((await serveStatic(request("/a.css")))?.headers.get("content-type")).toBe(
      "text/css; charset=utf-8",
    );
    // A type uf cannot name is one a browser must not execute.
    expect((await serveStatic(request("/b.bin")))?.headers.get("content-type")).toBe(
      "application/octet-stream",
    );
  });
});

describe("the two together", () => {
  it("serves a file before rendering, so a prerendered page stays prerendered", async () => {
    const distDir = directoryWith({ "guide/index.html": "<p>prerendered</p>" });
    const handle = createServeHandler({
      entry: entryWith({ render: () => ({ status: 200, html: "<p>rendered</p>" }) }),
      assets,
      distDir,
    });

    expect(await (await handle(request("/guide/"))).text()).toBe("<p>prerendered</p>");
  });

  it("renders a route the build never wrote a file for", async () => {
    const distDir = directoryWith({ "index.html": "<p>home</p>" });
    const handle = createServeHandler({
      entry: entryWith({ render: (url) => ({ status: 200, html: `<p>rendered ${url}</p>` }) }),
      assets,
      distDir,
    });

    // The whole reason `uf start` exists: `/posts/[slug]` with no
    // `generateStaticParams` is skipped by the prerender, so a build with only
    // static output has nothing to answer this with.
    expect(await (await handle(request("/posts/hello"))).text()).toBe(
      "<p>rendered /posts/hello</p>",
    );
  });

  it("answers an unrouted path with the renderer's 404, not with another page", async () => {
    const distDir = directoryWith({ "index.html": "<p>home</p>" });
    const handle = createServeHandler({
      entry: entryWith({ render: () => ({ status: 404, html: "<p>not found</p>" }) }),
      assets,
      distDir,
    });

    const response = await handle(request("/definitely-not-a-page/"));
    expect(response.status).toBe(404);
    expect(await response.text()).toBe("<p>not found</p>");
  });
});
