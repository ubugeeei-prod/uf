// @flow
//
// `@uniflowed/router/rsc/client`: starting, in the browser, an application that
// React Server Components rendered.
//
// `virtual:uf/client` imports this entry when routes render as Server
// Components, which is the default. It imports `@uniflowed/router/client` when
// they render from their modules (`app.rsc: false`) or into an empty shell
// (`app.rendering.modes: ["csr"]`). This is an entry of its own because it loads
// React's Flight client, `react-server-dom-parcel`. That package is an optional
// peer and needs React 19.3, while the rest of the router runs on the React
// 19.2.3 that Expo SDK 57 and React Native 0.87 ship (ubugeeei-prod/uf#992). A
// bundler resolves every import in the graph it is given, whether or not
// anything calls it, so a browser bundle that renders no Server Component
// leaves the package out only if nothing it imports names it.
// `crates/uf_lib/tests/package_surface.rs` holds the router to that.
//
// The same reason keeps the payload fetch out of `./internal/runtime.js`.
// Navigation there serves every application, so `hydrateFlight` hands it the
// fetch before the first render.

import * as React from "react";
import { StrictMode, startTransition } from "react";
import { hydrateRoot } from "react-dom/client";

import { type TrailingSlash, applicationPathOf } from "./internal/base-path.js";
import { ROOT_ID } from "./internal/document.js";
import {
  fetchFlight,
  installBrowserModules,
  readDocumentPayload,
} from "./internal/flight-browser.js";
import { domObserver } from "./internal/payload-rows.js";
import { prepareDocumentForHydration } from "./internal/prepare-document.js";
import { requireServerComponentsReact } from "./internal/react-version.js";
import {
  type AppProps,
  type Navigation,
  installFlightFetch,
  installNavigation,
  installRouting,
} from "./internal/runtime.js";

/**
 * Hydrate a document React Server Components rendered.
 *
 * `hydrate` in `./client.js` resolves the route from its modules and renders it
 * again over the server's markup. This one resolves nothing and imports no route
 * module: the document carries the Flight payload its tree was rendered from,
 * React's own client reads it, and the tree the browser hydrates is the tree the
 * server rendered — a Server Component is markup and a reference, and a client
 * component is the one kind of module this page loads. See
 * ubugeeei-prod/uf#519.
 *
 * The payload is read while the document is still arriving. Row 0 is in the
 * shell, so hydration starts as soon as the module script runs, and every row
 * after it lands in a later chunk that the reader picks up as it is parsed — so
 * a boundary the server completes after hydration began resolves then, with no
 * second request.
 *
 * Everything else is `hydrate`'s, for the reasons written there: the navigation
 * mode is installed before the first render, the development hydration report
 * captures the server's markup before React repairs it, and Strict Mode wraps
 * the root.
 *
 * On a React older than 19.3 it refuses before it touches the page, naming the
 * version it found; see `./internal/react-version.js`.
 */
export async function hydrateFlight(options: {|
  readonly App: React.ComponentType<AppProps>,
  readonly strictMode?: boolean,
  readonly navigation?: Navigation,
  readonly basePath?: string,
  readonly trailingSlash?: TrailingSlash,
|}): Promise<void> {
  requireServerComponentsReact("@uniflowed/router/rsc/client");
  installNavigation(options.navigation ?? "client");
  installRouting({ basePath: options.basePath, trailingSlash: options.trailingSlash });
  installFlightFetch(fetchFlight);
  installBrowserModules();
  const flight = readDocumentPayload(document, domObserver(document));

  // The route table has no base path in it, and the address bar does.
  const url =
    (applicationPathOf(window.location.pathname) ?? window.location.pathname) +
    window.location.search;
  const { App } = options;
  const container = document.getElementById(ROOT_ID) ?? document;
  prepareDocumentForHydration(document);

  let recovery = null;
  let restoreDevHead = null;
  if (import.meta.hot != null) {
    const { captureServerMarkup, hydrationErrorHandler, prepareDevHeadForHydration } =
      await import("./internal/hydration.js");
    restoreDevHead = prepareDevHeadForHydration(document);
    recovery = hydrationErrorHandler(container, captureServerMarkup(container), document);
  }

  const tree = <App url={url} flight={flight} />;

  startTransition(() => {
    hydrateRoot(
      container,
      options.strictMode === true ? <StrictMode>{tree}</StrictMode> : tree,
      recovery == null ? undefined : { onRecoverableError: recovery },
    );
    if (restoreDevHead != null) {
      setTimeout(restoreDevHead, 250);
    }
  });

  if (import.meta.hot != null) {
    const { reportDevtools } = await import("./internal/devtools.js");
    reportDevtools(window);
  }
}
