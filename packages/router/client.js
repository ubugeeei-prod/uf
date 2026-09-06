// @flow
//
// Hydrating the document in the browser.
//
// `virtual:uf/client` calls `hydrate` with the app root and the route table.
// The current route's chunks are loaded and its embedded loader data read
// *before* `hydrateRoot`, so the first client render is synchronous and
// matches the server's markup exactly.
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
  startTransition(() => {
    hydrateRoot(container, <App url={url} initial={resolved} />);
  });
}
