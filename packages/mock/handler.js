// @flow
//
// Declaring what answers a request.
//
// A separate module from `registry.js` because a handler is a *value*, not a
// registration: `http.get("/users/:id", …)` builds one and nothing has been
// installed, nothing is intercepting, and no global has been touched. That is
// what lets a project keep its default handlers in a file of their own, hand
// the same array to two suites, and write a helper that returns handlers.
//
//   const handlers = [
//     http.get("/users/:id", ({ params }) => HttpResponse.json({ id: params.id })),
//     http.post("/users", async ({ request }) => {
//       const body = await request.json();
//       return HttpResponse.json({ id: "1", ...body }, { status: 201 });
//     }),
//   ];
//
// # Why `http.get` and not `get`
//
// The contract this package was sketched with exported bare `get` and `post`.
// They are gone, and not for style: a test file imports from half a dozen
// packages, and `get` is a name that `@uniflowed/state`, a query client, a
// store and the test's own helpers all have a claim on. `http` says which
// `get` this is at every call site, and it is the spelling the roadmap item
// names, so a suite ported from MSW reads unchanged.

import type { PathParams, PathPattern } from "./internal/path.js";
import { compilePattern, matchPattern } from "./internal/path.js";

/** What a resolver is told about the request it is answering. */
export type ResolverInfo = {|
  /** The request itself, with its body unread. */
  readonly request: Request,
  /** What the path pattern captured. */
  readonly params: PathParams,
  /** The query string, already parsed. */
  readonly query: URLSearchParams,
|};

/**
 * What a resolver may return.
 *
 * `void` and `null` both mean "not this one after all": the next matching
 * handler is tried, and if there is none the request is unhandled. That is how
 * a handler makes its decision on the *body* rather than the path — answering
 * only the requests whose payload it recognises and leaving the rest alone.
 *
 * `passthrough()` is also a `Response`, which is why this union has three
 * members and not four; `response.js` says why it is spelled that way.
 */
export type ResolverResult = Response | void | null;

/** The function a handler runs when its method and path match. */
export type Resolver = (info: ResolverInfo) => ResolverResult | Promise<ResolverResult>;

/** Options a handler may be declared with. */
export type HandlerOptions = {|
  /**
   * Answer at most one request, then step aside.
   *
   * The reason it exists is the retry test: two handlers for the same path, the
   * first failing once and the second succeeding, is how a suite states "it
   * retries" without a counter in a closure.
   */
  readonly once?: boolean,
|};

/**
 * One declared answer: a method, a path, and what to do about it.
 *
 * Frozen and inert. Nothing here knows about interception, and the same handler
 * may be in two registries at once.
 */
export type MockHandler = {|
  /** An upper-cased HTTP method, or `"ALL"` for every method. */
  readonly method: string,
  /** The path as it was written, for a failure message. */
  readonly path: string,
  readonly pattern: PathPattern,
  readonly resolve: Resolver,
  readonly once: boolean,
|};

/** The shape of every `http.*` function. */
type Route = (path: string, resolver: Resolver, options?: HandlerOptions) => MockHandler;

/** The methods `http` covers. */
export type Http = {|
  readonly all: Route,
  readonly get: Route,
  readonly post: Route,
  readonly put: Route,
  readonly patch: Route,
  readonly delete: Route,
  readonly head: Route,
  readonly options: Route,
|};

function declare(method: string, path: string, resolver: Resolver, options?: HandlerOptions) {
  return {
    method,
    path,
    pattern: compilePattern(path),
    resolve: resolver,
    once: options?.once === true,
  };
}

/**
 * Handlers by HTTP method.
 *
 * `http.all` matches any method, which is the right tool for a passthrough rule
 * or a catch-all that fails loudly; everything else names one method, because a
 * `GET` handler quietly answering a `POST` is a test that passes for the wrong
 * reason.
 */
export const http: Http = {
  all: (path, resolver, options) => declare("ALL", path, resolver, options),
  get: (path, resolver, options) => declare("GET", path, resolver, options),
  post: (path, resolver, options) => declare("POST", path, resolver, options),
  put: (path, resolver, options) => declare("PUT", path, resolver, options),
  patch: (path, resolver, options) => declare("PATCH", path, resolver, options),
  delete: (path, resolver, options) => declare("DELETE", path, resolver, options),
  head: (path, resolver, options) => declare("HEAD", path, resolver, options),
  options: (path, resolver, options) => declare("OPTIONS", path, resolver, options),
};

/**
 * Whether this handler is the one for a request, and what its path captured.
 *
 * `null` for a miss. Method first because it is a string comparison and the
 * path walk is not, and because most misses in a real suite are the wrong
 * method on a path that exists.
 */
export function matchHandler(handler: MockHandler, method: string, url: URL): PathParams | null {
  if (handler.method !== "ALL" && handler.method !== method.toUpperCase()) {
    return null;
  }
  return matchPattern(handler.pattern, url);
}
