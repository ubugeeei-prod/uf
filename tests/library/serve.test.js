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

import { describe, expect, it, uft } from "@uniflowed/test";

// Not a package export, deliberately. `internal/serve.js` is the seam a deploy
// adapter will need, and naming it in `exports` before one exists would be
// promising an interface nothing has used yet.
import {
  createApplicationHandler,
  createPrerenderGate,
  createServeHandler,
  createStaticHandler,
} from "../../packages/vite/internal/serve.js";
import { createWorkerFetch } from "@uniflowed/server/edge";
import { beginRequest } from "@uniflowed/server/host";
// `send` moved to the package a deployment links, and this import did not
// follow it — so this whole file stopped loading, and twenty-three assertions
// about the two handlers stopped being made while `uf test` printed
// "0 failed" above its own "could not run 1 file". See ubugeeei-prod/uf#400.
import { send } from "@uniflowed/server/node";

const assets = { scripts: ["/assets/client.js"], styles: [], preloads: [] };

/**
 * The half of a `ReadableStream` controller these fixtures use.
 *
 * `ReadableStream`'s own controller type is not among the libdefs uf ships,
 * and the fixture below used to reach for `(controller: any)` — two casts,
 * which `flow/unclear-type` rejects. Naming the two methods the fixture
 * actually calls says more than `any` did and costs one line.
 */
type StreamController = {
  readonly enqueue: (chunk: Uint8Array) => mixed,
  readonly close: () => mixed,
  ...
};

/** A server bundle, as `loadBuild` would have imported one. */
function entryWith(options: {
  guard?: (request: Request) => Promise<Response | null> | Response | null,
  handler?: (request: Request) => Promise<Response | null> | Response | null,
  render?: (url: string) => { status: number, html: string, headers?: { [string]: string } },
}) {
  const bodies = [];
  return {
    routes: [],
    handlers: [],
    middleware: [],
    notFound: [],
    errors: [],
    /** Every document this entry was asked for, so a test can ask what became of one. */
    bodies,
    runMiddleware: async (request: Request) => (options.guard ? options.guard(request) : null),
    // No action, and the real answer for a build that declares none: the
    // endpoint declines every request that carries no id, which is what puts
    // it in the order below without changing what anything else answers.
    callAction: async () => null,
    dispatch: async (request: Request) => (options.handler ? options.handler(request) : null),
    render: async (url: string) => {
      const answer = options.render
        ? options.render(url)
        : { status: 200, html: `<!doctype html><p>${url}</p>` };
      const body = bodyOf(answer.html);
      bodies.push(body);
      return { ...answer, ...body };
    },
  };
}

/**
 * The document methods a real `render` returns, over a string a test wrote.
 *
 * The handler asks for a stream now, so a fake that answered with `html` would
 * be testing a renderer uf no longer has. `cancelled` is what the `HEAD` case
 * turns on: the handler has to release the render rather than abandon it, and
 * a fake that ignored `cancel()` could not tell the two apart.
 */
