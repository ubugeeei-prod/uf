// @flow
//
// `@uniflowed/server/fetch`: a built uf application as `Request` → `Response`.
//
// This is the seam every deploy adapter is written against, and it is one
// function: give it the module `uf build` wrote and the asset URLs from the
// client manifest, and it answers a request. It touches no filesystem, holds
// no Node types, and imports nothing from `@uniflowed/vite` — so the same
// handler runs inside `uf start`'s `node:http` loop, inside the `server.js`
// that `uf build --adapter node` writes, and inside a worker's `fetch` export.
//
// # Why it lives here and not where it was written
//
// It was `createApplicationHandler` in `@uniflowed/vite/internal/serve.js`,
// and `packages/server/serve.test.js` said what was wrong with that: "not a
// package export, deliberately — `internal/serve.js` is the seam a deploy
// adapter will need, and naming it in `exports` before one exists would be
// promising an interface nothing has used yet". An adapter exists now, and it
// may not import the package named after the bundler: the whole claim of
// deployable output is that the host needs neither Vite nor the toolchain that
// produced the build. So the seam moved to the package a deployment already
// links — `@uniflowed/server` — and `internal/serve.js` calls into it, which
// keeps `uf preview`, `uf start` and every adapter answering out of one
// function rather than out of copies that agree until they do not.
//
// `./standalone.js` is the one front door that does not come through here, and
// its own header says why: a compiled binary answers from bytes it carries
// rather than from a directory, and it writes into a Node response directly so
// that a document is not converted through a `Response` on its way out.
//
// # The route cache
//
// One `GET` at a time, and only where three things line up: the host passed a
// `cache` whose `route` is on (`rendering.cache.route` in `uf.config.js`), the
// render stated a lifetime with `cacheLife`, and the render did not read the
// request. All three are argued in `./cache.js`; what belongs here is the one
// cost that is this function's rather than the store's.
//
// **A cached route is buffered, and an uncached one is streamed.** The body has
// to be whole before it can be an entry, so the fill reads the document to its
// last byte before answering — which gives up the thing the streaming path
// exists for, on the fill. It buys two things back. The hit is the whole
// document at once with no render at all, which is faster than streaming a
// render; and the read of `requestStateReads` is only trustworthy *after* the
// last byte, because a component inside a `<Suspense>` boundary renders long
// after the shell resolved and `cookies()` in one of those is exactly the read
// that must stop the entry being stored. A cache that decided at the shell
// would cache a document whose tail was about one person.
//
// So the trade is per route and stated by the route: say nothing and stream as
// before, call `cacheLife` and buffer once per lifetime. `HEAD` never
// participates in either direction — it neither fills an entry nor reads one —
// because a `HEAD` is a request for a status and a length, and letting it fill
// a document cache would let a request that wants no body pay for one.
//
// # And what the host can do, which is not the same for all four
//
// `capabilities` is the other thing an adapter passes, and it exists because
// three of the four front doors can hold a connection open and one cannot. A
// route handler that returns an event stream or takes a WebSocket is correct
// on `uf start` and on a worker, and on a Lambda is a response buffered until
// the invocation times out — the same code, the same build, and a failure that
// appears only in the deployment nobody checked. So a host says what it is,
// once, where it is wired, and `./internal/capabilities.js` turns that into a
// refusal at the point somebody asks rather than a dropped connection later.
//
// A handler asks through `./socket.js`, `./events.js` and `./queue.js`; this
// function's only part in it is putting the answer on the request, beside the
// cache and for the same reason.
//
// # What it deliberately does not do
//
// Static files. A build's assets and its prerendered documents are the *host's*
// half — on a CDN-backed target they are not the application's job at all, and
// on a Node host they are a directory read, which is why `./node.js` has that
// half and this module has none of it. That split is the whole reason an
// adapter can be written for a worker: what is left after the files is exactly
// this function.

import { noStore } from "./cache.js";
import type { Application, DocumentAssets } from "./internal/application.js";
import type { CacheOptions, CacheOutcome } from "./internal/cache-store.js";
import { newScope, runInScope } from "./internal/cache-store.js";
import type { ServerCapabilities } from "./internal/capabilities.js";
import type { RequestContext } from "./internal/context.js";
import { currentContext } from "./internal/context.js";

