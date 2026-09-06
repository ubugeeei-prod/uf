// @flow
//
// One request, one context, settled after the response.
//
// `after()` is documented as "Run `callback` once the response has been sent",
// and until this suite existed nothing in uf ran one after a response was sent.
// The middleware runner drained at the end of the chain — which, when the chain
// answered, was several lines before the caller wrote a byte, and when it
// declined was before there was a response at all — and the dispatcher drained
// the moment a handler returned, which is "the response is in hand" and not the
// same sentence. `createRenderer` established no context whatsoever, so
// `cookies()` in a page threw. See ubugeeei-prod/uf#389.
//
// The fix is one sentence and this file is what holds it to it: the *host*
// owns the request. `beginRequest` gives it `run`, which wraps everything that
// decides the response, and `settle`, which it calls after the response has
// been written. The router's middleware runner, its dispatcher and its
// renderer run inside whatever request is ambient and establish none.
//
// # Why the ordering is asserted with a list rather than with time
//
// "The callback ran after the response" is a claim about order, and the honest
// way to make it is to have the thing that writes the response and the thing
// that was deferred both push onto one array and then read it. A callback that
// merely ran is not the claim; the old code ran it too.
//
// # What is not here
//
// `uf dev` and the plugin's own `configureServer` need a Vite server to drive,
// so their half is `crates/uf_cli/tests/vite.rs`. What is testable without a
// socket is every decision they share with the rest: the router's, and the two
// Node hosts' — `@uniflowed/server/node`'s `nodeListener`, which is `uf start`
// and the `server.js` an adapter writes and, spelled out in `driver.js`,
// `uf preview`; and `createHandler`, which is the compiled binary.

import * as React from "@uniflowed/react";
import { describe, expect, it } from "@uniflowed/test";
import { createDispatcher } from "@uniflowed/router/handler";
import { createMiddlewareRunner } from "@uniflowed/router/middleware";
import { beginRequest, createRenderer } from "@uniflowed/router/server";
import { routerView } from "@uniflowed/router";
import { after, cookies, draftMode, headers } from "@uniflowed/server";
import { createHandler } from "@uniflowed/server/standalone";

import { nodeListener } from "@uniflowed/server/node";

// Not a package export: the generated server module's text is `@uniflowed/vite`'s
// own, and `serve.test.js` reaches into that package the same way.
import { serverModuleSource } from "../../packages/vite/internal/routes.js";

const get = (url: string, init?: mixed) => new Request(`http://localhost${url}`, init);

/** A middleware table entry whose module is given inline. */
const guard = (path: string, middleware: mixed) => ({
  path,
  file: `app${path === "/" ? "" : path}/_uf.middleware.js`,
  load: async () => ({ default: middleware }),
});

/** A handler table entry whose module is given inline. */
const route = (path: string, module: mixed) => ({
  path,
  params: [],
  file: `app${path}/_uf.route.js`,
  load: async () => module,
});

/**
 * A host: begin the request, answer it, write it, settle it.
 *
 * `write` stands for the host's last line — `send` returning, `pipe`
 * resolving, `end` flushing — because what each host writes with differs and
 * *when* it settles must not. The two Node hosts below are driven for real; a
 * middleware and a handler have no socket in them and this is the shape they
 * see.
 */
async function serving<T>(request: Request, answer: () => Promise<T>, write: () => mixed) {
  const { run, settle } = beginRequest(request);
  try {
    const result = await run(answer);
    write();
    return result;
  } finally {
    await settle();
  }
}

describe("the seam the hosts reach through", () => {
  it("hands `beginRequest` out from the application bundle", async () => {
    // Every host below takes the lifecycle from `virtual:uf/server` rather than
    // importing `@uniflowed/server/host` for itself, and the reason is not
    // tidiness: the request lives in an `AsyncLocalStorage` belonging to one
    // module instance, and a host that resolved its own would begin the request
    // in a storage the application cannot read. Nothing would fail loudly — the
    // guard would run, the page would render, and every `cookies()` in it would
    // throw "outside a request" as though no host had run at all.
    //
    // The four hosts are driven end to end by `crates/uf_cli/tests/vite.rs`,
    // which needs sockets. This is the one line of that seam that does not.
    expect(serverModuleSource("./app.js")).toContain(
      'export { beginRequest } from "@uniflowed/router/server";',
    );
  });
});

