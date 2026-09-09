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
import { prerenderedMayAnswer } from "./internal/draft.js";
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

/**
 * Whether a response the assets binding gave is a document.
 *
 * The `content-type` and nothing else. uf holds no index of what is behind the
 * binding — that is the platform's, and asking it is the fetch that just
 * happened — so the answer has to be read off what came back. A missing type
 * is not a document: the binding names one for every file it serves, and
 * guessing "document" for the one case where it did not would send an asset
 * through a render.
 */
function isDocument(response: Response): boolean {
  const type = response.headers.get("content-type");
  return type != null && type.toLowerCase().startsWith("text/html");
}

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
 *
 * Except a **document** answering a **draft** request, which is
 * `./internal/draft.js`'s `prerenderedMayAnswer` — the rule the other three
 * front doors apply, applied here too, or draft mode would be a property of
 * where an application was deployed. This door is the one that cannot decide
 * what a document is before it looks: the assets binding is the platform's and
 * uf holds no index of what is behind it, so the answer is read off the
 * response it gave. A `content-type` of `text/html` is a document; everything
 * else is a chunk, a stylesheet or an image, and those are the same bytes in
 * draft mode as out of it.
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
          if (asset.status !== 404) {
            if (prerenderedMayAnswer(request.headers.get("cookie")) || !isDocument(asset)) {
              return asset;
            }
            // The application will answer instead, so this body has no reader.
            // Cancelled rather than abandoned: a stream nobody drains is a
            // stream the runtime keeps open until the request is torn down.
            await asset.body?.cancel("uf: the application answers this instead");
          }
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

/**
 * The origin a scheduled invocation carries.
 *
 * A cron event has no request and therefore no host, and a handler that reads
 * `new URL(request.url).host` has to read *something*. A reserved-invalid name
 * rather than the deployment's own: `.invalid` can never resolve, so a handler
 * that echoes the origin into a link produces something obviously wrong rather
 * than something that looks right and points at the wrong place.
 */
export const SCHEDULED_ORIGIN: string = "https://cron.invalid";

/** The header naming the expression that fired, on a scheduled invocation. */
export const SCHEDULED_HEADER: string = "uf-scheduled";

/** What Cloudflare hands a `scheduled()` export. */
export type ScheduledEvent = {
  /** The expression that fired, exactly as `wrangler.json` spells it. */
  readonly cron: string,
  readonly scheduledTime?: number,
  ...
};

/**
 * Cloudflare's `scheduled()` export, over the same application `fetch` answers.
 *
 * The counterpart of [`createWorkerFetch`], and it makes the same promises in
 * the same order: `beginRequest`, the whole of the work inside `run`, one line
 * in the log, and `settle` handed to `ctx.waitUntil` where there is a `ctx`.
 * A schedule that skipped any of those would be work the application could not
 * see itself doing — no request context, so no `cookies()`, no `after()`, and
 * no request id in the line it leaves behind.
 *
 * # A schedule is a request the platform makes
 *
 * There is no second dispatcher here. `routes` maps the expression Cloudflare
 * fires to the route path that answers it, and this synthesises a `GET` to
 * that path through the application's own handler — so a scheduled run and a
 * `curl` of the same path are the same code, and a route handler needs to know
 * nothing about schedules to be one.
 *
 * `GET` because a cron has no body to send. `uf build` refuses a module that
 * declares a schedule and exports no `GET`, so a trigger that could not be
 * answered is a build that did not happen rather than a 405 nobody reads.
 *
 * # A trigger with no route
 *
 * Logged and dropped, not thrown. `wrangler.json` is a file a person can edit
 * after uf writes it, and a cron added there by hand is not a reason to fail an
 * invocation — but it is a reason to say so, because the alternative is a
 * schedule that fires into silence.
 */
export function createWorkerScheduled(options: {|
  readonly handle: (request: Request) => Promise<Response>,
  readonly beginRequest: (request: Request) => RequestLifecycle,
  readonly routes: { readonly [cron: string]: string },
|}): (event: ScheduledEvent, env: mixed, ctx?: ExecutionContext) => Promise<void> {
  const { handle, beginRequest, routes } = options;

  return async function scheduledFromWorker(
    event: ScheduledEvent,
    env: mixed,
    ctx?: ExecutionContext,
  ): Promise<void> {
    const path = routes[event.cron];
    if (path == null) {
      processLogger().warn("no route for this schedule", { cron: event.cron });
      return;
    }

    const request = new Request(`${SCHEDULED_ORIGIN}${path}`, {
      method: "GET",
      headers: { [SCHEDULED_HEADER]: event.cron },
    });
    const lifecycle = beginRequest(request);
    const started = Temporal.Now.instant();
    let status = 500;
    try {
      const response = await lifecycle.run(() => handle(request));
      status = response.status;
      // Discarded, with the reason on it. A body nobody drains is a stream
      // the runtime keeps open until the isolate is torn down, and a schedule
      // has no client to read one.
      await response.body?.cancel("uf: a scheduled invocation has no reader");
    } catch (error) {
      // Logged and swallowed: there is no caller above a scheduled invocation
      // to catch it, and a rejection here is a Worker error with no request
      // behind it — less legible than the line below.
      processLogger().error("schedule failed", { error, cron: event.cron, path });
    } finally {
      logRequest(processLogger(), {
        requestId: lifecycle.context.id,
        method: "GET",
        path,
        route: lifecycle.context.route,
        status,
        durationMs: elapsedMs(started),
      });
      if (ctx != null) {
        ctx.waitUntil(lifecycle.settle());
      } else {
        await lifecycle.settle();
      }
    }
  };
}
