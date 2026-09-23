// @flow
//
// Middleware: what runs before a path answers, whatever answers it.
//
// `app/dashboard/$middleware.js` guards `/dashboard` and everything under
// it — the pages, the route handlers, and the paths under it that match
// nothing at all. There is no `matcher` to write because the directory the
// file sits in *is* the matcher, which is the same composition rule layouts
// already use and the reason uf does not inherit Next's regular expressions.
//
//   // app/dashboard/$middleware.js
//   // @flow
//   import { cookies } from "@uniflowed/server";
//
//   export default function middleware(request: Request): Response | void {
//     if (cookies().get("session") == null) {
//       return Response.redirect(new URL("/sign-in", request.url), 302);
//     }
//   }
//
// # The signature, and what it deliberately does not say
//
// `(request, context) => Response | void`. Returning a `Response` *is* the
// answer: nothing after it runs, no page is resolved and no handler is called.
// Returning nothing continues to the next middleware and then to whatever the
// path would otherwise have done. Both halves are load-bearing — a middleware
// that could only observe would not be able to reject, and one that had to
// answer could not be a logger.
//
// There is no `next()`. A third answer is `rewrite(destination)`: serve another
// route of this application at the address the visitor asked for.
//
//   // app/$middleware.js
//   import { rewrite } from "@uniflowed/router/middleware";
//
//   export default function middleware(request: Request) {
//     if (cookies().get("beta") != null) return rewrite("/beta" + new URL(request.url).pathname);
//   }
//
// A returned value rather than a returned `Request`, which was the other
// spelling on the table. A `Request` could change the method and the headers
// too, and `headers()` reads the request the host began — so a middleware that
// added a header would have handed the page one set of headers and `headers()`
// another. A rewrite changes the path, the query when it names one, and
// nothing else.
//
// # A rewrite runs the destination's middleware
//
// The chain starts again from the root over the middleware that has not run
// yet, against the new path. So a rewrite into `/admin` passes the guard on
// `/admin` exactly as a request for it would, and is never an unguarded way to
// a guarded page; a middleware that already ran for this request does not run
// a second time, which is also what makes the loop finite.
//
// # A payload request is its document
//
// A browser navigating a React Server Components application asks for
// `/pricing/__uf.flight` rather than `/pricing`. The chain is matched against,
// and every middleware is handed, the document's URL — so the check a
// middleware writes against `/pricing` holds for a client navigation too, and
// a rewrite of the document becomes a rewrite of its payload.
//
// # A payload that renders over another page runs that page's guards too
//
// An intercepted navigation — a modal slot opening over the page it was clicked
// from — asks for the payload of the URL it opens, and names the page it
// renders over in `uf-intercepted-from`. The answer is *both* pages: the one
// the URL names, in the slot, and the one the header names, underneath it,
// with that page's loader run and its data in the payload.
//
// So the chain matched against the URL alone would be the wrong chain. A guard
// on `/feed` holds for a request *for* `/feed`, and a payload for `/photo/1`
// rendered over `/feed` is `/feed` as much as it is `/photo/1`. After the
// ordinary chain has admitted the request, every middleware guarding the page
// underneath runs as well — against a `GET` for that page, with its params and
// query — including one that already ran for the URL, because a middleware
// that decides by reading the pathname (one root `$middleware.js` guarding
// several sections is a common shape) has only been asked about the other
// path. If every one of them declines, the interception stands. If any
// answers or rewrites, the page underneath is not this request's to render:
// the header is taken off, and what renders is the page the URL names on its
// own — exactly what a reader who could not open the page underneath would
// see after a reload. The guard's own answer is not sent: the request was for
// `/photo/1`, which that guard does not cover.
//
// The header is taken off whenever it is not a path on this origin, too, so
// the renderer is never handed a value this module has not judged.
//
// # The request it runs inside
//
// The runner does not establish one. The host does — `beginRequest` in
// `@uniflowed/server/host`, once per request, around everything that answers
// it — and the guard, the handler or page underneath it, and the render all
// see that one context. So `cookies()` in a guard and `cookies()` in the page
// it guards are the same cookies, `draftMode().isEnabled` gives a guard and
// the page under it the same answer, and every `after()` on the request is one
// ordered list the host drains after the response has gone.
//
// *Changing* draft mode is not a guard's to do, and that is a separate rule
// with a separate reason: `enable()` writes a cookie, a cookie is part of a
// response, and a guard may decline — so a guard that turned draft mode on and
// then let the request through would have made a decision with nowhere to be
// written. `asResponder` marks the two calls that do own a response, a route
// handler and a server action, and `draftMode().enable()` refuses anywhere
// else by name. A guard that wants draft mode on answers with a redirect to
// the handler that turns it on. See ubugeeei-prod/uf#282.
//
// It used to be the other way, and it is worth saying why that was wrong
// rather than merely different: this module built its own context and drained
// it before returning, which is early in both outcomes. When the chain
// answered, the caller was several lines from writing a byte; when it
// declined, there was no response at all yet and the dispatcher below was
// about to build a second context nothing here could see. `after()` says "once
// the response has been sent". See ubugeeei-prod/uf#389.
//
// # Server only
//
// This module is imported by `virtual:uf/server` and by nothing the browser
// loads. That is not decoration: a middleware is where an application puts the
// check it does not want a user to be able to read, and the client entry
// importing the table it lives in would ship every one of them to the page.
// `routesModuleSource` keeps the middleware table in an export the client
// never imports, for the same reason it does that with route handlers.

