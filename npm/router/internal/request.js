// @flow
//
// Internal to `@uniflowed/router`: the request the server half runs inside.
//
// One function, and it exists because two modules need the same refusal.
// `createMiddlewareRunner` and `createDispatcher` both used to build a request
// context of their own — `contextFor`, then `runWithContext`, then
// `drainDeferred` — which gave one request up to two contexts and ran what
// `after()` deferred before the response was written, and in the middleware's
// case before there was a response at all. See ubugeeei-prod/uf#389.
//
// Both now run inside whatever request the host established, which means both
// depend on the host having established one. That dependency is checked rather
// than assumed, for the reason `createApplicationHandler` gives about
// `entry.runMiddleware` itself: a missing half of the contract should be a
// failure on the first request, not an application that answers strangely.
//
// Server-only, like the two modules that import it. Nothing the browser loads
// reaches this.

import { insideRequest } from "@uniflowed/server/host";

/**
 * Refuse to run outside a request, naming what has to establish one.
 *
 * The message is addressed to whoever wired the host, because that is who can
 * fix it. Left to fail on its own, a mis-wired host would surface as the first
 * `cookies()` in somebody's guard throwing "called outside a request … a
 * static prerender, a module's top level, or a client component" — three
 * places to look, none of them this one — and an application that calls no
 * server function at all would sail past that and lose every `after()` instead.
 */
export function requireRequest(entry: string): void {
  if (insideRequest()) {
    return;
  }
  throw new Error(
    `@uniflowed/router: ${entry}() was called outside a request. A host owns the request: ` +
      "begin it with `beginRequest` from `@uniflowed/server/host` (a bundled application " +
      "re-exports it from `virtual:uf/server`), run the whole request inside `run`, and call " +
      "`settle` once the response has been sent. See ubugeeei-prod/uf#389.",
  );
}
