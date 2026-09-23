// @flow
//
// `@uniflowed/router/rsc`: a route, rendered as React Server Components.
//
// This module runs in the module graph uf resolves under the `react-server`
// export condition, which is the graph where `react` is the build with no
// `useState` in it and where React's Flight renderer is allowed to load
// (ubugeeei-prod/uf#519). It does three things, and each is a piece that
// already existed somewhere else:
//
//   1. **Resolve the URL** — `resolveMatch`, the same resolution a single-page
//      application runs in the browser: the match, the modules, the loader, the
//      metadata, the boundaries.
//   2. **Compose the tree** — `composeRoute`, the same walk `RouteView` makes,
//      so a layout, a fallback, a template and a slot land where they would in
//      any other render.
//   3. **Render it as a Flight payload** — `renderToReadableStream` from
//      `react-server-dom-parcel`, which is React's own renderer and React's own
//      wire format. uf writes no serialiser here and no decoder anywhere.
//
// The payload's root value is a [`FlightRoot`]: the route, as the part of it a
// hook reads, and the tree. The HTML renderer (`./server.js`) reads it to write
// the document, and the browser (`./client.js`) reads the same bytes to hydrate
// and, after that, to navigate — so what the browser renders is exactly what
// the server rendered, and no page, layout or loader module is ever the
// browser's to evaluate. A `"use client"` module is the one thing that crosses,
// as a reference to a chunk the browser loads.
//
// # What leaves before the payload does
//
// A redirect, and the status. Both are decided while the route resolves, which
// is before anything renders, so `renderFlight` answers a redirect instead of a
// stream and a route with the status it resolved to. That is the head-before-
// the-first-byte property `./internal/stream.js` depends on, kept from the
// other side: nothing the renderer learns while streaming can change the
// response line.
//
// # Error boundaries have to be client modules
//
// An `$error.js` catches a throw while the *browser* renders, so its component
// is the browser's, and a server component cannot be passed to a client one —
// Flight refuses a function where a reference was expected. So each boundary's
// component crosses as the client reference its `"use client"` directive made it
// in this graph, and a boundary module without the directive is refused here,
// with its file named, rather than by React with a sentence about functions.

import { traceLoader } from "./internal/server-instrumentation.js";

import * as React from "react";
import { use } from "react";
// React's Flight server. Only this graph can load it: it refuses to evaluate
// unless `react` resolved under the `react-server` condition.
import { renderToReadableStream } from "react-server-dom-parcel/server";

import { noteRoute } from "@uniflowed/server/host";
import { reportRequestError } from "@uniflowed/server/instrumentation";

import { BoundaryReporter } from "./internal/boundaries.js";
import { routeBoundaries } from "./internal/boundary-data.js";
import { composeRoute, pageComponent } from "./internal/compose.js";
import { ErrorRoutePage } from "./internal/error-view.js";
import { type FlightRoot, routeState } from "./internal/flight.js";
import { requireServerComponentsReact } from "./internal/react-version.js";
import type {
  ErrorModule,
  PageModule,
  ResolvedRoute,
  ResolvedSlot,
  RouteTable,
} from "./internal/resolve.js";
import { resolveFailure, resolveInterception, resolveMatch } from "./internal/resolve.js";
import type { RouteParams, SearchParams } from "./internal/routing.js";
import { RedirectError, nearestBoundary } from "./internal/routing.js";
import { withServerRoute } from "./internal/server-route.js";

export type { FlightRoot, RouteState } from "./internal/flight.js";
// For `virtual:uf/rsc`, so a Server Component's `basePath()` is the project's.
export { installRouting } from "./internal/base-path.js";

/** What a host may tell the renderer about one render. */
export type FlightOptions = {|
  /**
   * Whether a loader may be left running into a `$loading.js` boundary.
   *
   * On by default: a payload streams, so a slow loader is a fallback now and
   * its answer later. A prerender turns it off, because a file has no "later".
   */
  readonly defer?: boolean,
  /**
   * Every exception the render recovered from, including the late ones.
   *
   * What it returns becomes the exception's digest. React writes the digest
   * into the row that stands in for the part of the tree that failed, and sets
   * it on the error its Flight client rebuilds from that row — which is how
   * `./server.js` recognises, in the HTML renderer, the copy of an exception
   * this renderer has already reported.
   */
  readonly onError?: (error: mixed) => ?string,
  /**
   * Render the error boundary for this exception instead of resolving the URL.
   *
   * What the HTML renderer asks for when the shell it was streaming from the
   * payload threw before its first byte: the route resolved, and rendering it
   * did not.
   */
  readonly failure?: {| readonly error: mixed |},
  /**
   * The page a browser was showing when it asked for this payload.
   *
   * A document request never sets it. A client navigation may, because only the
   * browser knows the page it is navigating from, while only this renderer can
   * render the intercepted tree.
   */
  readonly interceptedFrom?: string,
  /** Stops the render, for a reader that went away. */
  readonly signal?: AbortSignal,
|};