import { INTERCEPTED_FROM_HEADER, documentPathOf, flightUrl } from "./internal/flight.js";
import { requireRequest } from "./internal/request.js";
import type { RouteParams } from "./internal/runtime.js";

/** What a middleware is given besides the request. */
export type MiddlewareContext = {|
  /** The `[param]` segments of the *directory the middleware guards*. */
  readonly params: RouteParams,
  /** The parsed query string, for the common case of reading one value. */
  readonly searchParams: URLSearchParams,
|};

/**
 * What a middleware returns to serve another route at the requested address.
 *
 * Built by [`rewrite`] and read by the runner; a class so that the runner can
 * tell it from a `Response` without trusting the shape of an object.
 */
export class Rewrite {
  readonly destination: string;

  constructor(destination: string) {
    this.destination = destination;
  }
}

/**
 * Serve `destination` — a path of this application — in place of the path the
 * request named.
 *
 * Relative to the request, so `"/beta/pricing"` and `"../pricing"` both work.
 * A destination that names no query keeps the request's; one that names a
 * query replaces it. Another origin is refused when the middleware returns it:
 * sending a visitor elsewhere is `Response.redirect`, and proxying to another
 * server is a route handler that fetches.
 */
export function rewrite(destination: string | URL): Rewrite {
  return new Rewrite(typeof destination === "string" ? destination : destination.href);
}

/** One middleware function. */
export type Middleware = (
  request: Request,
  context: MiddlewareContext,
) => Response | Rewrite | void | Promise<Response | Rewrite | void>;

/** A middleware module, as the generated table loads it. */
export type MiddlewareModule = { readonly [name: string]: mixed };

/** One entry of the generated middleware table. */
export type MiddlewareRecord = {|
  /** The route path of the directory this middleware guards, `/` at the root. */
  readonly path: string,
  readonly file: string,
  readonly load: () => Promise<MiddlewareModule>,
|};

/**
 * Build the middleware runner for one application.
 *
 * Returns `null` when every middleware on the path declined, which is the
 * caller's signal to carry on to the handler or the page. Returns a `Request`
 * when one of them rewrote: the same request at the destination, which the
 * caller carries on with instead — and which has already been past the
 * destination's middleware. It also returns a `Request` when a payload request
 * names a page to render over and that page's guards did not all admit it: the
 * same request without `uf-intercepted-from`. The host must carry on with that
 * request and read the header off nothing else, or the decision is undone.
 *
 * The runner is called once per request, above both the dispatcher and the
 * renderer, rather than from inside each of them. Putting the call inside
 * `createDispatcher` and again inside `createRenderer` was the first shape and
 * it is wrong twice over: a request that matches neither — `/dashboard/typo`,
 * which is a 404 — would have run no middleware at all, and a path that is
 * both a page and a handler would have run it twice. Middleware is a property
 * of the request, so it belongs where the request arrives.
 */
