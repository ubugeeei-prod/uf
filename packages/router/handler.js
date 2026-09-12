// @flow
//
// Route handlers: a path that answers a request instead of rendering a page.
//
// `app/api/users/$route.js` exporting `GET` and `POST` serves
// `/api/users`. A handler takes a `Request` and returns a `Response` — the
// platform's own types, not a framework's wrapper — because that is what runs
// unchanged on Node.js, Bun, Deno and a Cloudflare Worker, and uf's whole
// position is that the host is a capability rather than a target.
//
//   // app/api/users/[id]/$route.js
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
//
// # `QUERY`, and what refuses it
//
// `QUERY` is a `GET` with a body: safe, idempotent, cacheable, and the method
// that a search with more parameters than a URL can hold has been faking with a
// `POST` for twenty years. A handler exports it like any other verb, and this
// dispatcher matches it like any other verb, because there is nothing special
// about it *here*. What is special about it is the path between a client and
// this function, and that is the part worth writing down rather than leaving to
// be discovered in production.
//
// Three things refuse it, and they refuse it differently:
//
//   * **A client that cannot send it.** The Fetch standard forbids `CONNECT`,
//     `TRACE` and `TRACK` and allows any other token, so every browser and
//     every runtime uf targets can send a `QUERY` today. `XMLHttpRequest` and
//     `EventSource` cannot, and neither can a `<form>`.
//   * **An intermediary that will not forward it.** This is the real one. A
//     proxy, a CDN or a WAF that has a list of methods answers `405` or `501`
//     itself, and the request never arrives — so the failure looks exactly like
//     a route that does not exist, from a server that never saw it.
//     `@uniflowed/fetch` names that case in the error rather than passing the
//     status through, which is the whole of what "stated rather than
//     discovered" can mean from the other end of a wire.
//   * **A cache that does not know it is safe.** `QUERY` is cacheable in
//     principle and the key includes the body, which almost nothing implements.
//     uf's own route cache is `GET`-only and stays that way; anything in front
//     of the application should be told not to store a `QUERY` at all.
//
// What uf deliberately does not do about any of it is accept a method-override
// header. `X-HTTP-Method-Override: QUERY` on a `POST` is the usual workaround
// and it is the shape of CVE-2025-29927: an inbound header steering dispatch,
// which `docs/security.md` forbids in the row about that CVE and in rule 3. A
// route that must work through hostile infrastructure exports `POST` as well
// and says so in its own file, where a reader can see it.
//
// It also does not establish the request a handler is inside. The host does,
// around the whole of it, so a handler and the guard above it share one
// context; see the same section in `./middleware.js`. This module used to
// build its own and drain it the moment the handler returned, which the
// comment there called "the response is in hand" — true, and not what
// `after()` promises. A handler that streams its body has not sent a byte at
// that point. See ubugeeei-prod/uf#389.

import { asResponder, noteRoute } from "@uniflowed/server/host";

import { requireRequest } from "./internal/request.js";
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
 *
 * It was closed in name only until `QUERY` was added. `pick` looked the method
 * up on the module and this list decided nothing but the order of the `Allow`
 * header, so a module exporting `PURGE` answered `PURGE` — the exact behaviour
 * the paragraph above says is refused. Adding a verb was the moment to make the
 * sentence true, because the alternative was adding one to a list nothing read.
 *
 * `QUERY` is here and `CONNECT` and `TRACE` are not, and the difference is not
 * taste: the `fetch` specification forbids the last two outright, so a handler
 * exporting either could never be reached by a browser.
 */
export const HANDLER_METHODS: $ReadOnlyArray<string> = Object.freeze([
  "GET",
  "HEAD",
  "QUERY",
  "POST",
  "PUT",
  "PATCH",
  "DELETE",
  "OPTIONS",
]);

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
    // The host's half of the contract, checked rather than assumed; see
    // `./internal/request.js`.
    requireRequest("dispatch");
    const url = new URL(request.url);
    for (const record of table) {
      const params = matchPath(record.path, url.pathname);
      if (params == null) {
        continue;
      }

      // Before the module is loaded and before the method is checked, because
      // this is the answer to "what was this request" and a `405` is as much
      // this route's answer as a `200` is. A log of `/api/users/:id 405` is
      // actionable; the same line with the path in it is a million lines.
      noteRoute(record.path);

      const module = await record.load();
      const method = request.method.toUpperCase();
      const handler = pick(module, method);
      if (handler == null) {
        return methodNotAllowed(module);
      }

      // In the host's request, so a handler that calls `headers()`,
      // `cookies()` or `after()` answers about the same one its guard did, and
      // what it defers is drained once, by the host, after the bytes are out.
      //
      // And inside `asResponder`, which is the other half: a route handler is
      // one of the two things that owns a response, so it is one of the two
      // places `draftMode().enable()` is allowed — and the `Set-Cookie` it
      // decided on is written onto the response below rather than left on an
      // object the host is about to discard. See ubugeeei-prod/uf#282.
      const response = await asResponder("a route handler", async () =>
        handler(request, { params, searchParams: url.searchParams }),
      );

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

/**
 * The function for a method, falling back to `GET` for `HEAD`.
 *
 * The method is checked against `METHODS` first, which is what makes that list
 * closed rather than decorative: without it a request could name any export,
 * and a module's `PURGE` — or its `DEFAULT`, or a name a bundler added — would
 * answer one.
 */
function pick(module: HandlerModule, method: string): Handler | null {
  if (!HANDLER_METHODS.includes(method)) {
    return null;
  }
  const own = module[method];
  if (typeof own === "function") {
    return own as $FlowFixMe;
  }
  if (method === "HEAD" && typeof module.GET === "function") {
    return module.GET as $FlowFixMe;
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
  const own = new Set(HANDLER_METHODS.filter((method) => typeof module[method] === "function"));
  // A module exporting `GET` also answers `HEAD`, so `Allow` has to say so.
  if (own.has("GET")) {
    own.add("HEAD");
  }
  // Filtered through `METHODS` rather than listed in insertion order, so the
  // header reads in the conventional order however the module was written.
  return new Response(null, {
    status: 405,
    headers: { allow: HANDLER_METHODS.filter((method) => own.has(method)).join(", ") },
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

  return wanted.length === given.length ? (params as $FlowFixMe) : null;
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