/** A render: a redirect to answer with, or a route and its payload. */
export type FlightRender =
  | {| readonly kind: "redirect", readonly status: 307 | 308, readonly location: string |}
  | {|
      readonly kind: "route",
      readonly status: 200 | 401 | 403 | 404 | 500,
      /** The payload, as React writes it. Read exactly once. */
      readonly stream: ReadableStream<Uint8Array>,
      /**
       * The exception this route resolved to its error boundary for, when it did.
       *
       * The same field `./server.js` has always carried as `error`: `uf build`
       * fails a route that set it and `uf dev` reports it. `forbidden()` and
       * `unauthorized()` do not set it.
       */
      readonly failure: mixed,
    |};

/** Render one URL. */
export type FlightRenderer = (url: string, options?: FlightOptions) => Promise<FlightRender>;

/**
 * Whether this bundle marks the boundaries it renders.
 *
 * `BOUNDARY_MARKS` in `./internal/runtime.js` has the argument; a build replaces
 * `import.meta.hot` with `undefined` and every use below folds away.
 */
const BOUNDARY_MARKS: boolean = import.meta.hot != null;

/** How a client reference says what it is. React's symbol, not uf's. */
const CLIENT_REFERENCE: symbol = Symbol.for("react.client.reference");

/**
 * The renderer for one route table.
 *
 * The table is the server's whole one — every page, layout and boundary module
 * — and it is `virtual:uf/routes` as this graph generates it.
 */
export function createFlightRenderer(options: {|
  readonly routes: RouteTable["routes"],
  readonly notFound: RouteTable["notFound"],
  readonly errors: RouteTable["errors"],
  /**
   * The build's deployment id, written into every payload's root so that a
   * page on another build can tell — including from a payload that was
   * prerendered into a file. `null` under `uf dev`. See
   * `./internal/deployment.js`.
   */
  readonly deployment?: string | null,
|}): FlightRenderer {
  // Before anything else: on a React older than 19.3 nothing below can render,
  // and React's Flight renderer would only say so from inside a render.
  requireServerComponentsReact("@uniflowed/router/rsc");
  const table: RouteTable = {
    routes: options.routes,
    notFound: options.notFound,
    errors: options.errors,
  };

  return async function renderFlight(url: string, settings?: FlightOptions): Promise<FlightRender> {
    let resolved: ResolvedRoute;
    try {
      const failure = settings?.failure;
      if (failure == null) {
        const defer = settings?.defer !== false;
        const intercepted = await resolveFlightInterception(
          table,
          url,
          settings?.interceptedFrom,
          defer,
        );
        resolved =
          intercepted ??
          // `onMatch` records the route pattern on the request before the
          // loader runs, so every line the loader logs names its route.
          (await resolveMatch(table, url, { defer, onMatch: noteRoute, runLoader: traceLoader }));
      } else {
        resolved = await resolveFailure(table, url, failure.error);
      }
    } catch (error) {
      if (error instanceof RedirectError) {
        return { kind: "redirect", status: error.permanent ? 308 : 307, location: error.to };
      }
      throw error;
    }

    const errorFile = nearestBoundary(table.errors, resolved.pathname)?.file ?? null;
    const route = forTheBrowser(resolved, errorFile);
    const state = routeState(route);
    const marks = BOUNDARY_MARKS ? routeBoundaries(route, errorFile) : null;
    const tree = (
      <>
        {composeRoute(route, { page: pageElement(route), marks })}
        {BOUNDARY_MARKS && marks != null ? (
          <BoundaryReporter path={route.path} boundaries={marks} />
        ) : null}
      </>
    );
    const root: FlightRoot = { route: state, tree, deployment: options.deployment ?? null };
    const failure = renderFailure(route);
    if (failure != null) reportRequestError(failure, "render");
    const report = (error: mixed) => {
      reportRequestError(error, "render");
      if (settings?.onError != null) return settings.onError(error);
      console.error(error);
      return undefined;
    };
    // Inside the route's store, so a server component's `useRoute()` finds the
    // route however many `await`s into the render it asks.
    const stream = withServerRoute(state, () =>
      renderToReadableStream(root, {
        // Handed over as it is, so that the digest it returns reaches the row.
        // Our callback preserves React's console fallback when none was given.
        onError: report,
        signal: settings?.signal,
      }),
    );
    return { kind: "route", status: route.status, stream, failure: renderFailure(route) };
  };
}

