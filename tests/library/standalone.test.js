// @flow
//
// `@uniflowed/server/standalone`: what a compiled binary does with a request.
//
// This is the half of `uf build --compile` that has nothing to do with
// compiling. The Rust side proves that one file comes out and that the file
// carries the site; what it cannot prove on a machine that is not allowed to
// bind a socket is that the file answers correctly, and that is what is below.
//
// It drives `createHandler` rather than `serve`, which is why it needs no
// socket at all — the handler is the whole of the request path and the socket
// is only how bytes reach it.
//
// The order the handler resolves things in is the subject of most of these: a
// file `uf build` already wrote — an embedded asset, or the prerendered
// document for this URL — then a route handler, then a render. Every one of
// those steps can shadow the next, and a shadow in the wrong direction is how
// a route handler stops answering or a page starts being served for `POST`.
//
// It is also the order `uf preview` and `uf start` resolve in, and it is not
// this module's to choose: `uf preview` is Vite's own server, which serves a
// file before anything uf mounts behind it. So the assertions below are the
// same assertions `serve.test.js` makes about the other two, written against
// the handler that has the files inside it rather than on disk.

import { Buffer } from "node:buffer";

import { describe, expect, it } from "@uniflowed/testing";
import { createHandler } from "@uniflowed/server/standalone";

/** A file as the build embeds it. */
function embed(type: string, contents: string) {
  return { type, body: Buffer.from(contents, "utf8").toString("base64") };
}

/**
 * A build's worth of embedded files: a home page, a nested page, and a chunk.
 */
const assets = {
  "index.html": embed("text/html; charset=utf-8", "<!doctype html><p>prerendered home</p>"),
  "guide/index.html": embed("text/html; charset=utf-8", "<!doctype html><p>prerendered guide</p>"),
  "assets/app-a1b2c3.js": embed("text/javascript; charset=utf-8", "console.log('hydrate')"),
  "brand/logo с пробелом.svg": embed("image/svg+xml", "<svg/>"),
};

const document = { scripts: ["/assets/app-a1b2c3.js"], styles: [], preloads: [] };

/**
 * A render result of the shape the router hands back: a shell that is ready,
 * and a body still arriving.
 *
 * The two delivery methods a Node server uses — `pipe` for a response and
 * `stream` for the `HEAD` that cancels it. Written out rather than imported
 * from the router, because what is under test is the shim's half of the
 * contract, and a fake that satisfies the real one is the only way to see the
 * shim get it wrong.
 */
function rendered(status: number, html: string) {
  let cancelled = false;
  return {
    status,
    // `async`, because the router's is: `DocumentBody.pipe` resolves on the
    // last byte rather than on the first, and a fake that resolved
    // synchronously would hide a caller that never waited for either.
    pipe: async (destination) => {
      destination.write(Buffer.from(html, "utf8"));
      destination.end();
    },
    stream: () => ({
      cancel: async () => {
        cancelled = true;
      },
      get cancelled() {
        return cancelled;
      },
    }),
    wasCancelled: () => cancelled,
  };
}

/**
 * An application that records what it was asked, so a test can tell "the
 * renderer produced this" from "a file was found".
 */
function application() {
  const asked = { rendered: [], dispatched: [], guarded: [] };
  const app = {
    render: async (url: string) => {
      asked.rendered.push(url);
      return url.startsWith("/nowhere")
        ? rendered(404, "<!doctype html><p>not found</p>")
        : rendered(200, `<!doctype html><p>rendered ${url}</p>`);
    },
    // A guard that lets everything through, so every existing case is about
    // what it was about. The two cases below turn it on.
    runMiddleware: async (request: Request) => {
      asked.guarded.push(new URL(request.url).pathname);
      return null;
    },
    dispatch: async (request: Request) => {
      const { pathname } = new URL(request.url);
      asked.dispatched.push(`${request.method} ${pathname}`);
      if (pathname !== "/api/echo") {
        return null;
      }
      return new Response(`handled ${request.method}`, {
        status: 201,
        headers: { "content-type": "text/plain" },
      });
    },
  };
  return { app, asked };
}

