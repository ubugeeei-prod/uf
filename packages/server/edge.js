// @flow
//
// `@uniflowed/server/edge`: the Cloudflare Workers half of serving a build.
//
// [`./fetch.js`] answers the application's half of a request and touches no
// filesystem, because that is the half a worker runs. This is everything that
// is left on a worker, and there is much less of it than there is on Node: a
// worker has no socket to take and no directory to read, so what remains is
// the request lifecycle and the one decision `./node.js` makes with `stat` —
// does a file the build already wrote answer this, or does the application.
//
// # Where the static half went
//
// To the platform. Workers static assets are uploaded beside the script and
// served by Cloudflare's own asset server; the Worker reaches them through the
// `ASSETS` binding declared in `wrangler.json`, which answers a `Request` with
// a `Response` exactly as `createStaticHandler` does — and answers `404` where
// that returns `null`, which is the only translation this module performs.
//
// The Worker asks *first*, and `uf build --adapter edge` writes
// `"run_worker_first": true` so that it can. That looks like the wrong way
// round — Cloudflare's default is to serve a matching asset without invoking
// the script at all, which is faster and resolves a file/handler collision the
// same way — but the default also puts the resolution order in a platform
// setting rather than in uf, and `tests/library/deploy.test.js` cannot drive a
// platform setting. Asking here means one order, written once, checked by the
// test that checks `uf start`'s. See ubugeeei-prod/uf#391.
//
// # Who owns the request, and the one thing a worker cannot promise
//
// `after()` says "once the response has been sent", and there is no such line
// in a worker: [`createWorkerFetch`] has a `Response` in hand, and the platform
// writes its body after the handler has returned. `ctx.waitUntil` is the
// documented way to keep the isolate alive for work that outlives the handler,
// so that is what `settle` is handed — which means an `after()` callback on
// this target begins when the response has been *decided* rather than when its
// last byte is out. For a streamed document those are a document apart. It is
// written down here, in `docs/app/reference/cli/_uf.page.mdx`, and it is the
// one behavioural difference between this target and the other three.
//
// `beginRequest` is passed in rather than imported, for the reason `./node.js`
// gives at length: the request lives in an `AsyncLocalStorage` belonging to a
// module instance, and the instance the application reads is the one bundled
// into the `handler.js` beside the generated `worker.js`. See
// ubugeeei-prod/uf#389.

import { Temporal } from "@uniflowed/core/temporal";
import type { CapabilityOptions, ServerCapabilities } from "./internal/capabilities.js";
import { assertCapable, capabilitiesFor } from "./internal/capabilities.js";
import type { RequestLifecycle } from "./internal/context.js";
import { elapsedMs, logRequest, processLogger } from "./log.js";

export type { RequestLifecycle } from "./internal/context.js";

/**
 * What a Worker can do, plus whatever the deployment supplied.
 *
 * Streams, and does not persist — the one split in this package where the two
 * flags disagree, which is why they are two flags. A Worker writes a body as
 * it is produced, so an event stream is exactly as good here as it is on Node.
 * It is also an isolate the platform may tear down the moment the response is
 * out, which is the whole reason `after()` on this target goes through
 * `ctx.waitUntil` rather than being awaited — so an in-process queue is work
 * pushed into something that may not be there to drain it, and
 * `./internal/capabilities.js` refuses one.
 *
 * `websocket` is the deployment's. Cloudflare's upgrade is `new WebSocketPair`,
 * `server.accept()` and a `Response` carrying `webSocket` — four lines, using
 * two globals nothing outside a real Worker can produce, which is the same
 * reason `./lambda.js` does not implement response streaming.
 */
export function edgeCapabilities(options?: CapabilityOptions): ServerCapabilities {
  return assertCapable(capabilitiesFor("edge", { stream: true, persistent: false }, options));
}

/**
 * The `ASSETS` binding, as much of it as this module uses.
 *
 * One method, declared structurally: a worker's bindings are supplied by the
 * runtime, and naming the whole of Cloudflare's `Fetcher` here would be this
 * package holding a copy of another project's types.
 */
export type AssetsBinding = {
  readonly fetch: (request: Request) => Promise<Response>,
  ...
};