export function createMiddlewareRunner(options: {|
  readonly middleware: $ReadOnlyArray<MiddlewareRecord>,
|}): (request: Request) => Promise<Response | Request | null> {
  // Root first, so an application-wide check runs before the one that guards a
  // section of it. A shorter path is always an ancestor of a longer one that
  // also matched, so segment count is the whole of the ordering.
  const table = [...options.middleware].sort(
    (a, b) => segmentsOf(a.path).length - segmentsOf(b.path).length,
  );

  return async function runMiddleware(request: Request): Promise<Response | Request | null> {
    // Checked rather than assumed, and checked before the table so that a host
    // is caught on its first request whether or not this project happens to
    // have a middleware. `createApplicationHandler` makes the same argument
    // about `entry.runMiddleware` itself — called rather than tested for, so a
    // server bundle without it is a `TypeError` on the first request instead of
    // an application whose auth check quietly stopped running. The same
    // argument applies to the request this runs inside: without one, the first
    // `cookies()` in somebody's guard would throw "called outside a request …
    // a static prerender, a module's top level, or a client component", which
    // is three wrong places to look, and an application with no `cookies()`
    // anywhere would reach its render with no context at all and lose every
    // `after()` to a different exception later.
    requireRequest("runMiddleware");
    if (table.length === 0) {
      return null;
    }

    const arrived = new URL(request.url);
    const document = documentPathOf(arrived.pathname);
    // The URL the chain is matched against and every middleware is handed: the
    // document's, for a payload request. See "A payload request is its
    // document" above.
    let url = document == null ? arrived : withPathname(arrived, document);
    let seen = document == null ? request : requestAt(request, url);
    let rewritten = false;
    const ran: Set<MiddlewareRecord> = new Set();

    for (let index = 0; index < table.length; index += 1) {
      const record = table[index];
      if (ran.has(record)) {
        continue;
      }
      const params = matchPrefix(record.path, url.pathname);
      if (params == null) {
        continue;
      }
      ran.add(record);

      const middleware = pick(await record.load(), record.file);
      // In the host's context, not one of this module's own. Two middleware on
      // the same path see the same cookies, and so does the handler or the page
      // underneath them: `draftMode().isEnabled` is one answer for the whole
      // request, and every `after()` on the request lands in one ordered list
      // that the host drains once, after the response has gone.
      const result = await middleware(seen, { params, searchParams: url.searchParams });
      if (result == null) {
        continue;
      }
      if (result instanceof Rewrite) {
        url = destinationOf(result.destination, url, record.file);
        seen = requestAt(seen, url);
        rewritten = true;
        // From the root again, over what has not run: the destination's guards
        // are owed their say, and the ones that already had it are not asked
        // twice.
        index = -1;
        continue;
      }
      return result;
    }

    const admitted: Request | null = !rewritten
      ? null
      : document == null
        ? seen
        : requestAt(seen, new URL(flightUrl(url.pathname + url.search), url));

    // A payload rendered over another page: that page's guards have their say
    // too. See "A payload that renders over another page" above.
    if (document != null && request.headers.has(INTERCEPTED_FROM_HEADER)) {
      const carried = admitted ?? request;
      const underneath = interceptionBase(request.headers.get(INTERCEPTED_FROM_HEADER), url);
      if (underneath == null || !(await guardsAdmit(table, carried, underneath))) {
        return withoutInterception(carried);
      }
    }
    return admitted;
  };
}

/**
 * The page an intercepted payload names in its header, as a URL on `base`'s
 * origin, or `null` when the value is not a path on this origin.
 *
 * The same shape the renderer accepts (`usableInterceptionBase` in `./rsc.js`,
 * `interceptedFrom` in `@uniflowed/server`): a path, not a network-path
 * reference, with any fragment dropped. Parsed rather than compared as text,
 * so the pathname the guards are matched against is the one the renderer's
 * route matching will read.
 */
function interceptionBase(header: string | null, base: URL): URL | null {
  if (header == null || !header.startsWith("/") || header.startsWith("//")) {
    return null;
  }
  let parsed: URL;
  try {
    parsed = new URL(header, base);
  } catch {
    return null;
  }
  if (parsed.origin !== base.origin) {
    return null;
  }
  parsed.hash = "";
  return parsed;
}

/**
 * Whether every middleware guarding `underneath` lets this request see it.
 *
 * Each one is asked exactly as it would be asked by a request for that page: a
 * `GET` at its URL carrying this request's headers, with the params of the
 * directory it guards and the page's query. All of them, root first, whether
 * or not they already ran for the URL the request names — a guard that reads
 * the pathname has so far only been asked about the other one.
 *
 * A `Response` is a refusal and so is a `rewrite()`: the page a guard would
 * serve instead is not the page the header names, and the renderer has no way
 * to render one under the other. Nothing either returns is sent; the caller
 * only learns that the page underneath is not this request's to render.
 */
