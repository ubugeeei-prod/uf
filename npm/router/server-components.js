// @flow
//
// `@uniflowed/router`, as a React Server Component imports it.
//
// The package root has two entries and an export condition picks between them:
// a module graph resolved under `react-server` — the one uf renders server
// components in (ubugeeei-prod/uf#519) — gets this file, and every other graph
// gets `./index.js`. Both export the same names, which is the point. A layout
// imports `Link` and `useRoute` from `@uniflowed/router` and means the same
// thing wherever it is rendered; what differs is what the names *are* here.
//
// * `Link`, `RouterProvider`, `RouteView` and `routerView` come from
//   `./internal/runtime.js`, a `"use client"` module, so in this graph each one
//   is a client reference: rendered as markup on the server and hydrated in
//   the browser, with its code shipped for the browser's half only.
// * `useRoute`, `useLoaderData` and `useSeo` are server implementations. A
//   server component has no context to read, so they read the route the
//   Flight renderer is rendering — `./internal/server-route.js` — which is the
//   same route the browser's hooks read back out of the payload. This
//   repository's documentation site highlights the section a reader is in
//   from a server layout, and would otherwise have needed a client component to
//   find out where it was.
// * `useRouter` refuses, by name. Navigation happens in the browser, and a
//   router a server component could hold would be methods that can never do
//   anything.
// * `useIsServer` answers `true`, which is what it has always answered while a
//   server renders.
//
// Everything else — matching, the router's control errors, resolution — is
// React-free and is the same code `./index.js` exports.

import * as React from "react";
import { use } from "react";

import { Head } from "./internal/head.js";
import type { Metadata } from "./internal/resolve.js";
import type { RouteInfo, Router } from "./internal/runtime.js";
import { serverRoute } from "./internal/server-route.js";

export type { ErrorProps, LayoutProps, PageProps, SearchParamsOf } from "./index.js";

export type {
  AppProps,
  ErrorBoundary,
  ErrorModule,
  Interception,
  JsonLd,
  LayoutModule,
  LinkPrefetch,
  LoaderArgs,
  Metadata,
  MetadataArgs,
  NavigateOptions,
  NotFoundBoundary,
  PageModule,
  ResolvedRoute,
  Robots,
  RouteError,
  RouteInfo,
  RouteMatch,
  RouteParamSpec,
  RouteParams,
  RouteRecord,
  RouteTable,
  Router,
  ResolvedSlot,
  SearchParams,
  SlotRecord,
  SlotRouteRecord,
  TemplateModule,
  TwitterCard,
} from "./internal/runtime.js";

export { Link, RouteView, RouterProvider, routerView, useLinkStatus } from "./internal/runtime.js";
// `app.router.basePath`, for a Server Component that builds an address itself.
// The rsc entry installs it; `./internal/base-path.js` has no directive, so this
// graph gets the function rather than a reference to it.
export { basePath } from "./internal/base-path.js";

export {
  ForbiddenError,
  NotFoundError,
  RedirectError,
  SearchParamsError,
  UnauthorizedError,
  buildRoute,
  forbidden,
  hasClientPage,
  matchRoute,
  notFound,
  parseSearch,
  parseSearchAll,
  permanentRedirect,
  redirect,
  routeErrorStatus,
  splitUrl,
  unauthorized,
} from "./internal/routing.js";

export { resolveFailure, resolveMatch } from "./internal/resolve.js";

/**
 * The route this server component is rendering in.
 *
 * `pending` is always `false`: a pending navigation is a fact about a browser
 * that has asked for the next route and not received it, and a server renders
 * the route it was asked for. `data` waits for a loader the router deferred,
 * which is what the browser's `useRoute` does too, so a component that reads
 * the data suspends at the same boundary on both sides.
 */
export hook useRoute(): RouteInfo {
  const route = serverRoute("useRoute");
  return {
    path: route.path,
    pathname: route.pathname,
    params: route.params,
    searchParams: route.searchParams,
    data: route.deferred == null ? route.data : use(route.deferred),
    pending: false,
  };
}

/** The current page's loader data; see `useRoute` for what a deferred loader does. */
export hook useLoaderData(): mixed {
  const route = serverRoute("useLoaderData");
  return route.deferred == null ? route.data : use(route.deferred);
}

/**
 * A refusal: navigation is the browser's.
 *
 * Thrown rather than handed back as methods that do nothing, because a server
 * component that called `router.push` in response to something would be
 * waiting for a navigation that cannot happen, and nothing would say so.
 */
export hook useRouter(): Router {
  throw new Error(
    "@uniflowed/router: useRouter() was called in a Server Component, and navigation happens " +
      "in the browser. Call it from a client component — a module that opens with the use " +
      "client directive — or render a <Link>, which is one, and which a Server Component can " +
      "render.",
  );
}

/**
 * Head elements a server component contributes, with the route's
 * `metadataBase` applied to the relative URLs it names.
 *
 * The same component the browser's `useSeo` renders, and the same base: the one
 * the root layout declared, read from the route rather than from context.
 */
export hook useSeo(seo: Metadata): React.Node {
  const route = serverRoute("useSeo");
  const base = seo.metadataBase ?? route.metadata.metadataBase;
  return <Head metadata={base == null ? seo : { ...seo, metadataBase: base }} />;
}

/** Whether the app is being rendered on the server: here, always. */
export hook useIsServer(): boolean {
  return true;
}
