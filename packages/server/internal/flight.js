// @flow
//
// Internal to `@uniflowed/server`: answering a browser that is navigating.
//
// A page React Server Components rendered navigates by fetching the next
// route's Flight payload rather than its document, at `<route>/__uf.flight`
// (ubugeeei-prod/uf#519). The router spells that segment in
// `packages/router/internal/flight.js`, and this is a second spelling on
// purpose: `@uniflowed/router` depends on this package and not the other way
// round, so importing the router's from here would be a cycle.
// `../flight.test.js` imports both and compares them, which is what keeps two
// spellings one answer.
//
// One function for every front door — `./fetch.js` for `uf start`, `uf preview`
// and every adapter, `./standalone.js` for a compiled binary — so a payload is
// answered the same way wherever the build is served.

import type { Application } from "./application.js";

/** The last segment of a payload URL. */
export const FLIGHT_SEGMENT = "__uf.flight";

/**
 * The document a payload URL is for, or `null` for any other path.
 *
 * `/guide/__uf.flight` is `/guide`, and `/__uf.flight` is `/`.
 */
export function flightDocumentPath(pathname: string): string | null {
  const suffix = `/${FLIGHT_SEGMENT}`;
  if (!pathname.endsWith(suffix)) return null;
  const document = pathname.slice(0, -suffix.length);
  return document === "" ? "/" : document;
}

/**
 * The answer to a request for a route's payload, or `null` for any other one.
 *
 * `null` too for a server bundle with no `flight` — an application rendered
 * from its modules (`app.rsc: false`) — so such a request goes on to the
 * handlers and the renderer exactly as it always did, and its browser, which
 * never asks for a payload, is unaffected.
 *
 * Asked after the middleware, which sees the request as it was sent: a guard on
 * `/guide` covers `/guide/__uf.flight` by the same prefix rule that covers
 * everything else under it, so a payload is never an unguarded way to a page.
 * And before the route handlers, so a handler whose pattern happens to match
 * the payload URL — `/guide/:slug` matches `/guide/__uf.flight` — cannot answer
 * a navigation in place of the route.
 *
 * `within` is where a host puts what the render runs inside; `./fetch.js`
 * passes its cache scope, for the reason it renders a document inside one.
 */
export async function flightResponse(
  app: Application,
  request: Request,
  options: {|
    readonly onError: (error: mixed) => void,
    readonly within?: (body: () => Promise<FlightAnswer>) => Promise<FlightAnswer>,
  |},
): Promise<Response | null> {
  const render = app.flight;
  if (render == null) return null;
  const method = request.method.toUpperCase();
  if (method !== "GET" && method !== "HEAD") return null;
  const url = new URL(request.url);
  const document = flightDocumentPath(url.pathname);
  if (document == null) return null;

  const body = () => render(document + url.search, { onError: options.onError });
  const answered = await (options.within == null ? body() : options.within(body));
  // The exception the route resolved to its error boundary for, which never
  // reached `onError`: a loader that throws is caught while the route resolves,
  // before anything renders, and the boundary's payload is what answers. It is
  // reported here or nowhere — the response is a page, with a 500.
  if (answered.error != null) {
    options.onError(answered.error);
  }
  const headers = new Headers(answered.headers);
  const stream = answered.stream;
  // A redirect has no stream, and a `HEAD` wants none: the render behind a
  // stream nobody reads is cancelled rather than left to fill its queue.
  if (stream == null || method === "HEAD") {
    await stream?.cancel();
    return new Response(null, { status: answered.status, headers });
  }
  return new Response(stream, { status: answered.status, headers });
}

/** What `Application.flight` resolves with. */
type FlightAnswer = {|
  readonly status: number,
  readonly headers: { readonly [string]: string },
  readonly stream: ReadableStream<Uint8Array> | null,
  readonly error?: mixed,
|};
