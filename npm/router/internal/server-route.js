// @flow
//
// Internal to `@uniflowed/router`: the route a server component is rendering in.
//
// `useRoute()` in the browser reads the router's context, and a server
// component has no context to read. The graph it renders in resolves `react`
// under the `react-server` condition, which is the build with no `useContext`
// in it (ubugeeei-prod/uf#519), and a layout that highlights the section it is
// in — this repository's own documentation site has two — would otherwise have
// to become a client component to find out where it is.
//
// So the Flight renderer runs each render inside a store holding the route it
// resolved, and the server half of the hooks reads it back. `AsyncLocalStorage`
// and not React's `cache`: `cache` finds its render through React's own request
// storage, which the build React ships for Deno does not have, so after the
// first `await` in an async server component it would stop finding the render
// at all. A store of this module's own follows every continuation of the
// render on every host `@uniflowed/server` already runs on, and it is one value
// per render, so two requests in flight cannot read each other's route.
//
// Server-only by construction: nothing in the browser's graph imports this.

import { AsyncLocalStorage } from "node:async_hooks";

import type { RouteState } from "./flight.js";
import { requireServerComponentsReact } from "./react-version.js";

const storage: AsyncLocalStorage<RouteState> = new AsyncLocalStorage();

/** Run `body` as a render of `route`. Everything it starts sees the route. */
export function withServerRoute<T>(route: RouteState, body: () => T): T {
  return storage.run(route, body);
}

/**
 * The route being rendered, or a refusal naming the caller.
 *
 * A refusal rather than `null`, because the only way to arrive here without a
 * route is to call a router hook from a server module that is not being
 * rendered by the router — a script, a route handler, a module evaluated at
 * import time — and each of those is a mistake worth a sentence.
 *
 * The React version is checked first. On a React older than 19.3 the Flight
 * renderer that would have put the hook inside a route refuses to start, so the
 * sentence worth reading is that one, not "outside a route".
 */
export function serverRoute(caller: string): RouteState {
  requireServerComponentsReact(`${caller}()`);
  const route = storage.getStore();
  if (route == null) {
    throw new Error(
      `@uniflowed/router: ${caller}() was called in a server module outside a route the router ` +
        "is rendering. Server Components read the route they render in; a route handler reads " +
        "the request it was given, and a module evaluated at import time has no route at all.",
    );
  }
  return route;
}
