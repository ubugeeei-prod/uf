// @flow
//
// Internal to `@uniflowed/router`: the routes a navigation already has.
//
// A navigation used to ask the server every time. A `Link` prefetched a route's
// payload on hover and handed it to exactly one click, and every other way back
// to a page the reader had just seen, a second visit or the back button, waited
// on the network again (ubugeeei-prod/uf#960). This module is what a navigation
// reads first instead: the route it fetched or prefetched, kept for
// `app.rendering.staleTime` seconds and asked for again after that.
//
// # Off until a project says a number
//
// uf's caches are opt-in, and this one is too. `staleTime` is `0` until a
// project writes one, and at `0` nothing is kept, so a navigation shows what the
// server answered for it just now, which is the guarantee every project had
// before the setting existed.
//
// # What is kept
//
// One promise per route, never a copy of what it resolved to:
//
// - an application React Server Components render keeps the payload fetch: the
//   route's state and its tree, with the `$loading.js` fallbacks the tree
//   carries, so going back to a page that was still streaming shows the loading
//   shell the first visit did;
// - an application rendered from its modules keeps the resolved route: the
//   loader's data, and the page, layouts and loading boundaries it loaded.
//
// A promise rather than its value, so a click on a link whose prefetch is still
// in flight waits on that request instead of making a second one. The router
// forgets an entry whose fetch failed or turned out not to be a route.
//
// # Keyed by the application path
//
// The path and query the route table is asked about, without
// `app.router.basePath`: `/docs/guide?tab=api` under `/docs` is kept as
// `/guide?tab=api`. A fragment is never part of a key, because a server never
// sees one.
//
// # Cleared, not revalidated in place
//
// `router.refresh()` and every server action clear the whole cache. An action
// is a write, and which pages it changed is the server's to know, so the next
// navigation to any of them asks again rather than showing what the write made
// untrue.

import { applicationPathOf } from "./base-path.js";
import type { FetchedFlight } from "./flight.js";
import type { ResolvedRoute } from "./resolve.js";

/** How many routes each cache keeps at once. Past it, the oldest is dropped. */
export const NAVIGATION_CACHE_LIMIT = 32;

let staleTimeMs: number = 0;

/**
 * Say how long, in seconds, a route a navigation fetched is shown again without
 * asking. `0`, the default, keeps nothing. Called once, by the entry that
 * started the application.
 */
export function installStaleTime(seconds: number): void {
  staleTimeMs = Number.isFinite(seconds) && seconds > 0 ? seconds * 1000 : 0;
  if (staleTimeMs === 0) {
    clearNavigationCache();
  }
}

/** Whether navigations keep what they fetch. */
export function keepsNavigations(): boolean {
  return staleTimeMs > 0;
}

/** The key a URL's route is kept under: its application path, then its query. */
export function navigationKey(pathname: string, search: string): string {
  return `${applicationPathOf(pathname) ?? pathname}${search}`;
}

/** One kind of kept route. */
export type NavigationCache<T> = {|
  /** The route kept under `key`, while it is fresh. A stale one is dropped. */
  readonly read: (key: string) => T | null,
  /** Keep `value` under `key` from now. Nothing is kept while the stale time is `0`. */
  readonly store: (key: string, value: T) => void,
  /** Drop what `key` keeps, if it is still `value`. */
  readonly forget: (key: string, value: T) => void,
  readonly clear: () => void,
  /** How many routes are kept, fresh or not. */
  readonly size: () => number,
|};

function createNavigationCache<T>(): NavigationCache<T> {
  const entries: Map<string, {| readonly value: T, readonly at: number |}> = new Map();
  return {
    read(key) {
      const entry = entries.get(key);
      if (entry == null) {
        return null;
      }
      if (Date.now() - entry.at >= staleTimeMs) {
        entries.delete(key);
        return null;
      }
      return entry.value;
    },
    store(key, value) {
      if (staleTimeMs === 0) {
        return;
      }
      // Deleted first, so a route kept again moves to the young end.
      entries.delete(key);
      if (entries.size >= NAVIGATION_CACHE_LIMIT) {
        const oldest = entries.keys().next();
        if (oldest.done !== true) {
          entries.delete(oldest.value);
        }
      }
      entries.set(key, { value, at: Date.now() });
    },
    forget(key, value) {
      if (entries.get(key)?.value === value) {
        entries.delete(key);
      }
    },
    clear() {
      entries.clear();
    },
    size() {
      return entries.size;
    },
  };
}

/** Payload fetches, for an application React Server Components render. */
export const flightNavigations: NavigationCache<Promise<FetchedFlight>> = createNavigationCache();

/** Resolved routes, for an application rendered from its modules. */
export const routeNavigations: NavigationCache<Promise<ResolvedRoute>> = createNavigationCache();

/** Forget every kept route. `router.refresh()` and each server action call this. */
export function clearNavigationCache(): void {
  flightNavigations.clear();
  routeNavigations.clear();
}
