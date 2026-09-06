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
// and `tests/library/serve.test.js` said what was wrong with that: "not a
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
// # What it deliberately does not do
//
// Static files. A build's assets and its prerendered documents are the *host's*
// half — on a CDN-backed target they are not the application's job at all, and
// on a Node host they are a directory read, which is why `./node.js` has that
// half and this module has none of it. That split is the whole reason an
// adapter can be written for a worker: what is left after the files is exactly
// this function.

import type { Application, DocumentAssets } from "./internal/application.js";

export type { Application, DocumentAssets, RenderedDocument } from "./internal/application.js";

/** Everything the application half needs to answer a request. */
export type FetchHandlerOptions = {|
  /** The server bundle, as imported. */
  readonly app: Application,
  /** The script, stylesheet and preload URLs a rendered document references. */
  readonly document: DocumentAssets,
|};

/**
 * The application half: middleware, then route handlers, then rendering.
 *
 * Returns `null` for nothing, ever — a request that matches no handler and no
 * route is a rendered 404, because the renderer is what knows what the
 * project's `_uf.not-found` page says.
 *
 * The order is the dev server's, and has to stay the dev server's: middleware
 * first, then handlers for every method, because a handler is the only thing
 * that can answer a `POST` and it may also answer a `GET` for a path that has
 * no page. A page cannot answer a `POST`, so a non-navigation that no handler
 * claimed is a 404 rather than a rendered page with a 200.
 *
 * Middleware above both, and not inside either: it guards a path, so it has to
 * run for a page, for a route handler, and for a path under it that matches
 * neither — `/dashboard/typo` is a 404 that the guard on `/dashboard` still
 * answers. `app.runMiddleware` is called rather than tested for, so a server
 * bundle without it is a `TypeError` on the first request instead of an
 * application whose auth check quietly stopped running once it was built.
 * That is the whole of ubugeeei-prod/uf#260, and every host that reaches this
 * function is one more place it could have happened.
 */
export function createFetchHandler(
  options: FetchHandlerOptions,
): (request: Request) => Promise<Response> {
  const { app, document } = options;

  return async function handle(request: Request): Promise<Response> {
    const guarded = await app.runMiddleware(request);
    if (guarded != null) return guarded;

    const handled = await app.dispatch(request);
    if (handled != null) return handled;

    const method = request.method.toUpperCase();
    if (method !== "GET" && method !== "HEAD") {
      return new Response(null, { status: 404 });
    }

    const url = new URL(request.url);
    const result = await app.render(url.pathname + url.search, document, {
      // Nothing better than the console here: this function is what a worker
      // or a serverless invocation wraps, and it has no terminal of its own.
      // Losing a boundary's exception entirely would be worse — it is the only
      // trace a page that failed after its first byte leaves anywhere.
      onError: (error: mixed) => {
        console.error(error);
      },
    });
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
