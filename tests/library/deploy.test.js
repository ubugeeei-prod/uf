// @flow
//
// What `uf build --adapter node` writes, at the seam it writes it against.
//
// The directory that command produces is three things: `handler.js`, which is
// [`createFetchHandler`] over the project's server bundle; `server.js`, which
// is [`serve`] over that handler and a `static/` directory; and the copy of
// the build. This file is about the functions, because they are what a second
// adapter reuses — `crates/uf_cli/tests/vite.rs` is where the directories
// themselves are built, copied somewhere with no `node_modules` above them,
// and asked.
//
// There are five adapters that run an application, and four host halves
// rather than one: `@uniflowed/server/node` for `node` and `container`,
// `@uniflowed/server/bun` for `bun`, `@uniflowed/server/edge` for a
// Cloudflare Worker, and `@uniflowed/server/lambda` for an AWS Lambda
// invocation. Each of them wraps the *same* `createFetchHandler`, and the last
// `describe` in this file is what says so out loud.
//
// Three of the four are driven here. `@uniflowed/server/bun` is not, and
// cannot be: `Bun.serve` and `Bun.file` are not defined in the host running
// this suite, which is Node. What that module does *not* have of its own is
// the part that would be worth driving — the path policy is
// `@uniflowed/server/internal/static.js`, shared with the Node half and
// therefore already under every static assertion below.
// `crates/uf_cli/tests/vite.rs` is where its socket runs, on Bun.
//
// `--adapter static` is the sixth and is not here, because it has no handler
// to reuse: a static host runs nothing, so that target's whole implementation
// is deciding whether the project may be served that way at all. That decision
// is Rust — it needs the route table and what the prerender wrote — and lives
// in `crates/uf_cli/src/commands/deploy/static_host.rs`.
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
// `the front doors give one answer` below drives all of them over one fixture
// and compares. It is the test that fails if a future adapter decides to be
// cleverer than `uf preview` is allowed to be.
//
// **Five doors, not four.** The fifth is `@uniflowed/server/standalone`, which
// `uf build --compile` links into a single file. It is the one copy of the
// order whose code genuinely cannot be shared — a binary has no `dist/` to
// read, so its "a file the build already wrote" is a lookup in an embedded map
// rather than a `stat` — and that is exactly why its *answer* has to be
// checked here rather than only against itself in `standalone.test.js`. It was
// outside this comparison until ubugeeei-prod/uf#391 said so.
//
// # What these tests do not establish
//
// That any of this runs on Cloudflare or on AWS. Nothing here has: the sandbox
// uf is developed in cannot bind a socket and has no credentials for any
// cloud. What is driven is the module an adapter's generated entry imports,
// with the platform's own contract standing in for the platform — an `ASSETS`
// binding that answers 404 on a miss, because that is what
// `not_found_handling: "none"` does, and a payload format 2.0 event, because
// that is what a Lambda Function URL sends. A passing test here is a statement
// about uf, not about a deployment.

import { Buffer } from "node:buffer";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";

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

