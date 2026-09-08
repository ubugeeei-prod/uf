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
// # And whether React DevTools can see the page at all
//
// The other question only this module is in a position to ask.
// `@uniflowed/vite` installs the hook DevTools attaches through, above every
// module in the document; whether that worked *on this page* is a fact about a
// running browser, and the line after hydration is where it can be read.
// `internal/devtools.js` has the two findings and sends them to the same
// terminal the hydration report goes to. See ubugeeei-prod/uf#503.
//
// # Strict Mode, in development, by default
//
// `uf dev` generates `strictMode: true` into `virtual:uf/client` and `uf build`
// does not, so a development render is doubled and a visitor's is not. That is
// React's own check for the thing it cannot check any other way: a component
// whose render is not pure, and an effect whose cleanup does not undo its
// setup, both behave correctly until the one production render that interleaves
// with something — and Strict Mode makes them behave incorrectly at once, on
// the machine of the person writing them.
//
// The wrapper is the argument to `hydrateRoot` rather than something inside
// `<App>`, and that is load-bearing rather than tidy. React decides whether to
// double-invoke a mount's effects at the *topmost fiber it is placing*: if that
// fiber is not itself in Strict Mode, React stops there and never looks inside
// it. A `<StrictMode>` further down still doubles the renders under it — that
// comes from the fiber's own mode — and doubles no effect at all, so it would
// have bought the half of the check that is easy to notice and silently lost
// the half that finds the bug. It renders no element, so the hydrated tree is
// unchanged and the markup comparison above is unaffected.
// `app.react.strictMode: false` in `uf.config.js` turns it off. See
// ubugeeei-prod/uf#516.
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
import { StrictMode, startTransition } from "react";
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
  readonly strictMode?: boolean,
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

  // `<StrictMode>` renders no element of its own, so the tree React hydrates
  // against the server's markup is the same tree either way and the flag can
  // be a development-only difference without being a hydration difference.
  const tree = <App url={url} initial={resolved} />;

  startTransition(() => {
    hydrateRoot(
      container,
      options.strictMode === true ? <StrictMode>{tree}</StrictMode> : tree,
      recovery == null ? undefined : { onRecoverableError: recovery },
    );
  });

  // And, in development only, whether the panel a developer is about to open
  // can see any of that. `react-dom` announced itself while it was being
  // imported — long before this line — so the answer is already settled and
  // this only reads it. Behind the same `import.meta.hot` gate as the
  // hydration reporter, dynamically imported for the same reason: a production
  // bundle has no path to the module rather than merely no reason to run it.
  // See `./internal/devtools.js` and ubugeeei-prod/uf#503.
  if (import.meta.hot != null) {
    const { reportDevtools } = await import("./internal/devtools.js");
    reportDevtools(window);
  }
}