export type { Application, DocumentAssets, RenderedDocument } from "./internal/application.js";

export type {
  CapabilityDefaults,
  CapabilityOptions,
  JobRecord,
  QueueBackend,
  ServerCapabilities,
  WebSocketUpgrade,
  WebSocketUpgrader,
} from "./internal/capabilities.js";

export {
  CapabilityRefusedError,
  CapabilityUnavailableError,
  assertCapable,
  capabilitiesFor,
} from "./internal/capabilities.js";

/** Everything the application half needs to answer a request. */
export type FetchHandlerOptions = {|
  /** The server bundle, as imported. */
  readonly app: Application,
  /** The script, stylesheet and preload URLs a rendered document references. */
  readonly document: DocumentAssets,
  /**
   * The cache this host installed, from `rendering.cache` in `uf.config.js`.
   *
   * Absent is the default and means no cache at all — every request renders,
   * exactly as before this option existed. A host that passes one is saying
   * two separate things with it, `route` and `fetch`, because the two switches
   * in the configuration are two switches.
   */
  readonly cache?: CacheOptions,
  /**
   * What this host can do, from the adapter that built it.
   *
   * Absent means nothing was said, which is what every request looked like
   * before this option existed and is treated as such: an event stream is
   * allowed, because a `Response` streams by default everywhere except where
   * somebody said otherwise, and an upgrade and a queue are refused, because
   * both are objects and there is no such object. `./internal/capabilities.js`
   * argues that asymmetry.
   */
  readonly capabilities?: ServerCapabilities,
|};

/** A whole document, as an entry: what a hit answers with without rendering. */
type CachedDocument = {|
  readonly status: number,
  readonly headers: { readonly [string]: string },
  readonly body: Uint8Array,
|};

/**
 * The application half: middleware, then route handlers, then rendering.
 *
 * Returns `null` for nothing, ever — a request that matches no handler and no
 * route is a rendered 404, because the renderer is what knows what the
 * project's `$not-found` page says.
 *
 * The order is the dev server's, and has to stay the dev server's: middleware
 * first, then server actions, then handlers for every method, because a
 * handler is the only thing that can answer a `POST` and it may also answer a
 * `GET` for a path that has no page. A page cannot answer a `POST`, so a
 * non-navigation that no handler claimed is a 404 rather than a rendered page
 * with a 200.
 *
 * `app.callAction` is between the two, and this is the function that puts it
 * on all four deploy targets at once: `handler.js` is byte-for-byte the same
 * file in the node, container, edge and serverless artefacts, so an action
 * endpoint that works here works in each of them or in none. It declines every
 * request that carries no action id and answers every request that carries
 * one, refusals included — so a `POST` naming an action never reaches a route
 * handler that happens to sit at the same path, and a request naming none
 * pays one header lookup. Called rather than tested for, for the reason
 * `app.runMiddleware` is: a server bundle without it is a `TypeError` on the
 * first request rather than an application whose actions quietly answer 404.
 *
 * Middleware above both, and not inside either: it guards a path, so it has to
 * run for a page, for a route handler, and for a path under it that matches
 * neither — `/dashboard/typo` is a 404 that the guard on `/dashboard` still
 * answers. `app.runMiddleware` is called rather than tested for, so a server
 * bundle without it is a `TypeError` on the first request instead of an
 * application whose auth check quietly stopped running once it was built.
 * That is the whole of ubugeeei-prod/uf#260, and every host that reaches this
 * function is one more place it could have happened.
 *
 * # It must be called inside a request, and does not begin one
 *
 * `after()` says "once the response has been sent", and this function has a
 * `Response` in hand rather than a response on the wire — for a streamed body
 * those are a document apart. So the host begins the request with
 * `app.beginRequest`, runs this inside `run`, and settles it after the bytes:
 * `./node.js`'s `nodeListener` does that for `uf start` and for the `server.js`
 * an adapter writes, and `@uniflowed/vite`'s `withRequest` does it for `uf dev`
 * and `uf preview`.
 *
 * A worker-shaped host is the one uf does not write, and it has the same two
 * halves to place. `settle` is what to hand `ctx.waitUntil` where there is one;
 * without one, the honest moment is when the response body stream closes — and
 * a runtime that tears the isolate down at that moment drops the callback,
 * which is worth saying out loud rather than leaving to be discovered.
 *
 * A caller that forgets is not left to discover *that*, at least:
 * `app.runMiddleware` refuses outside a request and names what establishes one.
 * See ubugeeei-prod/uf#389.
 *
 * # And the cache, if the host installed one
 *
 * `cache` is `rendering.cache` from `uf.config.js`, and it does two separate
 * things here. It is put on the request before the guard runs, so that a route
 * handler or a server action calling `revalidateTag()` reaches the store that
 * is answering this request; and, when `route` is on, a `GET` goes through
 * [`cachedDocument`] instead of the streaming path. Both halves are argued in
 * the module header and in `./cache.js`. With no `cache` at all this function
 * is what it has always been, one `AsyncLocalStorage.run` aside.
 */