import { createWorkerFetch } from "@uniflowed/server/edge";
import { createFetchHandler } from "@uniflowed/server/fetch";
import { beginRequest } from "@uniflowed/server/host";
import { createLambdaHandler } from "@uniflowed/server/lambda";
import { createServeHandler, createStaticHandler } from "@uniflowed/server/node";
import { createHandler as createStandaloneHandler } from "@uniflowed/server/standalone";

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
    // The real one, because a bundle's own is what a host must be handed: the
    // request lives in an `AsyncLocalStorage` belonging to a module instance,
    // and `handler.js` re-exports this beside `fetch` so `server.js` has the
    // right copy to pass. `request-lifecycle.test.js` owns what it is for.
    beginRequest,
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
      return {
        status: answer.status,
        pipe: (destination: { write: (chunk: string) => mixed, end: () => mixed, ... }) => {
          destination.write(answer.html);
          destination.end();
        },
        stream: () =>
          new ReadableStream({
            start(controller: StreamController) {
              controller.enqueue(new TextEncoder().encode(answer.html));
              controller.close();
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

/**
 * The `ASSETS` binding a Worker is given, as `wrangler.json` configures it.
 *
 * Not a mock of Cloudflare, and it matters that it is not: the contract this
 * stands in for is one line of the platform's documentation — the binding
 * answers a `Request` with a `Response`, and with a `404` where there is no
 * such asset, which is what `"not_found_handling": "none"` means. Everything
 * else about it is `createStaticHandler`, so the file that answers here is the
 * same file `uf start` answers with.
 *
 * The one thing it deliberately does *not* copy is the method check.
 * Cloudflare's asset server has its own answer for a `POST`; the Worker never
 * asks it one, and `createWorkerFetch` skipping the lookup is the assertion in
 * `asks the assets binding for nothing but a GET or a HEAD` below.
 */
function assetsBinding(root: string) {
  const serveStatic = createStaticHandler({ root });
  return {
    fetch: async (request: Request): Promise<Response> =>
      (await serveStatic(request)) ?? new Response("not found", { status: 404 }),
  };
}

/** An `ExecutionContext` that remembers what it was asked to wait for. */
function executionContext() {
  const pending: Array<Promise<mixed>> = [];
  return {
    pending,
    waitUntil: (promise: Promise<mixed>) => {
      pending.push(promise);
    },
  };
}

/** What Lambda sends: payload format 2.0, from the request the others get. */
async function eventFor(request: Request) {
  const url = new URL(request.url);
  const headers: { [string]: string } = { host: url.host };
  const cookies: Array<string> = [];
  for (const [name, value] of request.headers) {
    if (name.toLowerCase() === "cookie") {
      cookies.push(...value.split("; "));
      continue;
    }
    headers[name] = value;
  }
  const method = request.method.toUpperCase();
  const body = method === "GET" || method === "HEAD" ? undefined : await request.text();
  return {
    version: "2.0",
    rawPath: url.pathname,
    rawQueryString: url.search.replace(/^\?/, ""),
    cookies,
    headers,
    body,
    isBase64Encoded: false,
    requestContext: { domainName: url.host, http: { method, path: url.pathname } },
  };
}

describe("the Cloudflare front door an adapter's worker.js runs", () => {
  it("serves an asset the build wrote, before the application sees it", async () => {
    const staticDir = directoryWith({
      "index.html": "<!doctype html><p>home</p>",
      "assets/client.js": "console.log(1);",
    });
    const handle = createWorkerFetch({
      handle: createFetchHandler({ app: appWith({}), document: assets }),
      beginRequest,
    });

    const ctx = executionContext();
    const asset = await handle(
      request("/assets/client.js"),
      { ASSETS: assetsBinding(staticDir) },
      ctx,
    );
    expect(asset.status).toBe(200);
    expect(await asset.text()).toBe("console.log(1);");
  });

  it("falls through to the application when the binding says 404", async () => {
    const staticDir = directoryWith({ "index.html": "<!doctype html><p>home</p>" });
    const handle = createWorkerFetch({
      handle: createFetchHandler({ app: appWith({}), document: assets }),
      beginRequest,
    });

    // The 404 the binding answers means "no such asset", and the 404 a visitor
    // is owed is the project's own not-found page. A worker that returned the
    // binding's would be a deployment whose 404 came from Cloudflare.
    const response = await handle(
      request("/posts/hello"),
      { ASSETS: assetsBinding(staticDir) },
      executionContext(),
    );
    expect(response.status).toBe(200);
    expect(await response.text()).toContain("/posts/hello");
  });

  it("asks the assets binding for nothing but a GET or a HEAD", async () => {
    const asked: Array<string> = [];
    const handle = createWorkerFetch({
      handle: createFetchHandler({
        app: appWith({ handler: () => Response.json({ from: "the handler" }) }),
        document: assets,
      }),
      beginRequest,
    });

    const env = {
      ASSETS: {
        fetch: async (incoming: Request): Promise<Response> => {
          asked.push(incoming.method);
          return new Response("not found", { status: 404 });
        },
      },
    };
    await handle(request("/api/health"), env, executionContext());
    await handle(request("/api/health", { method: "POST", body: "{}" }), env, executionContext());

    // A `POST` to a path that happens to have a file under it belongs to a
    // route handler, which is the rule `createStaticHandler` follows on Node.
    expect(asked).toEqual(["GET"]);
  });

  it("hands the request's own settle to ctx.waitUntil rather than awaiting it", async () => {
    const handle = createWorkerFetch({
      handle: createFetchHandler({ app: appWith({}), document: assets }),
      beginRequest,
    });

    // The one behavioural difference between this target and the other three,
    // and it is asserted rather than left to the header: a worker has no line
    // at which the last byte went out, so `after()` begins when the response
    // has been decided. Losing the promise entirely would be worse — the work
    // would be dropped with the isolate.
    const ctx = executionContext();
    await handle(request("/"), {}, ctx);
    expect(ctx.pending.length).toBe(1);
    await Promise.all(ctx.pending);
  });

  it("answers a handler that threw with the same bare 500 the Node listener writes", async () => {
    const handle = createWorkerFetch({
      handle: createFetchHandler({
        app: appWith({
          handler: () => {
            throw new Error("the database is on fire at 0x7f4a2b10");
          },
        }),
        document: assets,
      }),
      beginRequest,
    });

    // Without this the answer would be Cloudflare's own error page, which is a
    // different answer from `uf start`'s for the same failure. The body must
    // not carry the stack: it goes to whoever asked, and the console is where
    // the operator is already looking.
    const response = await handle(request("/api/health"), {}, executionContext());
    expect(response.status).toBe(500);
    expect(await response.text()).not.toContain("0x7f4a2b10");
  });

  it("answers without an ASSETS binding, for a Worker deployed with no assets", async () => {
    const handle = createWorkerFetch({
      handle: createFetchHandler({ app: appWith({}), document: assets }),
      beginRequest,
    });

    const response = await handle(request("/anything"), {}, executionContext());
    expect(response.status).toBe(200);
  });
});

describe("the AWS front door an adapter's lambda.js runs", () => {
  it("refuses an event that is not payload format 2.0, by name", async () => {
    const handler = createLambdaHandler({
      handle: createFetchHandler({ app: appWith({}), document: assets }),
      beginRequest,
    });

    // A REST API sends `httpMethod` and `path`, and reading those fields off
    // this event would give `undefined` — a request for `/undefined` with a
    // method of `undefined`, answered 404, on every invocation. Naming the
    // format is the difference between a bug report and a fix.
    const failure = await handler({ httpMethod: "GET", path: "/" }).then(
      () => null,
      (error: mixed) => String(error),
    );
    expect(failure).toContain("payload format 2.0");
  });

  it("puts every Set-Cookie in `cookies` and none of them in `headers`", async () => {
    const headers = new Headers();
    headers.append("set-cookie", "a=1; Path=/");
    headers.append("set-cookie", "b=2; Path=/");
    const handler = createLambdaHandler({
      handle: createFetchHandler({
        app: appWith({ handler: () => new Response("ok", { headers }) }),
        document: assets,
      }),
      beginRequest,
    });

    // Iterating `Headers` joins repeated fields with a comma, and for this one
    // field that is a corruption rather than a spelling: two cookies become one
    // header nothing can parse back apart, and a session is silently lost.
    const result = await handler(await eventFor(request("/api/session")));
    expect(result.cookies).toEqual(["a=1; Path=/", "b=2; Path=/"]);
    expect(Object.keys(result.headers)).not.toContain("set-cookie");
  });

  it("gives the application the cookies the event carried separately", async () => {
    const handler = createLambdaHandler({
      handle: createFetchHandler({
        app: appWith({
          handler: (incoming: Request) => new Response(incoming.headers.get("cookie") ?? "none"),
        }),
        document: assets,
      }),
      beginRequest,
    });

    // Format 2.0 delivers cookies in an array and not in a header. An
    // application reading `cookies()` would otherwise see none of them, which
    // is exactly the kind of difference between a deployment and `uf start`
    // this seam exists to prevent.
    const event = { ...(await eventFor(request("/api/session"))), cookies: ["a=1", "b=2"] };
    const result = await handler(event);
    expect(result.body).toBe("a=1; b=2");
  });

  it("base64-encodes a body whose media type is not text", async () => {
    const bytes = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    const handler = createLambdaHandler({
      handle: createFetchHandler({
        app: appWith({
          handler: (incoming: Request) =>
            new URL(incoming.url).pathname === "/logo.png"
              ? new Response(bytes, { headers: { "content-type": "image/png" } })
              : null,
        }),
        document: assets,
      }),
      beginRequest,
    });

    // The default is base64 rather than text, because getting it the other way
    // round corrupts an image silently: a byte that is not valid UTF-8 becomes
    // U+FFFD on the way out and nothing anywhere reports it.
    const result = await handler(await eventFor(request("/logo.png")));
    expect(result.isBase64Encoded).toBe(true);
    expect(result.body).toBe("iVBORw0KGgo=");

    // And the other way round for the rendered document, which is the whole
    // reason the default is base64: `text/html` is text, so it goes out as a
    // string rather than as a payload a browser has to be told to decode.
    const rendered = await handler(await eventFor(request("/")));
    expect(rendered.isBase64Encoded).toBe(false);
    expect(rendered.body).toContain("<!doctype html>");
  });

  it("answers a handler that threw with a 500 rather than failing the invocation", async () => {
    const handler = createLambdaHandler({
      handle: createFetchHandler({
        app: appWith({
          handler: () => {
            throw new Error("the database is on fire at 0x7f4a2b10");
          },
        }),
        document: assets,
      }),
      beginRequest,
    });

    // A rejected invocation is a 502 from API Gateway, which is a different
    // answer from `uf start`'s 500 for the same failure — and the stack stays
    // in CloudWatch rather than going out in the body.
    const result = await handler(await eventFor(request("/api/health")));
    expect(result.statusCode).toBe(500);
    expect(result.body).not.toContain("0x7f4a2b10");
  });

  it("serves the package's own static half before the application", async () => {
    const staticDir = directoryWith({ "guide/index.html": "<!doctype html><p>guide</p>" });
    const handler = createLambdaHandler({
      handle: createFetchHandler({ app: appWith({}), document: assets }),
      beginRequest,
      staticDir,
    });

    const result = await handler(await eventFor(request("/guide")));
    expect(result.statusCode).toBe(200);
    expect(result.body).toContain("guide");
  });
});

describe("the front doors", () => {
  it("give one answer, including where a file and a handler collide", async () => {
    const built = {
      "index.html": "<!doctype html><p>home</p>",
      // A path that is *both* a prerendered document and a route handler. The
      // router allows a handler beside a page in one directory, so this is a
      // project somebody can write, and it is the only request whose answer
      // depends on which implementation is running.
      "api/health/index.html": "<!doctype html><p>prerendered health</p>",
    };
    const distDir = directoryWith(built);
    const app = appWith({
      handler: (request: Request) =>
        new URL(request.url).pathname.startsWith("/api/health")
          ? Response.json({ from: "the handler" })
          : null,
    });

    const started = createViteServeHandler({ entry: app, assets, distDir });
    const handle = createFetchHandler({ app, document: assets });
    const deployed = createServeHandler({ staticDir: distDir, handle });
    const worker = createWorkerFetch({ handle, beginRequest });
    const invoked = createLambdaHandler({ handle, beginRequest, staticDir: distDir });
    // The fifth door, and the only one that is not `createFetchHandler` behind
    // a socket. A compiled binary has no `dist/` to read, so
    // `@uniflowed/server/standalone` carries its own copy of the order — over
    // an embedded map instead of a directory, which is why the code cannot be
    // shared and why the *answer* has to be checked instead. ubugeeei-prod/uf#391
    // named it as the copy that was outside this comparison.
    const compiled = createStandaloneHandler({ app, assets: embedded(built), document: assets });

    // Five front doors, one question each. `uf start` is the reference,
    // because it is the one a person checks a build with; each of the other
    // four is a deployment, and a deployment that answered differently from
    // the command it was checked with is the whole failure this file exists to
    // catch. The Worker's static half is the platform's, standing in as an
    // `ASSETS` binding; the Lambda's is the package's own copy; the binary's is
    // the bytes inside it.
    const doors = {
      "uf start": async (url: string, init?: mixed) => {
        const response = await started(request(url, init));
        return `${String(response.status)} ${await response.text()}`;
      },
      "adapter node": async (url: string, init?: mixed) => {
        const response = await deployed(request(url, init));
        return `${String(response.status)} ${await response.text()}`;
      },
      "adapter edge": async (url: string, init?: mixed) => {
        const response = await worker(
          request(url, init),
          { ASSETS: assetsBinding(distDir) },
          executionContext(),
        );
        return `${String(response.status)} ${await response.text()}`;
      },
      "adapter serverless": async (url: string, init?: mixed) => {
        const result = await invoked(await eventFor(request(url, init)));
        return `${String(result.statusCode)} ${result.body}`;
      },
      "uf build --compile": async (url: string, init?: mixed) => {
        const response = nodeResponse();
        const method = String(init?.method ?? "GET");
        const body = String(init?.body ?? "");
        await compiled(
          {
            method,
            url,
            headers: { host: "localhost" },
            // The handler hands a non-`GET` body straight to `Request`, so what
            // stands in for the socket has to be async-iterable the way an
            // `IncomingMessage` is. A `GET` carries none, and passing one would
            // be rejected before the comparison could ask anything.
            ...(method === "GET" || method === "HEAD"
              ? {}
              : {
                  // eslint-disable-next-line
                  [Symbol.asyncIterator]: async function* iterate() {
                    yield new TextEncoder().encode(body);
                  },
                }),
          },
          response,
        );
        return `${String(response.statusCode)} ${response.body()}`;
      },
    };

    for (const [url, init] of [
      ["/", undefined],
      ["/api/health", undefined],
      ["/api/health", { method: "POST", body: "{}" }],
      ["/posts/hello", undefined],
      ["/definitely-not-a-page", undefined],
      ["/../uf-deploy-secret", undefined],
    ]) {
      const said = `${String(init?.method ?? "GET")} ${String(url)}`;
      const reference = await doors["uf start"](String(url), init);
      for (const name of [
        "adapter node",
        "adapter edge",
        "adapter serverless",
        "uf build --compile",
      ]) {
        const answered = await doors[name](String(url), init);
        expect(`${name} ${said}: ${answered}`).toBe(`${name} ${said}: ${reference}`);
      }
    }
  });
});

/**
 * The same files, as `uf build --compile` embeds them.
 *
 * Built from the fixture's own literal rather than by reading back the
 * directory the other doors serve, so the comparison is about the *order* and
 * not about the fixture — and so that "the same bytes" is something a reader
 * can see rather than something a walk has to be trusted to have done.
 */
function embedded(files: { [string]: string }): {
  [string]: { readonly type: string, readonly body: string },
} {
  const out: { [string]: { readonly type: string, readonly body: string } } = {};
  for (const key of Object.keys(files)) {
    out[key] = {
      type: key.endsWith(".html") ? "text/html; charset=utf-8" : "application/octet-stream",
      body: Buffer.from(files[key], "utf8").toString("base64"),
    };
  }
  return out;
}

/**
 * The `ServerResponse` members `@uniflowed/server/standalone` touches.
 *
 * Only what this comparison needs — a status and a body. The pacing and
 * cancellation halves of that contract are `standalone.test.js`'s subject and
 * have their own, fuller, recorder there; a second copy of it here would be a
 * second thing to keep in step for a question this file does not ask.
 */
function nodeResponse() {
  const chunks = [];
  const headers: { [string]: string } = {};
  const collect = (chunk: Uint8Array | string) => {
    chunks.push(typeof chunk === "string" ? Buffer.from(chunk, "utf8") : Buffer.from(chunk));
  };
  return {
    statusCode: 0,
    headers,
    setHeader(name: string, value: string) {
      headers[name.toLowerCase()] = value;
    },
    // The handler attaches `drain` and `close` listeners to pace a body. This
    // comparison never fills a socket, so there is nothing to pace and nothing
    // to fire; `standalone.test.js` is where those are driven. Returning
    // `undefined` rather than the response is what keeps this a plain object
    // literal rather than one that refers to itself.
    on() {},
    once() {},
    off() {},
    write(chunk: Uint8Array | string) {
      collect(chunk);
      return true;
    },
    end(chunk?: Uint8Array | string) {
      if (chunk != null) {
        collect(chunk);
      }
    },
    destroy() {},
    body(): string {
      return Buffer.concat(chunks).toString("utf8");
    },
  };
}