describe("beginRequest", () => {
  it("puts the request in scope for everything `run` awaits", async () => {
    const { run, settle } = beginRequest(get("/", { headers: { "x-uf": "1" } }));

    const seen = await run(async () => {
      await Promise.resolve();
      return headers().get("x-uf");
    });
    await settle();

    expect(seen).toBe("1");
  });

  it("leaves nothing in scope afterwards", async () => {
    const { run, settle } = beginRequest(get("/"));
    await run(async () => headers().get("x-uf"));
    await settle();

    expect(() => headers()).toThrow();
  });

  it("settles once, however many times a host says the response has gone", async () => {
    // A host learns a response is over more than once — the body stream closed,
    // and then the socket did. Draining twice would run whatever the first
    // drain's callbacks registered, at a moment nothing asked for.
    let runs = 0;
    const { run, settle } = beginRequest(get("/"));
    await run(async () => {
      after(() => {
        runs += 1;
      });
    });
    await settle();
    await settle();

    expect(runs).toBe(1);
  });
});

describe("a middleware's deferred work", () => {
  it("runs after the guarded response, not before it", async () => {
    const order: Array<string> = [];
    const runMiddleware = createMiddlewareRunner({
      middleware: [
        guard("/dashboard", () => {
          after(() => order.push("audited the denial"));
          return new Response(null, { status: 403 });
        }),
      ],
    });

    const request = get("/dashboard");
    const response = await serving(
      request,
      () => runMiddleware(request),
      () => order.push("403 written"),
    );

    expect(response?.status).toBe(403);
    // The claim, in one line. The runner used to drain before returning, so
    // "audited the denial" came first — the audit of a rejection recorded
    // before the rejection was sent.
    expect(order).toEqual(["403 written", "audited the denial"]);
  });

  it("runs after the response of a request the guard let through", async () => {
    // The worse half of the two. When the chain declines there is no response
    // at all yet: the handler or the page has not run. A logging middleware
    // drained here recorded a request that had not been answered.
    const order: Array<string> = [];
    const runMiddleware = createMiddlewareRunner({
      middleware: [
        guard("/", () => {
          after(() => order.push("logged"));
        }),
      ],
    });
    const dispatch = createDispatcher({
      handlers: [
        route("/api/thing", {
          GET: () => {
            order.push("handler ran");
            return new Response("ok");
          },
        }),
      ],
    });

    const request = get("/api/thing");
    await serving(
      request,
      async () => (await runMiddleware(request)) ?? (await dispatch(request)),
      () => order.push("200 written"),
    );

    expect(order).toEqual(["handler ran", "200 written", "logged"]);
  });
});

describe("a handler's deferred work", () => {
  it("runs after the response is written, not when it is returned", async () => {
    const order: Array<string> = [];
    const dispatch = createDispatcher({
      handlers: [
        route("/api/orders", {
          POST: () => {
            after(() => order.push("receipt emailed"));
            order.push("handler returned");
            return new Response(null, { status: 201 });
          },
        }),
      ],
    });

    const request = get("/api/orders", { method: "POST" });
    await serving(
      request,
      () => dispatch(request),
      () => order.push("201 written"),
    );

    // The dispatcher used to drain between the first and the second of these,
    // which its own comment called "the response is in hand". For a streamed
    // body that is a document earlier than "the response has been sent".
    expect(order).toEqual(["handler returned", "201 written", "receipt emailed"]);
  });
});

describe("one context for the whole request", () => {
  it("shows a handler what the guard above it did", async () => {
    // Two contexts on one request was the other half of the bug: the runner
    // built one and the dispatcher built another, so a guard that turned draft
    // mode on was talking to a context the handler underneath could not see.
    const runMiddleware = createMiddlewareRunner({
      middleware: [
        guard("/", () => {
          draftMode().enable();
        }),
      ],
    });
    const dispatch = createDispatcher({
      handlers: [
        route("/api/post", { GET: () => Response.json({ draft: draftMode().isEnabled }) }),
      ],
    });

    const request = get("/api/post");
    const response = await serving(
      request,
      async () => (await runMiddleware(request)) ?? (await dispatch(request)),
      () => {},
    );

    expect(await response?.json()).toEqual({ draft: true });
  });

  it("drains a guard's callback and a handler's as one ordered list", async () => {
    const order: Array<string> = [];
    const runMiddleware = createMiddlewareRunner({
      middleware: [guard("/", () => after(() => order.push("from the guard")))],
    });
    const dispatch = createDispatcher({
      handlers: [
        route("/api/thing", {
          GET: () => {
            after(() => order.push("from the handler"));
            return new Response("ok");
          },
        }),
      ],
    });

    const request = get("/api/thing");
    await serving(
      request,
      async () => (await runMiddleware(request)) ?? (await dispatch(request)),
      () => order.push("written"),
    );

    // Registration order, in one drain, after the response — not two drains at
    // two different moments, neither of them after anything was sent.
    expect(order).toEqual(["written", "from the guard", "from the handler"]);
  });
});

