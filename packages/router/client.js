// @flow
//
// Starting the application in the browser.
//
// Two entry points, and which one `virtual:uf/client` calls is decided by
// `app.rendering.modes`. `hydrate` is the one every uf build has used: a server
// or a prerender wrote the markup, and React attaches to it. `render` is for
// `["csr"]`, where the build wrote one shell with an empty root and nothing has
// been rendered anywhere yet; see its own comment for why that is not `hydrate`
// with a flag.
//
// `virtual:uf/client` calls `hydrate` with the app root and the route table.
// The current route's chunks are loaded and its embedded loader data read
// *before* `hydrateRoot`, so the first client render is synchronous and
// matches the server's markup exactly.
//
// # What "its loader data" means once the loader can defer
//
// It is a payload rather than a value: `internal/payload.js` writes the model
// into `<script id="__uf_data">` with a `"$P<n>"` reference wherever the loader
// left a promise, and each of those arrives later in a `<script data-uf-row>`
// of its own. So two things happen before `hydrateRoot` rather than one — the
// model is decoded, and `internal/payload-rows.js` starts watching for the
// rows it referred to. Both have to be first: the decoded model is what the
// first render is handed, and a row that landed while nothing was watching
// would be a boundary that never resolves. A document with nothing deferred
// has no references, so the reader is handed no ids and installs nothing.
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
// # And an application can decline to be navigated
//
// `app.rendering.navigation: "document"` is the whole application saying what
// the paragraph below says about one route: the document the server wrote is
// what a link produces, and the browser fetches the next one. It is *not* the
// same as declining to hydrate — the page still hydrates, so a `"use client"`
// component is still interactive — and what it removes is the takeover. The
// flag reaches the runtime through `installNavigation` before the first render;
// see `internal/runtime.js` for what `RouterProvider` and `Link` then do, and
// `docs/app/guide/rendering` for when a project wants it.
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
import { createRoot, hydrateRoot } from "react-dom/client";

import {
  type AppProps,
  type Navigation,
  type RouteTable,
  RedirectError,
  hasClientPage,
  installNavigation,
  installRoutes,
  matchRoute,
  resolveFailure,
  resolveMatch,
} from "./internal/runtime.js";
import { DATA_ID, ROOT_ID } from "./internal/document.js";
import { decodePayload } from "./internal/payload.js";
import { createPayloadReader, domObserver } from "./internal/payload-rows.js";

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
  readonly navigation?: Navigation,
|}): Promise<void> {
  const table: RouteTable = {
    routes: options.routes,
    notFound: options.notFound,
    errors: options.errors,
  };
  installRoutes(table);
  // Beside the table, and before anything renders. `"client"` when the entry
  // says nothing, which is what `virtual:uf/client` generated before
  // `app.rendering.navigation` existed and what a hand-written entry still
  // means: the default is the behaviour, not the absence of one.
  installNavigation(options.navigation ?? "client");

  // Before the loader data is read and before `resolveMatch` is called: both
  // would go looking for a page module that is not in this bundle.
  const matched = matchRoute(table.routes, window.location.pathname);
  if (matched != null && !hasClientPage(matched.route)) {
    return;
  }

  const url = window.location.pathname + window.location.search;
  // Row 0 of the payload, and the reader that will fill in the rows it refers
  // to. Both before `hydrateRoot`, and in this order: `decodePayload` is what
  // tells the reader which rows the page is waiting for, and `watch` is what
  // makes it notice the ones the server has not written yet. A document with
  // nothing deferred has no references, so the reader is handed no ids, and
  // `watch` returns without installing anything — see `internal/payload.js`.
  const embedded = document.getElementById(DATA_ID);
  const reader = createPayloadReader(document, domObserver(document));
  const data =
    embedded != null
      ? decodePayload(
          JSON.parse(embedded.textContent ?? "null"),
          reader.resolve,
          "the route's loader data",
        )
      : undefined;
  reader.watch();
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
  // dead there; Node leaves it undefined, so `packages/vite/rsc-split.test.js`
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

/**
 * Render the current route into an empty shell.
 *
 * The single-page entry point: `app.rendering.modes: ["csr"]` writes one
 * document with an empty root and no markup in it, and this is what fills it.
 *
 * # Why it is not `hydrate` with a flag
 *
 * Because hydration is React comparing what it renders against what a server
 * sent, and here no server sent anything. `hydrateRoot` against an empty
 * container is a mismatch on the first node of every page — React would report
 * it, throw the shell away and render from scratch, which is this function
 * with a warning in front of it. `createRoot` says what is actually happening:
 * the browser is the only renderer this application has.
 *
 * Three more things follow from there, and each of them is a line below rather
 * than an omission:
 *
 *   * **The loader runs here.** `resolveMatch` fetches the route's modules and
 *     runs its loader in the browser, because there was no server render to run
 *     it in and no `<script id="__uf_data">` for it to have left an answer in.
 *   * **A URL that matches nothing is the not-found boundary**, resolved the
 *     way a server resolves it. The host served this shell for a URL it had no
 *     file for, so "nothing matched" is a perfectly ordinary arrival here
 *     rather than the exception it is during hydration.
 *   * **A redirect is the browser's.** `redirect()` from a loader throws before
 *     anything is rendered; on a server that becomes a 307 and here it becomes
 *     `location.replace`, which is the same instruction to the same browser.
 */
export async function render(options: {|
  readonly App: React.ComponentType<AppProps>,
  readonly routes: RouteTable["routes"],
  readonly notFound: RouteTable["notFound"],
  readonly errors: RouteTable["errors"],
  readonly strictMode?: boolean,
  readonly navigation?: Navigation,
|}): Promise<void> {
  const table: RouteTable = {
    routes: options.routes,
    notFound: options.notFound,
    errors: options.errors,
  };
  installRoutes(table);
  installNavigation(options.navigation ?? "client");

  const url = window.location.pathname + window.location.search;
  let resolved;
  try {
    resolved = await resolveMatch(table, url);
  } catch (error) {
    if (error instanceof RedirectError) {
      window.location.replace(error.to);
      return;
    }
    // The error boundary, chosen the same way the server chooses it. A throw
    // from a loader is a page that cannot render, and rendering the boundary is
    // what this application has instead of a 500.
    resolved = await resolveFailure(table, url, error);
  }

  const { App } = options;
  // An element, and never `document` — which is the other difference from
  // `hydrate` above. `hydrateRoot` takes a document, because an app whose root
  // layout renders `<html>` owns the whole of one and the server wrote it;
  // `createRoot` does not, because creating a root *is* replacing the
  // container's children and the container here would be the document. The
  // shell always writes this element, so its absence means the document being
  // rendered into is not one this build produced.
  const container = document.getElementById(ROOT_ID);
  if (container == null) {
    throw new Error(
      `@uniflowed/router: no #${ROOT_ID} in this document, so there is nothing to render into. ` +
        "A single-page build writes the shell that carries it; this document came from " +
        "somewhere else.",
    );
  }
  const tree = <App url={url} initial={resolved} />;
  createRoot(container).render(
    options.strictMode === true ? <StrictMode>{tree}</StrictMode> : tree,
  );
}
