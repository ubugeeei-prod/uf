// @flow
//
// Hydrating the document in the browser.
//
// `virtual:uf/client` calls `hydrate` with the app root and the route table.
// The current route's chunks are loaded and its embedded loader data read
// *before* `hydrateRoot`, so the first client render is synchronous and
// matches the server's markup exactly.

import * as React from "react";
import { startTransition } from "react";
import { hydrateRoot } from "react-dom/client";

import { type AppProps, type RouteTable, installRoutes, resolveMatch } from "./internal/runtime.js";
import { DATA_ID, ROOT_ID } from "./internal/document.js";

/**
 * Hydrate the current document.
 */
export async function hydrate(options: {|
  readonly App: React.ComponentType<AppProps>,
  readonly routes: RouteTable["routes"],
  readonly notFound: RouteTable["notFound"],
|}): Promise<void> {
  const table: RouteTable = { routes: options.routes, notFound: options.notFound };
  installRoutes(table);

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