export function createFetchHandler(
  options: FetchHandlerOptions,
): (request: Request) => Promise<Response> {
  const { app, cache, capabilities, document } = options;

  return async function handle(request: Request): Promise<Response> {
    // Before the guard, not after it. A route handler and a server action both
    // run inside `dispatch`, and `revalidateTag()` in one of them has to reach
    // the store that is answering this request — a mutation that invalidates
    // nothing is the failure this whole seam exists to prevent, and it would
    // be a silent one.
    const context = currentContext();
    if (context != null && cache != null) {
      context.cache = cache;
      // And, in the same breath, the durable half of whatever this request
      // does to that cache. A store with a provider starts its writes and does
      // not await them — a reader must not wait on a disk for a document it is
      // already holding — so something has to, or a host that stops the process
      // when the response is written drops them. That host is every serverless
      // one, and a shared cache is worth most exactly there. `settled()`
      // resolves immediately when nothing is outstanding, which is every
      // request on a memory-only store.
      context.deferred.push(() => cache.store.settled());
    }
    // And beside it, for the same reason and at the same moment: a handler
    // that upgrades a connection or queues work is inside `dispatch` too, and
    // what it can do is a fact about the host rather than about the route.
    if (context != null && capabilities != null) {
      context.capabilities = capabilities;
    }

    const guarded = await app.runMiddleware(request);
    if (guarded != null) return guarded;

    const acted = await app.callAction(request);
    if (acted != null) return acted;

    const handled = await app.dispatch(request);
    if (handled != null) return handled;

    const method = request.method.toUpperCase();
    if (method !== "GET" && method !== "HEAD") {
      return new Response(null, { status: 404 });
    }

    const url = new URL(request.url);
    const target = url.pathname + url.search;
    // Nothing better than the console here: this function is what a worker or
    // a serverless invocation wraps, and it has no terminal of its own. Losing
    // a boundary's exception entirely would be worse — it is the only trace a
    // page that failed after its first byte leaves anywhere.
    const onError = (error: mixed) => {
      console.error(error);
    };

    // A draft request goes down the streaming path whatever the cache says, and
    // it is a *bypass* rather than a refusal to store. The store already
    // refuses to keep a render that read `draftMode()`, which stops one
    // person's draft becoming everybody's page; this is the other direction,
    // and it is the one that makes draft mode mean anything: a cached entry is
    // an answer from before the draft existed, so serving it to an editor who
    // came to look at the draft answers a different question from the one they
    // asked. See ubugeeei-prod/uf#282.
    if (method === "GET" && cache != null && cache.route === true && context?.draft !== true) {
      return cachedDocument(app, cache, context, url, target, document, onError);
    }

    // Rendered inside a scope even with no cache in sight, so that a component
    // calling `cacheLife` is a component that states a lifetime nobody is
    // honouring rather than a component that throws. Turning the route cache
    // off must not change what an application is allowed to say.
    const result = await runInScope(newScope({ key: [] }), () =>
      app.render(target, document, { onError }),
    );
    const headers = new Headers(result.headers ?? {});
    headers.set("content-type", "text/html; charset=utf-8");
    // A `HEAD` gets the status and the headers and no body, which is what the
    // renderer cannot know to do for itself. The stream is cancelled rather
    // than dropped, so the render behind it stops instead of filling its queue
    // and waiting for a reader that is never coming.
    if (method === "HEAD") {
      await result.stream().cancel();
      return new Response(null, { status: result.status ?? 200, headers });
    }
    // The body is a stream, so the layouts and any `<Suspense>` fallback reach
    // the browser while the page they surround is still resolving.
    return new Response(result.stream(), { status: result.status ?? 200, headers });
  };
}

