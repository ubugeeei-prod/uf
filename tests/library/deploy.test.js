// @flow
//
// What `uf build --adapter node` writes, at the seam it writes it against.
//
// The directory that command produces is three things: `handler.js`, which is
// [`createFetchHandler`] over the project's server bundle; `server.js`, which
// is [`serve`] over that handler and a `static/` directory; and the copy of
// the build. This file is about the two functions, because they are what a
// second adapter reuses — `crates/uf_cli/tests/vite.rs` is where the directory
// itself is built, copied somewhere with no `node_modules` above it, and
// asked.
//
// # The invariant that matters most
//
// Not "the handler answers", which `serve.test.js` already establishes for the
// same code through `uf preview` and `uf start`. It is that a *deployment* and
// `uf start` answer **the same**. Those two used to be different
// implementations of one order — `@uniflowed/vite`'s `internal/serve.js` for
// the two servers, and whatever an adapter would have written for itself — and
// the case where copies drift is never the ordinary request. It is the
// collision: a path that has both a prerendered file and a route handler.
// `uf preview` cannot choose, because Vite's preview server runs its own file
// middleware before anything uf mounts behind it, so the file wins there and
// therefore has to win everywhere.
//
// `the two front doors give one answer` below drives both of them over one
// fixture and compares. It is the test that fails if a future adapter decides
// to be cleverer than `uf preview` is allowed to be.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";

import { createFetchHandler } from "@uniflowed/server/fetch";
import { createServeHandler, createStaticHandler } from "@uniflowed/server/node";

// The other front door, for the comparison. Reached by path rather than by
// specifier because `@uniflowed/vite` deliberately does not export it: it is
// the bundler's copy of a question that is now answered in `@uniflowed/server`.
import { createServeHandler as createViteServeHandler } from "../../packages/vite/internal/serve.js";

const assets = { scripts: ["/assets/client.js"], styles: [], preloads: [] };

const request = (url: string, init?: mixed) => new Request(`http://localhost${url}`, init);

/** A directory holding `files`, in the system's temporary directory. */
function directoryWith(files: { [string]: string }): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-deploy-"));
  for (const name of Object.keys(files)) {
    const file = path.join(root, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, files[name]);
  }
  return root;
}

/**
 * A server bundle, as `handler.js` imports one.
 *
 * The same shape `serve.test.js` builds, and deliberately so: both files are
 * about the module `uf build` writes, and a second idea of what that module
 * looks like would let one of them pass against something the other could not.
 */
function appWith(options: {
  guard?: (request: Request) => Promise<Response | null> | Response | null,
  handler?: (request: Request) => Promise<Response | null> | Response | null,
  render?: (url: string) => { status: number, html: string },
}) {
  return {
    routes: [],
    handlers: [],
    middleware: [],
    notFound: [],
    errors: [],
    runMiddleware: async (request: Request) => (options.guard ? options.guard(request) : null),
    dispatch: async (request: Request) => (options.handler ? options.handler(request) : null),
    render: async (url: string) => {
      const answer = options.render
        ? options.render(url)
        : { status: 200, html: `<!doctype html><p>${url}</p>` };
      return {
        status: answer.status,
        pipe: (destination: { write: (chunk: string) => mixed, end: () => mixed, ... }) => {
          destination.write(answer.html);
          destination.end();
        },
        stream: () =>
          new ReadableStream({
            start(controller: mixed) {
              (controller: any).enqueue(new TextEncoder().encode(answer.html));
              (controller: any).close();
            },
          }),
      };
    },
  };
}

describe("the handler an adapter writes", () => {
  it("is reachable by its package name, which is what makes it an adapter's to use", async () => {
    const handle = createFetchHandler({
      app: appWith({ handler: () => Response.json({ ok: true }) }),
      document: assets,
    });

    const response = await handle(request("/api/health", { method: "POST", body: "{}" }));
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ ok: true });
  });

  it("touches no filesystem, so a worker can run it unchanged", async () => {
    // Nothing to assert about the absence of a read except that the handler
    // answers with no directory in sight: it is constructed from a module and
    // a table of URLs, and there is no argument it could have used to open a
    // file. That is the property every adapter after `node` depends on.
    const handle = createFetchHandler({ app: appWith({}), document: assets });

    const response = await handle(request("/posts/hello?draft=1"));
    expect(response.status).toBe(200);
    expect(await response.text()).toContain("/posts/hello?draft=1");
  });
});