/** A route table of one page at `/`. */
function pageTable(Page: React.ComponentType<empty>) {
  return {
    routes: [
      {
        path: "/",
        params: [],
        mdx: false,
        file: "app/_uf.page.js",
        page: () => Promise.resolve({ default: Page }),
        layouts: [],
        loading: [],
      },
    ],
    notFound: [],
    errors: [],
  };
}

describe("the request a page is inside", () => {
  it("lets a page read the request's cookies", async () => {
    // `createRenderer` establishes no context and never did, so this threw
    // `cookies() was called outside a request` for every page in every uf
    // application — the third place the feature was wrong, and the one where
    // it was not mis-ordered but absent. It works now for the same reason the
    // other two do: the host began the request, and the render is inside it.
    component Page() {
      return cookies().get("session") ?? "no session";
    }
    const renderer = createRenderer({ App: routerView("./app"), ...pageTable(Page) });

    const request = get("/", { headers: { cookie: "session=abc" } });
    const document = await serving(
      request,
      async () => {
        const result = await renderer.render("/", { scripts: [], styles: [], preloads: [] });
        return result.text();
      },
      () => {},
    );

    expect(document).toContain("abc");
  });

  it("cannot, outside one, which is why every host establishes one", async () => {
    // The renderer gained nothing in this change and that is the point: it
    // still establishes no context, so a page rendered outside a request still
    // fails. What changed is that there is no longer a host that renders
    // outside one — `uf dev`, `uf preview`, `uf start` and a compiled binary
    // all begin the request above the render. Before that, this was every
    // page in every uf application.
    component Page() {
      return cookies().get("session") ?? "no session";
    }
    const renderer = createRenderer({ App: routerView("./app"), ...pageTable(Page) });

    const result = await renderer.render("/", { scripts: [], styles: [], preloads: [] });

    expect(String((result.error: $FlowFixMe)?.message)).toContain(
      "cookies() was called outside a request",
    );
  });

  it("runs a page's deferred work after the document has been written", async () => {
    const order: Array<string> = [];
    component Page() {
      after(() => order.push("view recorded"));
      return "home";
    }
    const renderer = createRenderer({ App: routerView("./app"), ...pageTable(Page) });

    const request = get("/");
    await serving(
      request,
      async () => {
        const result = await renderer.render("/", { scripts: [], styles: [], preloads: [] });
        return result.text();
      },
      () => order.push("document written"),
    );

    expect(order).toEqual(["document written", "view recorded"]);
  });
});

/**
 * A server bundle, with the real router behind it.
 *
 * `beginRequest` is the real one and it has to be: a host takes it from the
 * bundle so that the storage it begins a request in is the storage the bundle's
 * own `cookies()` reads. A fake here would prove the host calls something.
 */
function bundle(options: {
  middleware?: $ReadOnlyArray<mixed>,
  handlers?: $ReadOnlyArray<mixed>,
  render?: (url: string) => mixed,
}) {
  return {
    beginRequest,
    runMiddleware: createMiddlewareRunner({ middleware: options.middleware ?? [] }),
    dispatch: createDispatcher({ handlers: options.handlers ?? [] }),
    render: options.render ?? (async () => ({ status: 200, stream: () => emptyStream() })),
  };
}

function emptyStream(): ReadableStream {
  return new ReadableStream({
    start(controller: mixed) {
      (controller: $FlowFixMe).close();
    },
  });
}

/** A Node request, as `node:http` hands one over. */
const incoming = (method: string, url: string) => ({
  method,
  url,
  headers: { host: "example.test" },
});

const text = (chunk: Uint8Array | string) =>
  typeof chunk === "string" ? chunk : new TextDecoder().decode(chunk);