async function guardsAdmit(
  table: $ReadOnlyArray<MiddlewareRecord>,
  request: Request,
  underneath: URL,
): Promise<boolean> {
  const asked = new Request(underneath.href, { method: "GET", headers: request.headers });
  for (const record of table) {
    const params = matchPrefix(record.path, underneath.pathname);
    if (params == null) {
      continue;
    }
    const middleware = pick(await record.load(), record.file);
    const result = await middleware(asked, { params, searchParams: underneath.searchParams });
    if (result != null) {
      return false;
    }
  }
  return true;
}

/**
 * `request` with its `uf-intercepted-from` header taken off, so the renderer
 * answers with the page its URL names and nothing underneath it.
 *
 * Only a payload request reaches here, and a payload is a `GET` or a `HEAD`,
 * so there is no body to hand on.
 */
function withoutInterception(request: Request): Request {
  const headers = new Headers(request.headers);
  headers.delete(INTERCEPTED_FROM_HEADER);
  return new Request(request.url, { method: request.method, headers, signal: request.signal });
}

/**
 * Where a rewrite goes, resolved against the URL the middleware was handed.
 *
 * Refused by name when it leaves the origin, because the one thing a rewrite
 * promises is that this application answers.
 */
function destinationOf(destination: string, base: URL, file: string): URL {
  const next = new URL(destination, base);
  if (next.origin !== base.origin) {
    throw new Error(
      `${file} rewrote ${base.pathname} to ${destination}, which is another origin. A rewrite ` +
        "serves another route of this application: answer with `Response.redirect` to send the " +
        "visitor elsewhere, or fetch the other server from a route handler.",
    );
  }
  if (!destination.includes("?")) {
    next.search = base.search;
  }
  next.hash = "";
  return next;
}

function withPathname(url: URL, pathname: string): URL {
  const next = new URL(url.href);
  next.pathname = pathname;
  return next;
}

/**
 * `request` at another URL: same method, headers, signal and body.
 *
 * A `Request` is a valid `RequestInit`, so a streamed body is handed on rather
 * than read.
 */
function requestAt(request: Request, url: URL): Request {
  // $FlowFixMe[incompatible-call] - a `Request` is read as the `RequestInit` it satisfies.
  return new Request(url.href, request);
}

/**
 * The function a middleware module exports.
 *
 * `default` or `middleware`, the same two spellings a page offers for its
 * component. Anything else is an authoring mistake and throws rather than
 * being skipped: a file named `$middleware.js` that the router quietly
 * ignored is the bug this whole module exists to stop happening.
 */
function pick(module: MiddlewareModule, file: string): Middleware {
  // `rewrite` is an export a middleware module may well import, and is never
  // the middleware itself.
  const exported = typeof module.default === "function" ? module.default : module.middleware;
  if (typeof exported !== "function") {
    throw new Error(
      `${file} is a middleware but exports no middleware function: export it as \`default\` or as \`middleware\`.`,
    );
  }
  return exported as $FlowFixMe;
}

/**
 * Match a middleware's directory path against a pathname, as a *prefix*.
 *
 * The difference from the dispatcher's `matchPath` is the whole point:
 * `/dashboard` matches `/dashboard`, `/dashboard/settings` and
 * `/dashboard/a/b`, because a middleware guards a subtree rather than a path.
 * `null` when it does not match, so a route with no parameters is still
 * distinguishable from a miss.
 */
function matchPrefix(routePath: string, pathname: string): RouteParams | null {
  const wanted = segmentsOf(routePath);
  const given = segmentsOf(pathname);
  const params: { [string]: string | Array<string> } = {};

  for (let index = 0; index < wanted.length; index += 1) {
    const segment = wanted[index];
    if (segment.startsWith(":") && segment.endsWith("*")) {
      params[segment.slice(1, -1)] = given.slice(index);
      return params as $FlowFixMe;
    }
    if (index >= given.length) {
      return null;
    }
    if (segment.startsWith(":")) {
      params[segment.slice(1)] = given[index];
      continue;
    }
    if (segment !== given[index]) {
      return null;
    }
  }

  return params as $FlowFixMe;
}

function segmentsOf(value: string): Array<string> {
  return value.split("/").filter((segment) => segment !== "");
}
