// @flow
//
// Internal to `@uniflowed/router`: the names a Flight payload is spoken in.
//
// A route rendered by React Server Components travels as React's own Flight
// payload (ubugeeei-prod/uf#519). `react-server-dom-parcel` writes it on the
// server and reads it in the browser, and uf invents no format of its own —
// the one uf used to have for loader data, `./payload.js`'s numbered rows, was
// Flight-shaped precisely so that it could be replaced by this. What uf still
// decides is two things around the payload: what its root value *is*, and the
// URL a browser asks for one at. Every graph has to agree about both, so both
// are here.
//
// Pure data and string functions, and type imports only: the module graph
// that renders server components, the one that renders HTML and the browser's
// all import this file, and none of them may be handed anything the others
// could not evaluate.

import type { Node } from "react";

import type { RouteError, RouteParams, SearchParams } from "./routing.js";
import type { Metadata, ResolvedRoute } from "./resolve.js";

/**
 * What a route resolved to, as the browser holds it.
 *
 * Everything a hook reads, and nothing that is a module. A resolved route
 * carries the page, the layouts and the boundaries it loaded; none of those
 * crosses, because what they rendered is already in the payload beside this
 * value, and a component the browser does need crosses inside that tree as a
 * client reference. So `useRoute()` and `useLoaderData()` read this, and
 * `RouteView` renders the tree.
 *
 * `data` and `deferred` cross as Flight values. That is a narrower contract
 * than the JSON the loader's answer used to be embedded as: Flight carries a
 * `Date`, a `Map`, a `Set` and a promise, and it refuses a class instance by
 * name rather than turning it into `{}` the way `JSON.stringify` did. A thrown
 * error in `error` crosses the way React sends any error value — with its
 * message in development and replaced by React's own sentence in a build.
 */
export type RouteState = {|
  readonly pathname: string,
  readonly search: string,
  readonly path: string,
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  /** What the loader returned, once it has. `undefined` while `deferred` is set. */
  readonly data: mixed,
  /** The loader still running, as a promise the browser can `use`. */
  readonly deferred: ?Promise<mixed>,
  readonly metadata: Metadata,
  readonly viewTransition: ?string,
  readonly status: 200 | 401 | 403 | 404 | 500,
  readonly error: ?RouteError,
  readonly interception?: ?FlightInterception,
|};

/** Row 0 of a route's payload: the route, and the tree it rendered. */
export type FlightRoot = {|
  readonly route: RouteState,
  readonly tree: Node,
  /**
   * The build that rendered it, when the build has an id.
   *
   * In the payload rather than only in a response header, because the payload
   * of a prerendered route is a file and a host that serves files runs nothing
   * that could set one. A page on another build loads the document instead of
   * rendering this; see `./deployment.js`.
   */
  readonly deployment?: string | null,
|};

/** The intercepted URL a Flight payload rendered, and the page it rendered over. */
export type FlightInterception = {|
  readonly pathname: string,
  readonly search: string,
  readonly from: string,
|};

/** What the browser may send with a payload request. */
export type FlightFetchOptions = {|
  readonly interceptedFrom?: string,
  /**
   * Ask for the URL's not-found page rather than its route: the payload a
   * loader's `notFound()` would have answered with. What a route's error
   * boundary asks for when a hydrated server action called `notFound()`
   * (ubugeeei-prod/uf#1489).
   */
  readonly notFound?: boolean,
|};

/**
 * What fetching a route's payload turned into.
 *
 * Declared here rather than beside the fetch in `./flight-browser.js`, because
 * the router's navigation names this type and must not import React's Flight
 * client: `./runtime.js` is in every application's bundle, and that client is
 * only in the bundles of applications that render Server Components
 * (ubugeeei-prod/uf#992).
 */
export type FetchedFlight =
  | {|
      readonly kind: "flight",
      /** The route the server answered for: a redirect's target, when there was one. */
      readonly url: string,
      readonly root: Promise<FlightRoot>,
    |}
  | {|
      /**
       * The answer was not a payload: a redirect off this origin, or a host
       * that had no payload for the URL. The browser should load `url` as a
       * document.
       */
      readonly kind: "document",
      readonly url: string,
    |};