/**
 * The `env` a Worker's `fetch` is called with.
 *
 * Inexact, because a project's own bindings — a KV namespace, a secret — are
 * in here too and are none of this module's business. `ASSETS` is optional
 * because a Worker deployed without an assets directory has no such binding,
 * and the honest answer for that deployment is "the application answers
 * everything" rather than a `TypeError` on the first request.
 */
export type EdgeEnvironment = {
  readonly ASSETS?: AssetsBinding,
  ...
};

/**
 * The `ctx` a Worker's `fetch` is called with.
 *
 * `waitUntil` only. `passThroughOnException` exists and is deliberately not
 * used: it serves the origin's response when the script throws, and a Worker
 * that *is* the origin has nothing to pass through to.
 */
export type ExecutionContext = {
  readonly waitUntil: (promise: Promise<mixed>) => mixed,
  ...
};

/** Everything the worker half needs to answer a request. */
export type WorkerHandlerOptions = {|
  /** The application, from the generated `handler.js`. */
  readonly handle: (request: Request) => Promise<Response>,
  /** That same module's `beginRequest`; see the header. */
  readonly beginRequest: (request: Request) => RequestLifecycle,
|};

/**
 * A built uf application as a Worker's `fetch`.
 *
 * Static assets first, then the application — the order `uf preview` cannot
 * deviate from and therefore the order every other front door matches. The
 * asset lookup is skipped for anything that is not a `GET` or a `HEAD`, which
 * is what `createStaticHandler` does and for the same reason: a `POST` to a
 * path that happens to have a file under it belongs to a route handler.
 *
 * A `404` from the assets binding means "no such asset", not "the site has no
 * such page": `wrangler.json` sets `"not_found_handling": "none"` so that the
 * miss falls through to here, and the 404 a visitor sees is the project's own
 * `_uf.not-found` rendered by the application. Any other status is the asset's
 * answer and is returned as it stands.
 */
export function createWorkerFetch(
  options: WorkerHandlerOptions,
): (request: Request, env: EdgeEnvironment, ctx?: ExecutionContext) => Promise<Response> {
  const { handle, beginRequest } = options;

  return async function fetchFromWorker(
    request: Request,
    env: EdgeEnvironment,
    ctx?: ExecutionContext,
  ): Promise<Response> {
    const lifecycle = beginRequest(request);
    const started = Temporal.Now.instant();
    // Declared out here so the `finally` can say what this request answered. A
    // worker has no terminal at all, so the line it leaves behind is the only
    // account of it there will ever be.
    let status = 500;
    try {
      const response = await lifecycle.run(async () => {
        const assets = env?.ASSETS;
        const method = request.method.toUpperCase();
        if (assets != null && (method === "GET" || method === "HEAD")) {
          const asset = await assets.fetch(request);
          if (asset.status !== 404) return asset;
        }
        return await handle(request);
      });
      status = response.status;
      return response;
    } catch (error) {
      // The same 500 `./node.js`'s `nodeListener` writes, and for the same
      // reasons: the body must not carry the stack, because the body goes to
      // whoever asked, and the log is where the operator is already looking.
      // Without this the answer would be Cloudflare's own error page, which is
      // a different answer from `uf start`'s for the same failure — and the
      // whole claim of the seam is that there is one answer.
      processLogger().error("request failed", { error });
      return new Response("500 Internal Server Error\n", {
        status: 500,
        headers: { "content-type": "text/plain; charset=utf-8" },
      });
    } finally {
      logRequest(processLogger(), {
        requestId: lifecycle.context.id,
        method: request.method.toUpperCase(),
        path: new URL(request.url).pathname,
        route: lifecycle.context.route,
        status,
        durationMs: elapsedMs(started),
      });
      // Scheduled rather than awaited: awaiting it here would hold the
      // response back until every `after()` callback had finished, which is
      // the opposite of what `after()` is for. Where there is no `ctx` — a
      // test, or a host that calls this directly — it is awaited, because
      // dropping the promise would lose both the work and its rejection.
      if (ctx != null) {
        ctx.waitUntil(lifecycle.settle());
      } else {
        await lifecycle.settle();
      }
    }
  };
}