describe("the Node front door an adapter's server.js runs", () => {
  it("serves the build's own files from the directory beside it", async () => {
    const staticDir = directoryWith({
      "index.html": "<!doctype html><p>home</p>",
      "guide/index.html": "<!doctype html><p>guide</p>",
      "assets/client.js": "console.log(1);",
    });
    const handle = createServeHandler({
      staticDir,
      handle: createFetchHandler({ app: appWith({}), document: assets }),
    });

    const asset = await handle(request("/assets/client.js"));
    expect(asset.status).toBe(200);
    expect(asset.headers.get("content-type")).toBe("text/javascript; charset=utf-8");

    // `/guide` and `/guide/` are the same prerendered document, and neither
    // spelling is the one a person types.
    for (const url of ["/guide", "/guide/"]) {
      const page = await handle(request(url));
      expect(await page.text()).toContain("guide");
    }
  });

  it("renders a route the build wrote no file for", async () => {
    const staticDir = directoryWith({ "index.html": "<!doctype html><p>home</p>" });
    const handle = createServeHandler({
      staticDir,
      handle: createFetchHandler({ app: appWith({}), document: assets }),
    });

    // The whole reason a deployment is more than a static host: a route with
    // parameters and no `generateStaticParams` has no file, and this is the
    // only thing that can answer it.
    const response = await handle(request("/posts/hello"));
    expect(response.status).toBe(200);
    expect(await response.text()).toContain("/posts/hello");
  });

  it("refuses a path that escapes the directory it was given", async () => {
    const staticDir = directoryWith({ "index.html": "<!doctype html><p>home</p>" });
    fs.writeFileSync(path.join(staticDir, "..", "uf-deploy-secret"), "not yours");
    const serveStatic = createStaticHandler({ root: staticDir });

    // Decoded first and checked after, so a percent-encoded traversal is the
    // same question as a plain one; `docs/security.md` rule 2.
    expect(await serveStatic(request("/../uf-deploy-secret"))).toBe(null);
    expect(await serveStatic(request("/%2e%2e/uf-deploy-secret"))).toBe(null);
  });
});

describe("the two front doors", () => {
  it("give one answer, including where a file and a handler collide", async () => {
    const distDir = directoryWith({
      "index.html": "<!doctype html><p>home</p>",
      // A path that is *both* a prerendered document and a route handler. The
      // router allows a handler beside a page in one directory, so this is a
      // project somebody can write, and it is the only request whose answer
      // depends on which implementation is running.
      "api/health/index.html": "<!doctype html><p>prerendered health</p>",
    });
    const app = appWith({
      handler: (request: Request) =>
        new URL(request.url).pathname.startsWith("/api/health")
          ? Response.json({ from: "the handler" })
          : null,
    });

    const started = createViteServeHandler({ entry: app, assets, distDir });
    const deployed = createServeHandler({
      staticDir: distDir,
      handle: createFetchHandler({ app, document: assets }),
    });

    for (const [url, init] of [
      ["/", undefined],
      ["/api/health", undefined],
      ["/api/health", { method: "POST", body: "{}" }],
      ["/posts/hello", undefined],
      ["/definitely-not-a-page", undefined],
      ["/../uf-deploy-secret", undefined],
    ]) {
      const fromStart = await started(request(String(url), init));
      const fromDeployment = await deployed(request(String(url), init));
      const said = `${String(init?.method ?? "GET")} ${String(url)}`;
      expect(`${said}: ${String(fromDeployment.status)}`).toBe(
        `${said}: ${String(fromStart.status)}`,
      );
      expect(`${said}: ${await fromDeployment.text()}`).toBe(`${said}: ${await fromStart.text()}`);
    }
  });
});