/**
 * Answer a `GET` from the route cache, filling it if it has to.
 *
 * The fill is the interesting half, and everything it refuses is refused for a
 * reason it can name:
 *
 * * **The render read the request.** `requestStateReads` is compared across the
 *   whole document rather than across the shell; see the module header.
 * * **The render did not answer 200.** A 404 or a 500 is a fact about this
 *   moment far more often than it is a fact about the URL, and a cached 500 is
 *   an outage that outlives its cause.
 * * **The render set a cookie.** A `Set-Cookie` in a shared entry is one
 *   person's session handed to the next reader. This is belt and braces — a
 *   render that set a cookie almost certainly read one first — and it is here
 *   because the cost of being wrong is not symmetric.
 *
 * A render that states no lifetime is refused by the store itself, which is
 * where "no lifetime, no entry" belongs: it is a property of the cache, not of
 * documents.
 */
async function cachedDocument(
  app: Application,
  cache: CacheOptions,
  context: RequestContext | null,
  url: URL,
  target: string,
  document: DocumentAssets,
  onError: (error: mixed) => void,
): Promise<Response> {
  const result = await cache.store.resolve(
    { key: ["route", "GET", url.pathname, url.search], path: url.pathname },
    async (): Promise<CachedDocument> => {
      const before = context?.requestStateReads ?? 0;
      const rendered = await app.render(target, document, { onError });
      const body = await drain(rendered.stream());
      const status = rendered.status ?? 200;
      const headers: { [string]: string } = { ...(rendered.headers ?? {}) };

      if (status !== 200) {
        noStore(`the render answered ${status}`);
      } else if (Object.keys(headers).some((name) => name.toLowerCase() === "set-cookie")) {
        noStore("the render set a cookie");
      } else if ((context?.requestStateReads ?? 0) > before) {
        noStore("the render read cookies(), headers() or draftMode()");
      }
      return { status, headers, body };
    },
  );

  const headers = new Headers(result.value.headers);
  headers.set("content-type", "text/html; charset=utf-8");
  // What this request did to the cache, in one word. It is the only way to see
  // a cache working from outside the process — a benchmark reads it, and so
  // does anybody wondering why a page is fast.
  headers.set("x-uf-cache", label(result.outcome));
  return new Response(result.value.body, { status: result.value.status, headers });
}

/** The header word for an outcome. */
function label(outcome: CacheOutcome): string {
  return match (outcome) {
    "hit" => "HIT",
    "stale" => "STALE",
    "coalesced" => "COALESCED",
    "miss" => "MISS",
    "uncacheable" => "BYPASS",
  };
}

/**
 * Every byte of `stream`, as one array.
 *
 * The chunks are collected and joined once rather than concatenated as they
 * arrive: a document is a few hundred chunks, and growing an array per chunk
 * copies the whole document per chunk.
 */
async function drain(stream: ReadableStream<Uint8Array>): Promise<Uint8Array> {
  const reader = stream.getReader();
  const chunks: Array<Uint8Array> = [];
  let total = 0;
  for (;;) {
    const step = await reader.read();
    if (step.done === true) break;
    const chunk = step.value;
    if (chunk != null) {
      chunks.push(chunk);
      total += chunk.byteLength;
    }
  }
  const body = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    body.set(chunk, at);
    at += chunk.byteLength;
  }
  return body;
}