/**
 * `error` as it may cross into a Flight payload: a thrown value that is not an
 * `Error` becomes one.
 *
 * The route's error travels to the browser as a prop of the error page and as
 * part of the route state, and React serialises it the way it serialises any
 * prop. For an `Error` that is safe by React's own rule: a production payload
 * carries the digest and none of the message or the stack. A thrown value
 * that is *not* an `Error` gets no such rule. A string, or a plain object
 * such as an API client's `{ message, details, hint }`, would be serialised
 * as the data it is, and a query, a table name or a token in it would be
 * published to whoever asked for the page.
 *
 * So it is wrapped. The message is the value's `String()`, which `uf dev`
 * shows and which a production payload drops along with every other `Error`
 * message. The original value is still what the server reported: this is
 * applied only to the copy that crosses. `unauthorized` and `forbidden` carry
 * nothing and pass through as they are.
 */
export function crossableRouteError(error: ?RouteError): ?RouteError {
  if (error == null || error.kind !== "thrown" || error.error instanceof Error) {
    return error;
  }
  let text = "a value that is not an Error was thrown";
  try {
    text = String(error.error);
  } catch {
    // A value whose `toString` throws: the sentence above is all it gets.
  }
  return { kind: "thrown", error: new Error(text) };
}

/** The part of a resolved route that crosses to the browser. */
export function routeState(resolved: ResolvedRoute): RouteState {
  const interception = resolved.interception;
  return {
    pathname: resolved.pathname,
    search: resolved.search,
    path: resolved.path,
    params: resolved.params,
    searchParams: resolved.searchParams,
    data: resolved.data,
    deferred: resolved.deferred,
    metadata: resolved.metadata,
    viewTransition: resolved.viewTransition,
    status: resolved.status,
    error: resolved.error,
    interception:
      interception == null
        ? null
        : {
            pathname: interception.pathname,
            search: interception.search,
            from: interception.base.pathname + interception.base.search,
          },
  };
}

/**
 * The last path segment of the URL a route's payload is fetched from.
 *
 * A path rather than a header, for the two reasons a header would have been
 * wrong. A static host answers files, and a file has one name per URL: `uf
 * build` writes `dist/guide/__uf.flight` beside `dist/guide/index.html`, and a
 * host that has never heard of uf serves both. And a shared cache that ignores
 * `Vary` would hand a browser a payload where it asked for a document, which is
 * the class of bug `docs/security.md` records for RSC caches — a different URL
 * cannot be confused with the document's by anything that caches by URL.
 *
 * A segment rather than an extension, so that `/` and `/index` stay two URLs
 * with two payloads, and so that a middleware guarding `/dashboard` guards the
 * payload of `/dashboard` by the path rule it already has.
 */
export const FLIGHT_SEGMENT: string = "__uf.flight";

/** The content type a payload is answered with. */
export const FLIGHT_CONTENT_TYPE: string = "text/x-component";

/** The request header that carries the page an intercepted payload is rendered over. */
export const INTERCEPTED_FROM_HEADER: string = "uf-intercepted-from";

/**
 * The request header that asks for a URL's not-found payload instead of its
 * route. Its one value is `1`; see [`FlightFetchOptions`].
 */
export const NOT_FOUND_HEADER: string = "uf-not-found";

/**
 * The URL of `url`'s payload: its pathname with [`FLIGHT_SEGMENT`] appended,
 * and its query string unchanged.
 *
 * `url` is a path and query, as the router holds one; a hash never reaches a
 * server and is dropped.
 */
export function flightUrl(url: string): string {
  const hash = url.indexOf("#");
  const withoutHash = hash === -1 ? url : url.slice(0, hash);
  const question = withoutHash.indexOf("?");
  const pathname = question === -1 ? withoutHash : withoutHash.slice(0, question);
  const search = question === -1 ? "" : withoutHash.slice(question);
  const trimmed = pathname.replace(/\/+$/, "");
  return `${trimmed}/${FLIGHT_SEGMENT}${search}`;
}

/**
 * The document path a payload URL's pathname names, or `null` when the
 * pathname is not a payload URL.
 *
 * `/__uf.flight` is the root's; `/guide/__uf.flight` is `/guide`'s. Anything
 * else is `null`, and that includes a pathname that merely contains the segment
 * somewhere in the middle — `/__uf.flight/x` is a URL some route may own.
 */
export function documentPathOf(pathname: string): string | null {
  const suffix = `/${FLIGHT_SEGMENT}`;
  if (!pathname.endsWith(suffix)) {
    return null;
  }
  const document = pathname.slice(0, pathname.length - suffix.length);
  return document === "" ? "/" : document;
}