/**
 * A Node response that remembers everything written to it.
 *
 * With the events a `ServerResponse` has, because the handler uses them: a body
 * is paced by `drain` and stopped by `close`, and a fake without them would let
 * the shim be written against a response that does not exist.
 *
 * `full` makes every `write` answer `false`, which is how a real one says the
 * kernel buffer is full and the rest is being held in this process.
 */
function recorder(options?: {| readonly full?: boolean |}) {
  const chunks = [];
  const listeners: Map<string, Array<() => mixed>> = new Map();
  return {
    statusCode: 0,
    headers: ({}: { [string]: string }),
    setHeader(name: string, value: string) {
      this.headers[name.toLowerCase()] = value;
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
    /** Fire an event, the way a socket would. */
    emit(event: string) {
      for (const listener of [...(listeners.get(event) ?? [])]) {
        listener();
      }
    },
    /** How many listeners are still attached, so a leak is visible. */
    listening(event: string): number {
      return (listeners.get(event) ?? []).length;
    },
    write(chunk) {
      chunks.push(Buffer.from(chunk));
      return options?.full !== true;
    },
    end(chunk) {
      if (chunk != null) {
        chunks.push(Buffer.from(chunk));
      }
    },
    // A real `ServerResponse` has one, and the handler needs it: a render that
    // fails after the shell has no status left to answer with.
    destroyed: (null: mixed),
    destroy(error) {
      this.destroyed = error ?? true;
      return this;
    },
    body(): string {
      return Buffer.concat(chunks).toString("utf8");
    },
  };
}

/** One request through a fresh handler, with what the application saw. */
async function request(method: string, url: string) {
  const { app, asked } = application();
  const handle = createHandler({ app, assets, document });
  const response = recorder();
  await handle({ method, url, headers: { host: "example.test" } }, response);
  return { response, asked };
}

describe("embedded files", () => {
  it("serves a chunk out of the binary, with the type the build decided", async () => {
    const { response, asked } = await request("GET", "/assets/app-a1b2c3.js");
    expect(response.statusCode).toBe(200);
    expect(response.headers["content-type"]).toBe("text/javascript; charset=utf-8");
    expect(response.body()).toBe("console.log('hydrate')");
    // The renderer must never have been consulted: a request for a chunk is
    // not a route, and in `uf dev` it never reaches uf's middleware at all.
    expect(asked.rendered).toEqual([]);
  });

  it("sends a length, so a client can keep the connection", async () => {
    const { response } = await request("GET", "/assets/app-a1b2c3.js");
    expect(response.headers["content-length"]).toBe("22");
  });

  it("percent-decodes a path before looking for the file", async () => {
    const { response } = await request(
      "GET",
      "/brand/logo%20%D1%81%20%D0%BF%D1%80%D0%BE%D0%B1%D0%B5%D0%BB%D0%BE%D0%BC.svg",
    );
    expect(response.statusCode).toBe(200);
    expect(response.headers["content-type"]).toBe("image/svg+xml");
  });

  it("renders rather than crashing when a path will not decode", async () => {
    // `%ZZ` is not an escape. A malformed request is a request, not a 500.
    const { response, asked } = await request("GET", "/%ZZ");
    expect(response.statusCode).toBe(200);
    expect(asked.rendered).toEqual(["/%ZZ"]);
  });

  it("cannot be made to find something on Object.prototype", async () => {
    // The lookup is a `Map` for exactly this: with a plain object, a request
    // for `/constructor` finds a function and serves it.
    for (const path of ["/constructor", "/__proto__", "/hasOwnProperty"]) {
      const { response, asked } = await request("GET", path);
      expect(response.statusCode).toBe(200);
      expect(asked.rendered).toEqual([path]);
    }
  });
});

describe("prerendered documents", () => {
  it("serves the home page that `uf build` already wrote", async () => {
    const { response, asked } = await request("GET", "/");
    expect(response.statusCode).toBe(200);
    expect(response.body()).toBe("<!doctype html><p>prerendered home</p>");
    expect(response.headers["cache-control"]).toBe("no-cache");
    expect(asked.rendered).toEqual([]);
  });

  it("finds a nested page with or without the trailing slash", async () => {
    for (const url of ["/guide", "/guide/"]) {
      const { response, asked } = await request("GET", url);
      expect(response.body()).toBe("<!doctype html><p>prerendered guide</p>");
      expect(asked.rendered).toEqual([]);
    }
  });

  it("renders a route that was not prerendered", async () => {
    const { response, asked } = await request("GET", "/guide/dynamic?q=1");
    expect(response.statusCode).toBe(200);
    expect(response.body()).toBe("<!doctype html><p>rendered /guide/dynamic?q=1</p>");
    // The query string reaches the renderer: a page that reads it would
    // otherwise render the wrong thing for every search on the site.
    expect(asked.rendered).toEqual(["/guide/dynamic?q=1"]);
  });

  it("answers an unrouted path with the renderer's own status", async () => {
    const { response } = await request("GET", "/nowhere/at/all");
    expect(response.statusCode).toBe(404);
  });
});

describe("route handlers", () => {
  it("answers a GET the handler claims", async () => {
    const { response } = await request("GET", "/api/echo");
    expect(response.statusCode).toBe(201);
    expect(response.body()).toBe("handled GET");
  });

  it("answers a POST, which no page ever can", async () => {
    const { response, asked } = await request("POST", "/api/echo");
    expect(response.statusCode).toBe(201);
    expect(response.body()).toBe("handled POST");
    expect(asked.rendered).toEqual([]);
  });

  it("offers every request to the dispatcher before rendering", async () => {
    const { asked } = await request("GET", "/guide/dynamic");
    expect(asked.dispatched).toEqual(["GET /guide/dynamic"]);
  });

  it("does not shadow a page the build already prerendered", async () => {
    // The router lets `_uf.route.js` sit beside `_uf.page.js`, so one path can
    // have both a handler and a prerendered document. Vite's preview server
    // serves the file first and gives uf no say in it, so a compiled binary
    // that let the handler win would answer one way when the build was checked
    // with `uf preview` and another way once it was deployed. See
    // `@uniflowed/vite`'s `internal/serve.js`.
    const dispatched = [];
    const handle = createHandler({
      app: {
        render: async () => rendered(200, "<!doctype html><p>rendered</p>"),
        runMiddleware: async () => null,
        dispatch: async (request: Request) => {
          dispatched.push(new URL(request.url).pathname);
          return new Response("handled", { status: 201 });
        },
      },
      assets,
      document,
    });

    const response = recorder();
    await handle({ method: "GET", url: "/guide/", headers: { host: "example.test" } }, response);

    expect(response.statusCode).toBe(200);
    expect(response.body()).toBe("<!doctype html><p>prerendered guide</p>");
    expect(dispatched).toEqual([]);
  });

  it("refuses a POST to a page instead of rendering one", async () => {
    // A page cannot answer a POST. Rendering one would turn a missing handler
    // into a 200 where the caller expected to be told the method was wrong.
    const { response, asked } = await request("POST", "/guide/");
    expect(response.statusCode).toBe(405);
    // Required by the specification on every 405, and computable here because
    // a page has exactly these two methods.
    expect(response.headers["allow"]).toBe("GET, HEAD");
    expect(asked.rendered).toEqual([]);
  });
});

describe("HEAD", () => {
  it("answers an embedded file with its headers and no body", async () => {
    const { response } = await request("HEAD", "/assets/app-a1b2c3.js");
    expect(response.statusCode).toBe(200);
    expect(response.headers["content-length"]).toBe("22");
    expect(response.body()).toBe("");
  });

  it("answers a rendered page with its headers and no body", async () => {
    const { response } = await request("HEAD", "/guide/dynamic");
    expect(response.statusCode).toBe(200);
    expect(response.body()).toBe("");
  });
});

describe("writing a handler's body to the socket", () => {
  /**
   * A handler whose body is endless, and which says when it was last read.
   *
   * Endless on purpose: a body that finishes on its own proves nothing about
   * pacing or about cancelling, because the loop stops either way.
   */
  function endless() {
    const state = { pulls: 0, cancelled: false };
    const handle = createHandler({
      app: {
        render: async () => rendered(200, "<p>unused</p>"),
        runMiddleware: async () => null,
        dispatch: async () =>
          new Response(
            new ReadableStream({
              // `async`, with a turn of the event loop in it, because a
              // producer that resolves in a microtask starves the timers this
              // test is driven by — and no real body is instant either.
              async pull(controller) {
                await new Promise((resolve) => setTimeout(resolve, 2));
                state.pulls += 1;
                controller.enqueue(new TextEncoder().encode(`chunk ${String(state.pulls)}\n`));
              },
              cancel() {
                state.cancelled = true;
              },
            }),
            { status: 200, headers: { "content-type": "text/plain" } },
          ),
      },
      assets,
      document,
    });
    return { handle, state };
  }

  it("stops reading while the socket is full, and goes on when it drains", async () => {
    // `write` answering `false` means the kernel buffer is full and everything
    // after it is being held in *this* process. A loop that read on regardless
    // turns a slow client into a heap the size of everything it has not
    // acknowledged — streaming in shape and buffering in fact, which is the
    // failure `ChunkQueue` exists to avoid one layer up in the renderer.
    const { handle, state } = endless();
    const response = recorder({ full: true });
    const request = handle(
      { method: "GET", url: "/api/echo", headers: { host: "example.test" } },
      response,
    );

    await new Promise((resolve) => setTimeout(resolve, 30));
    const held = state.pulls;
    expect(held > 0).toBe(true);
    // Nothing more was pulled while the socket said it was full, however many
    // turns of the loop went by.
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(state.pulls).toBe(held);

    // And it is a pause rather than a stop.
    response.emit("drain");
    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(state.pulls > held).toBe(true);

    response.emit("close");
    await request;
  });

  it("cancels the body when the client hangs up", async () => {
    // Nothing written after a client closes goes anywhere, and the producer
    // behind the body — a render, a proxied upstream, an event stream — goes on
    // producing for a reader that is never coming back. `cancel()` is what
    // tells it to stop; `releaseLock()` only detaches this end.
    const { handle, state } = endless();
    const response = recorder();
    const request = handle(
      { method: "GET", url: "/api/echo", headers: { host: "example.test" } },
      response,
    );

    await new Promise((resolve) => setTimeout(resolve, 30));
    expect(state.cancelled).toBe(false);
    response.emit("close");
    await request;

    expect(state.cancelled).toBe(true);
    // And the listener went with it: a server keeps a response object per
    // in-flight request, and one that accumulates listeners is a leak measured
    // in requests served.
    expect(response.listening("close")).toBe(0);
  });
});

describe("a render that fails after the shell", () => {
  /**
   * A render whose shell is out and whose body then throws.
   *
   * Which is what `DocumentBody.pipe` does: React hands a post-shell failure
   * to the destination's `destroy(error)`, `ChunkQueue.fail` records it, and
   * the generator being iterated rethrows it — so the promise `pipe` returned
   * rejects *after* the status and the first bytes have gone out. The
   * rejection is deferred by a turn on purpose: a handler that did not await
   * would have returned before it happened, which is exactly the bug.
   */
  function failingRender(error: Error) {
    return {
      status: 200,
      pipe: async (destination) => {
        destination.write(Buffer.from("<!doctype html><p>the shell", "utf8"));
        await new Promise((resolve) => setTimeout(resolve, 0));
        destination.end();
        throw error;
      },
      stream: () => ({ cancel: async () => {} }),
    };
  }

  /** One request whose render fails, with whatever reached stderr. */
  async function failing(error: Error) {
    const said = [];
    const write = process.stderr.write;
    // eslint-disable-next-line no-undef
    (process.stderr: $FlowFixMe).write = (chunk) => {
      said.push(String(chunk));
      return true;
    };
    const response = recorder();
    try {
      const handle = createHandler({
        app: {
          render: async () => failingRender(error),
          runMiddleware: async () => null,
          dispatch: async () => null,
        },
        assets,
        document,
      });
      await handle({ method: "GET", url: "/late", headers: { host: "example.test" } }, response);
    } finally {
      // eslint-disable-next-line no-undef
      (process.stderr: $FlowFixMe).write = write;
    }
    return { response, said: said.join("") };
  }

  it("waits for the body, so the failure does not escape the handler", async () => {
    // `rendered.pipe(response)` was called and dropped. The promise it returns
    // rejects on a post-shell failure, and a rejection nobody is holding is an
    // unhandled rejection — which under Node's default
    // `--unhandled-rejections=throw` takes the whole binary down, in the
    // middle of a request the server had otherwise survived. `serve`'s
    // `handle(…).catch` cannot help: it had already resolved.
    const { response } = await failing(new Error("the boundary threw late"));
    expect(response.destroyed).toBeTruthy();
  });

  it("says so where a supervisor looks", async () => {
    // The only trace a page that failed after its first byte leaves anywhere.
    // There is no framework above this server and no log drain beside it.
    const { said } = await failing(new Error("the boundary threw late"));
    expect(said).toContain("the boundary threw late");
  });

  it("drops the socket rather than closing a truncated document cleanly", async () => {
    // The shell went out with a 200 on it, so there is no status left to
    // answer with. Ending the chunked response normally would tell the client
    // that the half-document it received is the whole document, which is the
    // silent success this pull request exists to stop.
    const { response } = await failing(new Error("the boundary threw late"));
    expect(response.statusCode).toBe(200);
    expect(response.body()).toBe("<!doctype html><p>the shell");
    expect(String(response.destroyed)).toContain("the boundary threw late");
  });
});

describe("the guard on the path", () => {
  it("runs before the dispatcher and before the render", async () => {
    // ubugeeei-prod/uf#260, in the one place it had not been closed: a
    // middleware ran under `uf dev`, under `uf preview` and under `uf start`,
    // and a compiled binary served the same application without it. An auth
    // check that stops running when the build is compiled is worse than one
    // that never worked, because the version that was tested is the version
    // that had it.
    const { app, asked } = application();
    const handle = createHandler({
      app: {
        ...app,
        runMiddleware: async (request: Request) => {
          asked.guarded.push(new URL(request.url).pathname);
          return new URL(request.url).pathname.startsWith("/dashboard")
            ? new Response(null, { status: 302, headers: { location: "/sign-in" } })
            : null;
        },
      },
      assets,
      document,
    });

    const guarded = recorder();
    await handle(
      { method: "GET", url: "/dashboard/reports", headers: { host: "example.test" } },
      guarded,
    );

    expect(guarded.statusCode).toBe(302);
    expect(guarded.headers.location).toBe("/sign-in");
    // Neither of the two below it was reached.
    expect(asked.dispatched).toEqual([]);
    expect(asked.rendered).toEqual([]);
  });

  it("guards a path under it that matches no route at all", async () => {
    // The reason the guard is a flat table keyed by path rather than a field
    // on a route: `/dashboard/typo` matches nothing, and a 404 rendered
    // without the guard is the page the guard existed to keep private saying
    // "no such page" to someone who should not have been asked.
    const { app, asked } = application();
    const handle = createHandler({
      app: {
        ...app,
        runMiddleware: async (request: Request) =>
          new URL(request.url).pathname.startsWith("/dashboard")
            ? new Response(null, { status: 302, headers: { location: "/sign-in" } })
            : null,
      },
      assets,
      document,
    });

    const response = recorder();
    await handle(
      { method: "GET", url: "/dashboard/typo", headers: { host: "example.test" } },
      response,
    );

    expect(response.statusCode).toBe(302);
    expect(asked.rendered).toEqual([]);
  });

  it("lets an unguarded path through to the render", async () => {
    // The half that proves the guard is conditional rather than a wall.
    const { app, asked } = application();
    const handle = createHandler({ app, assets, document });

    const response = recorder();
    await handle({ method: "GET", url: "/about", headers: { host: "example.test" } }, response);

    expect(response.statusCode).toBe(200);
    expect(asked.guarded).toEqual(["/about"]);
    expect(asked.rendered).toEqual(["/about"]);
  });
});
