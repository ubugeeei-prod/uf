// @flow
//
// Route handlers: a path that answers a request instead of rendering a page.
//
// `app/api/users/_uf.route.js` exporting `GET` and `POST` serves
// `/api/users`. A handler takes a `Request` and returns a `Response` — the
// platform's own types, not a framework's wrapper — because that is what runs
// unchanged on Node.js, Bun, Deno and a Cloudflare Worker, and uf's whole
// position is that the host is a capability rather than a target.
//
//   // app/api/users/[id]/_uf.route.js
//   // @flow
//   export async function GET(request: Request, context: HandlerContext) {
//     const user = await find(context.params.id);
//     return user == null
//       ? new Response("not found", { status: 404 })
//       : Response.json(user);
//   }
//
// # What the dispatcher decides, and what it does not
//
// It matches a path and a method and calls a function. It does not catch the
// handler's errors, because a handler that throws is a bug the host's own
// error reporting should see, and swallowing it into a 500 here would hide it.
// It does answer `405` itself when the path matches and the method does not,
// with the `Allow` header the specification requires — that is not the
// handler's business, and every handler would otherwise write it.

import type { RouteParams } from "./internal/runtime.js";

/** What a handler is given besides the request. */
export type HandlerContext = {|
  /** The `[param]` and `[...rest]` segments of the matched path. */
  readonly params: RouteParams,
  /** The parsed query string, for the common case of reading one value. */
  readonly searchParams: URLSearchParams,
|};

/** One exported method of a handler module. */
export type Handler = (request: Request, context: HandlerContext) => Response | Promise<Response>;

/** A handler module, as the generated table loads it. */
export type HandlerModule = { readonly [method: string]: mixed };

/** One entry of the generated handler table. */
export type HandlerRecord = {|
  readonly path: string,
  readonly params: $ReadOnlyArray<{| readonly name: string, readonly catchAll: boolean |}>,
  readonly file: string,
  readonly load: () => Promise<HandlerModule>,
|};

/**
 * The methods a handler may export.
 *
 * A closed list, because the alternative is treating every export as a method
 * — and a module that exports a helper would then answer requests with it.
 * `HEAD` falls back to `GET` with the body dropped, which is what a client
 * asking for headers expects and what nobody remembers to write.
 */
const METHODS = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

/**
 * Match a request against the handler table and run it.
 *
 * Returns `null` when no path matches, which is the caller's signal to carry
 * on — a request for `/about` is a page, and the dispatcher declining is how
 * it says so.
 */
export function createDispatcher(options: {|
  readonly handlers: $ReadOnlyArray<HandlerRecord>,
|}): (request: Request) => Promise<Response | null> {
  // Longest path first, so `/api/users/new` wins over `/api/users/[id]` and a
  // catch-all is the last thing tried.
  const table = [...options.handlers].sort((a, b) => specificity(b.path) - specificity(a.path));

  return async function dispatch(request: Request): Promise<Response | null> {
    const url = new URL(request.url);
    for (const record of table) {
      const params = matchPath(record.path, url.pathname);
      if (params == null) {
        continue;
      }

      const module = await record.load();
      const method = request.method.toUpperCase();
      const handler = pick(module, method);
      if (handler == null) {
        return methodNotAllowed(module);
      }

      const response = await handler(request, {
        params,
        searchParams: url.searchParams,
      });

      // A `HEAD` answered by `GET` must not carry the body. The test is
      // against the module's own `HEAD`, not `pick`'s — `pick` falls back to
      // `GET`, so asking it whether a `HEAD` exists always said yes and the
      // body went out anyway.
      if (method === "HEAD" && typeof module.HEAD !== "function") {
        return new Response(null, {
          status: response.status,
          statusText: response.statusText,
          headers: response.headers,
        });
      }
      return response;
    }
    return null;
  };
}

/** The function for a method, falling back to `GET` for `HEAD`. */
function pick(module: HandlerModule, method: string): Handler | null {
  const own = module[method];
  if (typeof own === "function") {
    return (own: $FlowFixMe);
  }
  if (method === "HEAD" && typeof module.GET === "function") {
    return (module.GET: $FlowFixMe);
  }
  return null;
}

/**
 * `405`, with the `Allow` header naming what the path does accept.
 *
 * Required by the specification, and the reason a client can tell "you may not
 * do that here" from "there is nothing here".
 */
function methodNotAllowed(module: HandlerModule): Response {
  const own = new Set(METHODS.filter((method) => typeof module[method] === "function"));
  // A module exporting `GET` also answers `HEAD`, so `Allow` has to say so.
  if (own.has("GET")) {
    own.add("HEAD");
  }
  // Filtered through `METHODS` rather than listed in insertion order, so the
  // header reads in the conventional order however the module was written.
  return new Response(null, {
    status: 405,
    headers: { allow: METHODS.filter((method) => own.has(method)).join(", ") },
  });
}

/**
 * Match one route path against a pathname, returning its parameters.
 *
 * `null` rather than an empty object when it does not match, so a route with
 * no parameters is still distinguishable from a miss.
 */
function matchPath(routePath: string, pathname: string): RouteParams | null {
  const wanted = segmentsOf(routePath);
  const given = segmentsOf(pathname);
  const params: { [string]: string | Array<string> } = {};

  for (let index = 0; index < wanted.length; index += 1) {
    const segment = wanted[index];
    if (segment.startsWith(":") && segment.endsWith("*")) {
      // A catch-all takes the rest, and matches zero segments as well as many.
      params[segment.slice(1, -1)] = given.slice(index);
      return (params: $FlowFixMe);
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

  return wanted.length === given.length ? (params: $FlowFixMe) : null;
}

function segmentsOf(value: string): Array<string> {
  return value.split("/").filter((segment) => segment !== "");
}

/**
 * How specific a path is, so the table can be tried in the right order.
 *
 * A literal segment is worth more than a parameter and a parameter more than a
 * catch-all, and a longer path outranks a shorter one — which is what makes
 * `/api/users/new` win over `/api/users/[id]`.
 */
function specificity(routePath: string): number {
  let score = 0;
  for (const segment of segmentsOf(routePath)) {
    if (segment.startsWith(":") && segment.endsWith("*")) {
      score += 1;
    } else if (segment.startsWith(":")) {
      score += 10;
    } else {
      score += 100;
    }
  }
  return score;
}
