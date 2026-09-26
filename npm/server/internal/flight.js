// @flow
//
// Internal to `@uniflowed/server`: answering a browser that is navigating.
//
// A page React Server Components rendered navigates by fetching the next
// route's Flight payload rather than its document, at `<route>/__uf.flight`
// (ubugeeei-prod/uf#519). The router spells that segment in
// `npm/router/internal/flight.js`, and this is a second spelling on
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

/** The request header carrying the page an intercepted payload renders over. */
export const INTERCEPTED_FROM_HEADER = "uf-intercepted-from";

/**
 * The request header asking for a URL's not-found payload rather than its
 * route, sent with the value `1` by a page whose server action called
 * `notFound()` (ubugeeei-prod/uf#1489).
 */
export const NOT_FOUND_HEADER = "uf-not-found";

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
 * The payload URL's pathname for a document path: the inverse of
 * [`flightDocumentPath`]. `/guide` is `/guide/__uf.flight`, and `/` is
 * `/__uf.flight`.
 *
 * The trailing slashes are counted off rather than matched with a pattern,
 * because the path came from a request (docs/security.md rule 5).
 */
export function flightPath(documentPath: string): string {
  let end = documentPath.length;
  while (end > 0 && documentPath.charCodeAt(end - 1) === 47) {
    end -= 1;
  }
  return `${documentPath.slice(0, end)}/${FLIGHT_SEGMENT}`;
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
  // Only the half that renders a payload: a compiled binary's bundle is not a
  // whole `Application`, and this reads nothing else off it.
  app: { readonly flight?: Application["flight"], ... },
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

  const body = () =>
    render(document + url.search, {
      onError: options.onError,
      interceptedFrom: interceptedFrom(request),
      notFound: request.headers.get(NOT_FOUND_HEADER) === "1",
    });
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

function interceptedFrom(request: Request): string | void {
  const header = request.headers.get(INTERCEPTED_FROM_HEADER);
  if (header == null || !header.startsWith("/") || header.startsWith("//")) {
    return undefined;
  }
  const hash = header.indexOf("#");
  return hash === -1 ? header : header.slice(0, hash);
}

/** What `Application.flight` resolves with. */
type FlightAnswer = {|
  readonly status: number,
  readonly headers: { readonly [string]: string },
  readonly stream: ReadableStream<Uint8Array> | null,
  readonly error?: mixed,
|};
