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
|};

/** Row 0 of a route's payload: the route, and the tree it rendered. */
export type FlightRoot = {|
  readonly route: RouteState,
  readonly tree: Node,
|};

/** The part of a resolved route that crosses to the browser. */
export function routeState(resolved: ResolvedRoute): RouteState {
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