function bodyOf(html: string) {
  let cancelled = false;
  return {
    cancelled: () => cancelled,
    pipe: async (destination: {
      readonly write: (chunk: string) => mixed,
      readonly end: () => mixed,
      ...
    }) => {
      destination.write(html);
      destination.end();
    },
    text: async () => html,
    stream: () =>
      new ReadableStream({
        start(controller: StreamController) {
          controller.enqueue(new TextEncoder().encode(html));
          controller.close();
        },
        cancel() {
          cancelled = true;
        },
      }),
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

  it("gives a HEAD the status and no body, and releases the render", async () => {
    // The status and the headers come from a real render, so the render
    // happens — and then has to be stopped. A body nobody reads is a React
    // render filling its queue and waiting for a drain that is never coming,
    // which is a request that never ends.
    const entry = entryWith({});
    const handle = createApplicationHandler({ entry, assets });

    const response = await handle(request("/", { method: "HEAD" }));
    expect(response.status).toBe(200);
    expect(await response.text()).toBe("");
    expect(response.headers.get("content-type")).toBe("text/html; charset=utf-8");
    expect(entry.bodies.length).toBe(1);
    expect(entry.bodies[0].cancelled()).toBe(true);
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

  /**
   * ubugeeei-prod/uf#550: the containment check was textual, so a path that
   * read as inside the root opened a file outside it.
   *
   * `dist/` is written by the build, but `public/` is copied verbatim from
   * whatever the author — or a dependency's install script — put there. The
   * same shape as the tarball traversals in `docs/security.md`, at the serving
   * end rather than the extraction end.
   */
  it("refuses a symlink that leaves the root, and serves one that does not", async () => {
    const outside = fs.mkdtempSync(path.join(os.tmpdir(), "uf-outside-"));
    fs.writeFileSync(path.join(outside, "secret.txt"), "the private key");

    const root = directoryWith({ "inside.txt": "public", "deep/target.txt": "also public" });
    fs.symlinkSync(path.join(outside, "secret.txt"), path.join(root, "escape.txt"));
    fs.symlinkSync(path.join(root, "deep/target.txt"), path.join(root, "stays.txt"));
    const serveStatic = createStaticHandler({ root });

    expect(await serveStatic(request("/escape.txt"))).toBe(null);
    // Indistinguishable from a file that is not there, which is what it should
    // look like: a 404 that differed would say whether the target exists.
    expect(await serveStatic(request("/nothing-at-all.txt"))).toBe(null);
    // A link that stays inside still works, because that is a thing people do
    // on purpose.
    expect(await (await serveStatic(request("/stays.txt")))?.text()).toBe("also public");
    expect(await (await serveStatic(request("/inside.txt")))?.text()).toBe("public");
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

  it("does not hand a draft request a prerendered document, but still serves its assets", async () => {
    // A file in `dist/` is what the site said before the draft existed, so an
    // editor who came to look at the draft has to reach the renderer instead.
    // Only documents: a stylesheet and a chunk are the same bytes either way,
    // and skipping those would leave the page unstyled and unhydrated for no
    // gain at all. ubugeeei-prod/uf#282.
    const root = directoryWith({
      "guide/index.html": "<p>published</p>",
      "assets/client.js": "export {};",
    });
    const serveStatic = createStaticHandler({ root });
    const drafting = { headers: { cookie: "__Host-uf.draft=1.whatever" } };

    expect(await serveStatic(request("/guide", drafting))).toBe(null);
    expect(await serveStatic(request("/guide/", drafting))).toBe(null);
    expect(await (await serveStatic(request("/assets/client.js", drafting)))?.text()).toBe(
      "export {};",
    );

    // And the same request without the cookie is answered off disk as before,
    // which is what makes the line above about draft mode rather than about
    // documents.
    expect(await (await serveStatic(request("/guide")))?.text()).toBe("<p>published</p>");
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

describe("writing a `Response` to a Node response", () => {
  /**
   * A `ServerResponse` with the events one has, and nothing else.
   *
   * `full` makes every `write` answer `false`, which is how a real one says
   * the kernel buffer is full and the remainder is being held in this process.
   */
  function outgoing(options?: {| readonly full?: boolean |}) {
    const listeners: Map<string, Array<() => mixed>> = new Map();
    return {
      statusCode: 0,
      statusMessage: "",
      written: ([]: Array<string>),
      ended: false,
      setHeader() {},
      write(chunk: Uint8Array): boolean {
        this.written.push(new TextDecoder().decode(chunk));
        return options?.full !== true;
      },
      end() {
        this.ended = true;
      },
      on(event: string, listener: () => mixed) {
        listeners.set(event, [...(listeners.get(event) ?? []), listener]);
        return this;
      },
      once(event: string, listener: () => mixed) {
        return this.on(event, listener);
      },
      off(event: string, listener: () => mixed) {
        listeners.set(
          event,
          (listeners.get(event) ?? []).filter((each) => each !== listener),
        );
        return this;
      },
      emit(event: string) {
        for (const listener of [...(listeners.get(event) ?? [])]) {
          listener();
        }
      },
      listening(event: string): number {
        return (listeners.get(event) ?? []).length;
      },
    };
  }

  /**
   * A body that never ends, and says when it was last read.
   *
   * Endless because a body that finishes on its own proves nothing about
   * pacing or about cancelling — the loop stops either way. The producer takes
   * a turn of the event loop, because one that resolves in a microtask starves
   * the timers below and because no real body is instant either.
   *
   * `reading` is true from the moment a pull is asked for until the chunk it
   * produces is enqueued, and it is what lets a test wait for the producer to
   * be *idle* rather than guess how long idle takes. A count alone cannot say
   * that: a pull already in flight raises it after the count was taken, which
   * is not the stream being read on — it is the same read finishing. See
   * ubugeeei-prod/uf#593.
   */
  function endless() {
    const state = { pulls: 0, reading: false, cancelled: false };
    const body = new ReadableStream({
      async pull(controller) {
        state.reading = true;
        await new Promise((resolve) => setTimeout(resolve, 2));
        state.pulls += 1;
        controller.enqueue(new TextEncoder().encode(`chunk ${String(state.pulls)}\n`));
        state.reading = false;
      },
      cancel() {
        state.cancelled = true;
      },
    });
    return { body, state };
  }

  it("stops reading while the socket is full, and goes on when it drains", async () => {
    // `write` answering `false` means the kernel buffer is full and everything
    // after it is being held in *this* process. Reading on regardless turns a
    // slow client, or an open-ended body, into a heap the size of everything
    // that client has not acknowledged: streaming in shape and buffering in
    // fact, which is what the renderer's `ChunkQueue` exists to avoid a layer
    // up and what this had no answer for at all.
    const { body, state } = endless();
    const response = outgoing({ full: true });
    const writing = send(response, new Response(body, { status: 200 }));

    // Waited for rather than timed, and the wait is for a *state* rather than
    // for a number: the loop is parked on `drain` — that listener is `send`'s
    // own, attached only while it is waiting to be told the socket can take
    // more — and nothing is in flight behind it. Thirty milliseconds and a
    // count used to stand in for both, and on a loaded machine the pull that
    // was already in flight when `write` came back `false` landed on the wrong
    // side of the line: `expected 2 to be 1`, two runs in three. That number
    // was the scheduler's to choose and never the code's. See
    // ubugeeei-prod/uf#593.
    //
    // Two seconds rather than `waitUntil`'s default second, and both waits in
    // this case together stay inside the five-second budget a case is given —
    // so a genuinely broken drain is reported as the wait that failed rather
    // than as a case that ran out of time, which says much less.
    await uft.waitUntil(() => response.listening("drain") === 1 && !state.reading, {
      timeout: 2_000,
    });
    const held = state.pulls;
    expect(held > 0).toBe(true);

    // And there it stays. Nothing is reading, nothing is in flight, and the
    // stream's own one-chunk buffer is full, so no length of wait can move the
    // count — while a loop that read on regardless of `write`'s answer moves
    // it about fifteen times in this window.
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(state.pulls).toBe(held);

    // A pause, not a stop.
    response.emit("drain");
    await uft.waitUntil(() => state.pulls > held, { timeout: 2_000 });

    response.emit("close");
    await writing;
  });

  it("cancels the body when the client hangs up", async () => {
    // Nothing written after a client closes goes anywhere, and the producer
    // behind the body — a render, a proxied upstream, an event stream — keeps
    // producing for a reader that is never coming back. `cancel()` is what
    // says so; `releaseLock()` would only detach this end.
    const { body, state } = endless();
    const response = outgoing();
    const writing = send(response, new Response(body, { status: 200 }));

    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(state.cancelled).toBe(false);
    response.emit("close");
    await writing;

    expect(state.cancelled).toBe(true);
    // The response is not ended: it is already gone, and `end()` on a closed
    // socket is a write to nowhere.
    expect(response.ended).toBe(false);
    // And the listener went with it. A server holds a response per in-flight
    // request, and one that accumulates listeners leaks per request served.
    expect(response.listening("close")).toBe(0);
  });

  it("still ends a response whose body finished normally", async () => {
    // The half that keeps the two above from being a wall: an ordinary body
    // is written and the response is closed, exactly as before.
    const response = outgoing();
    await send(response, new Response("hello", { status: 200 }));

    expect(response.written.join("")).toBe("hello");
    expect(response.ended).toBe(true);
    expect(response.listening("close")).toBe(0);
  });
});

// A prerendered document may not answer a draft request. That is one rule,
// `@uniflowed/server/internal/draft.js`'s `prerenderedMayAnswer`, and it is
// argued there once — every door below asks it rather than deciding again.
//
// It was not one rule, and that is ubugeeei-prod/uf#620. `uf start` and the
// compiled binary each applied it in their own static lookup and got it right;
// `uf preview` delegates the static half to Vite, whose file middleware runs in
// front of everything uf mounts behind it, so the skip that landed in #615 was
// dead code on that door and draft mode looked switched off there. The worker
// never applied it at all.
//
// `uf dev` is not in this table and does not need to be: it serves no
// prerendered document. Vite's dev server serves `public/`, a route path is
// never a file in it, and every navigation reaches uf's middleware and is
// rendered. There is nothing there for a draft request to be given instead.
describe("a draft request, at every front door", () => {
  const PUBLISHED = "<!doctype html><p>published</p>";
  const drafting = { headers: { cookie: "__Host-uf.draft=1.whatever" } };

  /**
   * A build: one prerendered page, one chunk, and an application that renders
   * something visibly different from what the build wrote.
   */
  function built() {
    const distDir = directoryWith({
      "guide/index.html": PUBLISHED,
      "assets/client.js": "export {};",
    });
    const entry = entryWith({
      render: (url: string) => ({ status: 200, html: `<!doctype html><p>drafted ${url}</p>` }),
    });
    return { distDir, entry };
  }

  /**
   * Vite's preview file middleware, as a handler.
   *
   * The contract it stands in for is one sentence of Vite's own: a `GET` under
   * the output directory is answered from the output directory, `/guide`
   * resolves through `guide/index.html`, and anything else is handed on. It has
   * never heard of a draft cookie, it is not uf's to teach, and a stand-in that
   * *had* heard of one would be testing a Vite that does not exist. That it
   * cannot be taught is the whole reason uf has to go in front of it.
   */
  function viteFiles(root: string) {
    return async (request: Request): Promise<Response | null> => {
      const { pathname } = new URL(request.url);
      for (const candidate of [
        path.join(root, pathname),
        path.join(root, pathname, "index.html"),
      ]) {
        if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
          return new Response(fs.readFileSync(candidate, "utf8"));
        }
      }
      return null;
    };
  }

  /**
   * What the assets binding would call these bytes.
   *
   * The extension and nothing else, because that is all a static host has to
   * go on — and it is the whole of what `createWorkerFetch` reads to tell a
   * document from a chunk. A stand-in that called every file `text/html`
   * would make the chunk assertion below pass for the wrong reason.
   */
  function contentType(pathname: string): string {
    if (pathname.endsWith(".js")) return "text/javascript; charset=utf-8";
    if (pathname.endsWith(".css")) return "text/css; charset=utf-8";
    return "text/html; charset=utf-8";
  }

  /** The three doors, each composed the way its command composes it. */
  function doors() {
    const { distDir, entry } = built();

    // `uf start`: uf owns the socket, so its own static handler is first and
    // the rule is inside it.
    const start = createServeHandler({ entry, assets, distDir });

    // `uf preview`: Vite's file middleware is first and cannot be moved, so uf
    // mounts the gate in front of it and the same handler behind. This is what
    // `packages/vite/driver.js` builds.
    const gate = createPrerenderGate();
    const preview = async (request: Request): Promise<Response> => {
      if (!(await gate(request.headers.get("cookie")))) return start(request);
      return (await viteFiles(distDir)(request)) ?? (await start(request));
    };

    // A worker: the platform's asset store answers first and uf never sees the
    // request until it has, so the rule is applied to the answer it gave.
    const cdn = viteFiles(distDir);
    const fetchFromWorker = createWorkerFetch({
      handle: createApplicationHandler({ entry, assets }),
      beginRequest,
    });
    const worker = async (request: Request): Promise<Response> =>
      fetchFromWorker(request, {
        ASSETS: {
          fetch: async (asked: Request) => {
            const found = await cdn(asked);
            // `"not_found_handling": "none"`, which is what `wrangler.json`
            // sets, and a `content-type` off the extension on everything it
            // does serve — both are the binding's own contract, and the second
            // is the whole of what this door has to read a document out of.
            if (found == null) return new Response("not found", { status: 404 });
            const { pathname } = new URL(asked.url);
            return new Response(await found.text(), {
              headers: { "content-type": contentType(pathname) },
            });
          },
        },
      });

    return { "uf start": start, "uf preview": preview, "a worker": worker };
  }

  it("is rendered rather than answered from the prerendered document", async () => {
    for (const [door, handle] of Object.entries(doors())) {
      const response = await handle(request("/guide/", drafting));
      const body = await response.text();
      expect(`${door}: ${String(response.status)}`).toBe(`${door}: 200`);
      expect(`${door}: ${body}`).toBe(`${door}: <!doctype html><p>drafted /guide/</p>`);
    }
  });

  it("still gets its stylesheets and chunks off disk", async () => {
    // Only documents. A chunk is the same bytes in draft mode as out of it,
    // and skipping it would leave the page unstyled and unhydrated for no gain.
    for (const [door, handle] of Object.entries(doors())) {
      const response = await handle(request("/assets/client.js", drafting));
      expect(`${door}: ${await response.text()}`).toBe(`${door}: export {};`);
    }
  });

  it("and the same request without the cookie is answered off disk", async () => {
    // Which is what makes the two above about draft mode rather than about
    // documents: take the cookie away and every door serves the file again.
    for (const [door, handle] of Object.entries(doors())) {
      const response = await handle(request("/guide/"));
      expect(`${door}: ${await response.text()}`).toBe(`${door}: ${PUBLISHED}`);
    }
  });
});