async function resolveFlightInterception(
  table: RouteTable,
  url: string,
  from: ?string,
  defer: boolean,
): Promise<?ResolvedRoute> {
  const baseUrl = usableInterceptionBase(from);
  if (baseUrl == null) {
    return null;
  }
  try {
    const base = await resolveMatch(table, baseUrl, {
      defer,
      onMatch: noteRoute,
      runLoader: traceLoader,
    });
    return await resolveInterception(table, base, url);
  } catch (error) {
    if (error instanceof RedirectError) {
      return null;
    }
    throw error;
  }
}

function usableInterceptionBase(from: ?string): ?string {
  if (from == null || !from.startsWith("/") || from.startsWith("//")) {
    return null;
  }
  const hash = from.indexOf("#");
  return hash === -1 ? from : from.slice(0, hash);
}

/**
 * The page element: the route's page with its data, the deferred page that
 * waits for its loader, or the error route's page.
 *
 * An element for a component rather than a call, so that a page module with no
 * component in it throws while React renders — inside the boundaries the
 * composition placed — rather than before the render has begun.
 */
function pageElement(route: ResolvedRoute): React.Node {
  const error = route.error;
  if (error != null) {
    return <ErrorRoutePage module={route.errorBoundary.module} error={error} />;
  }
  const deferred = route.deferred;
  if (deferred != null) {
    return (
      <DeferredPage
        page={route.page}
        params={route.params}
        searchParams={route.searchParams}
        loader={deferred}
      />
    );
  }
  return (
    <RoutePage
      page={route.page}
      params={route.params}
      searchParams={route.searchParams}
      data={route.data}
    />
  );
}

/** A route's page, with its loader's answer. */
component RoutePage(
  page: PageModule,
  params: RouteParams,
  searchParams: SearchParams,
  data: mixed,
) {
  const Page = pageComponent(page);
  // The route module's own export, looked up by route: `pageComponent` hands
  // back its `default` or `Page` as it is, so this is the same component on
  // every render of the same route. The React Compiler cannot see through the
  // lookup and reports a component created during render.
  // uf-lint-disable-next-line react-compiler/static-components
  return <Page params={params} searchParams={searchParams} data={data} />;
}

/**
 * The same page, once the loader the router deferred has answered.
 *
 * `use` rather than an `async` component: it suspends at the same point, inside
 * the innermost `$loading.js` boundary, and it is the one spelling the
 * browser's `AwaitedPage` already uses for the same wait.
 */
component DeferredPage(
  page: PageModule,
  params: RouteParams,
  searchParams: SearchParams,
  loader: Promise<mixed>,
) {
  return <RoutePage page={page} params={params} searchParams={searchParams} data={use(loader)} />;
}

/**
 * The resolved route with every error boundary as the browser receives it.
 *
 * An error module is the server's module; what the browser needs is its
 * component, and in this graph a `"use client"` component *is* a client
 * reference. So each boundary becomes `{ default: <reference> }` — a plain
 * object Flight can carry — and a boundary whose component is not a reference
 * is refused with its file named.
 */
function forTheBrowser(resolved: ResolvedRoute, file: string | null): ResolvedRoute {
  const boundary = resolved.errorBoundary;
  return {
    ...resolved,
    errorBoundary: { above: boundary.above, module: clientErrorModule(boundary.module, file) },
    slots: resolved.slots.map(slotForTheBrowser),
  };
}

function slotForTheBrowser(slot: ResolvedSlot): ResolvedSlot {
  const boundary = slot.errorBoundary;
  return {
    ...slot,
    errorBoundary:
      boundary == null
        ? null
        : { above: boundary.above, module: clientErrorModule(boundary.module, null) },
    slots: slot.slots.map(slotForTheBrowser),
  };
}

function clientErrorModule(module: ?ErrorModule, file: string | null): ?ErrorModule {
  if (module == null) {
    return null;
  }
  const component = module.default ?? module.Error;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: an error module must export a component as `default` or `Error`",
    );
  }
  if (!isClientReference(component)) {
    throw new Error(
      `@uniflowed/router: ${file ?? "an `$error.js` inside a slot"} is an error boundary, and an ` +
        "error boundary catches a throw while the browser renders — so its component runs in " +
        "the browser, and the module has to open with the use client directive, as its first " +
        "statement.",
    );
  }
  return { default: component };
}

function isClientReference(value: mixed): boolean {
  if (value == null || (typeof value !== "object" && typeof value !== "function")) {
    return false;
  }
  const tagged: { readonly $$typeof?: mixed, ... } = value as $FlowFixMe;
  return tagged.$$typeof === CLIENT_REFERENCE;
}

/** The exception a resolved route fell back to its error boundary for. */
function renderFailure(resolved: ResolvedRoute): mixed {
  if (resolved.error == null) {
    return undefined;
  }
  return match (resolved.error) {
    {kind: "thrown", error: const error} => error,
    {kind: "unauthorized"} => undefined,
    {kind: "forbidden"} => undefined,
  };
}
