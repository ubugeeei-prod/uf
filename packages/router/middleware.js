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
// There is no `next()` and no way to rewrite the request. Rewriting needs a
// spelling — a returned `Request`, or a `next(request)` argument — and picking
// one badly is harder to undo than not having it, so it is not spelled here
// yet. What exists answers or continues, and says so.
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

import { requireRequest } from "./internal/request.js";
import type { RouteParams } from "./internal/runtime.js";

/** What a middleware is given besides the request. */
export type MiddlewareContext = {|
  /** The `[param]` segments of the *directory the middleware guards*. */
  readonly params: RouteParams,
  /** The parsed query string, for the common case of reading one value. */
  readonly searchParams: URLSearchParams,
|};

/** One middleware function. */
export type Middleware = (
  request: Request,
  context: MiddlewareContext,
) => Response | void | Promise<Response | void>;

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
 * caller's signal to carry on to the handler or the page.
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
|}): (request: Request) => Promise<Response | null> {
  // Root first, so an application-wide check runs before the one that guards a
  // section of it. A shorter path is always an ancestor of a longer one that
  // also matched, so segment count is the whole of the ordering.
  const table = [...options.middleware].sort(
    (a, b) => segmentsOf(a.path).length - segmentsOf(b.path).length,
  );

  return async function runMiddleware(request: Request): Promise<Response | null> {
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

    const url = new URL(request.url);

    for (const record of table) {
      const params = matchPrefix(record.path, url.pathname);
      if (params == null) {
        continue;
      }

      const middleware = pick(await record.load(), record.file);
      // In the host's context, not one of this module's own. Two middleware on
      // the same path see the same cookies, and so does the handler or the page
      // underneath them: `draftMode().isEnabled` is one answer for the whole
      // request, and every `after()` on the request lands in one ordered list
      // that the host drains once, after the response has gone.
      const result = await middleware(request, { params, searchParams: url.searchParams });
      if (result != null) {
        return result;
      }
    }

    return null;
  };
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
