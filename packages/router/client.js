// @flow
//
// Hydrating the document in the browser.
//
// `virtual:uf/client` calls `hydrate` with the app root and the route table.
// The current route's chunks are loaded and its embedded loader data read
// *before* `hydrateRoot`, so the first client render is synchronous and
// matches the server's markup exactly.
//
// # A hydration that fails says what differed
//
// React reports a mismatch with one sentence and a list of the six things that
// usually cause it, and leaves the reader to find which node of the two
// thousand on the page was the one. This module is the only place that can do
// better, because it is the only place that runs between the parser finishing
// and React starting: `internal/hydration.js` takes a copy of the server's
// markup here, and compares it against the repaired tree when React reports.
// Development only, and dynamically imported so a production bundle has no path
// to it. See ubugeeei-prod/uf#508.
//
// # A route can decline to be hydrated
//
// uf's server-component analysis decides which routes have a `"use client"`
// boundary anywhere in them, and `@uniflowed/vite` leaves the page out of the
// client route table for the ones that have none. Such a route has nothing in
// the browser to attach: the document the server wrote is the whole of it. So
// this returns without calling `hydrateRoot`, and the `<a>` elements a `Link`
// rendered stay what the server made them — real links the browser follows.
// See ubugeeei-prod/uf#350.

import * as React from "react";
import { startTransition } from "react";
import { hydrateRoot } from "react-dom/client";

import {
  type AppProps,
  type RouteTable,
  hasClientPage,
  installRoutes,
  matchRoute,
  resolveMatch,
} from "./internal/runtime.js";
import { DATA_ID, ROOT_ID } from "./internal/document.js";

/**
 * Hydrate the current document.
 *
 * Resolves without mounting anything when the current route ships no client
 * page — see the header. The promise settling is not a claim that React is on
 * the document.
 */
export async function hydrate(options: {|
  readonly App: React.ComponentType<AppProps>,
  readonly routes: RouteTable["routes"],
  readonly notFound: RouteTable["notFound"],
  readonly errors: RouteTable["errors"],
|}): Promise<void> {
  const table: RouteTable = {
    routes: options.routes,
    notFound: options.notFound,
    errors: options.errors,
  };
  installRoutes(table);

  // Before the loader data is read and before `resolveMatch` is called: both
  // would go looking for a page module that is not in this bundle.
  const matched = matchRoute(table.routes, window.location.pathname);
  if (matched != null && !hasClientPage(matched.route)) {
    return;
  }

  const url = window.location.pathname + window.location.search;
  const embedded = document.getElementById(DATA_ID);
  const data = embedded != null ? JSON.parse(embedded.textContent ?? "null") : undefined;
  const resolved = await resolveMatch(table, url, { data, skipLoader: embedded != null });

  const { App } = options;
  const container = document.getElementById(ROOT_ID) ?? document;

  // The server's markup, and the reporter that will read it, in development
  // only. Both have to be in place *before* `hydrateRoot`: React repairs a
  // mismatched subtree by rendering over it, so the bytes the server sent exist
  // for exactly the moment between the parser finishing and this line.
  //
  // `import.meta.hot` is the gate because it is the one signal that is right in
  // all three places this module is evaluated. Vite defines it while serving
  // and replaces it with `undefined` in a build, so the branch is statically
  // dead there; Node leaves it undefined, so `tests/library/rsc-split.test.js`
  // imports this file without a bundler and gets the production path. The
  // import is dynamic so that the overlay is not merely shaken out of a
  // production bundle but never reachable from one.
  let recovery = null;
  if (import.meta.hot != null) {
    const { captureServerMarkup, hydrationErrorHandler } = await import("./internal/hydration.js");
    recovery = hydrationErrorHandler(container, captureServerMarkup(container), document);
  }

  startTransition(() => {
    hydrateRoot(
      container,
      <App url={url} initial={resolved} />,
      recovery == null ? undefined : { onRecoverableError: recovery },
    );
  });
}