/** A `ServerResponse` with the members the writers touch, and a record. */
function outgoing(order: Array<string>) {
  const listeners: Map<string, Array<() => mixed>> = new Map();
  return {
    statusCode: 0,
    statusMessage: "",
    headersSent: false,
    setHeader() {},
    write(chunk: Uint8Array): boolean {
      order.push(`wrote ${text(chunk)}`);
      return true;
    },
    // A string as well as bytes, because a `ServerResponse` takes either and
    // the 500 below is written as one.
    end(chunk?: Uint8Array | string) {
      if (chunk != null) order.push(`wrote ${text(chunk)}`);
      order.push("response ended");
    },
    destroy() {},
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
  };
}

describe("`uf start`, `uf preview` and a deployed directory", () => {
  it("settles the request after the last byte, not after the handler", async () => {
    // `nodeListener` is `uf start`'s server, the socket the `server.js` an
    // adapter writes takes, and — spelled out in `driver.js`, because Vite's
    // chain is not a bare `node:http` server — `uf preview`'s. All of them
    // write with `send`, and all of them settle on the line after it.
    const order: Array<string> = [];
    const entry = bundle({
      handlers: [
        route("/api/thing", {
          GET: () => {
            after(() => order.push("metric flushed"));
            return new Response("ok");
          },
        }),
      ],
    });
    const listen = nodeListener(async (request) => await entry.dispatch(request), {
      beginRequest: entry.beginRequest,
    });

    const response = outgoing(order);
    await listen(incoming("GET", "/api/thing"), response);

    expect(order).toEqual(["wrote ok", "response ended", "metric flushed"]);
  });

  it("settles a request the guard answered, after that answer is written", async () => {
    const order: Array<string> = [];
    const entry = bundle({
      middleware: [
        guard("/dashboard", () => {
          after(() => order.push("denial audited"));
          return new Response(null, { status: 403 });
        }),
      ],
    });
    const listen = nodeListener(async (request) => await entry.runMiddleware(request), {
      beginRequest: entry.beginRequest,
    });

    await listen(incoming("GET", "/dashboard"), outgoing(order));

    expect(order).toEqual(["response ended", "denial audited"]);
  });

  it("settles a request that failed, after the 500 it answered with", async () => {
    // Deferred work is not what the response depended on, so a render that
    // threw does not cancel the log line a middleware registered on the way in
    // — and the drain is still after the bytes, because the bytes here are the
    // listener's own 500 and the settle is in a `finally` below the `catch`.
    const order: Array<string> = [];
    const entry = bundle({
      middleware: [guard("/", () => after(() => order.push("logged")))],
    });
    const listen = nodeListener(
      async (request) => {
        await entry.runMiddleware(request);
        throw new Error("the page threw");
      },
      { beginRequest: entry.beginRequest },
    );

    const response = outgoing(order);
    const reported = [];
    const error = console.error;
    // eslint-disable-next-line no-console
    (console: $FlowFixMe).error = (value) => reported.push(value);
    try {
      await listen(incoming("GET", "/"), response);
    } finally {
      // eslint-disable-next-line no-console
      (console: $FlowFixMe).error = error;
    }

    expect(response.statusCode).toBe(500);
    expect(order).toEqual(["wrote 500 Internal Server Error\n", "response ended", "logged"]);
    expect(reported.length).toBe(1);
  });
});

describe("the compiled binary", () => {
  /** A render result of the shape the router hands back. */
  const rendered = (order: Array<string>) => ({
    status: 200,
    pipe: async (destination: $FlowFixMe) => {
      destination.write(Buffer.from("<p>home</p>", "utf8"));
      destination.end();
      order.push("document piped");
    },
    stream: () => ({ cancel: async () => {} }),
  });

  it("settles after `pipe` resolves, which is after the last byte", async () => {
    // The binary's own answer to "when has the response been sent": `pipe`
    // resolves on the last byte rather than the first, which is why the
    // handler awaits it and why `settle` is the line after.
    const order: Array<string> = [];
    const app = {
      ...bundle({
        middleware: [
          guard("/", () => {
            after(() => order.push("view recorded"));
          }),
        ],
      }),
      render: async () => rendered(order),
    };
    const handle = createHandler({
      app,
      assets: {},
      document: { scripts: [], styles: [], preloads: [] },
    });

    const response = {
      statusCode: 0,
      setHeader() {},
      write() {},
      end() {},
      destroy() {},
      on() {},
      once() {},
      off() {},
    };
    await handle(incoming("GET", "/"), response);

    expect(order).toEqual(["document piped", "view recorded"]);
  });
});
