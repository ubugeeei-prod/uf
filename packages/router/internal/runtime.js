// @flow
//
// The router runtime: matching, loading, navigation, and the React binding.
//
// A route table is data — the virtual module `virtual:uf/routes` that
// `@uniflowed/vite` generates from the `app/` directory — and this module is
// everything that turns it into a running application. The same code runs on
// the server (`./server.js` renders one URL) and in the browser (`./client.js`
// hydrates it and then navigates), so a page's loader, layouts and metadata
// resolve identically in both places.

import * as React from "react";
import {
  createContext,
  startTransition,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from "react";

/** One parameter a route path captures. */
export type RouteParamSpec = {| readonly name: string, readonly catchAll: boolean |};

/** The parameters captured from a URL. A catch-all captures the rest as a list. */
export type RouteParams = { readonly [string]: string | $ReadOnlyArray<string> };

/** The query string, as a read-only map. */
export type SearchParams = { readonly [string]: string };

/**
 * A component found in a route module.
 *
 * `React.ComponentType<empty>` is "some React component", and it is a claim
 * rather than a shrug. `ComponentType` is contravariant in its props — Flow's
 * library definition writes it `component(...P)` with `in P` — so `empty` is
 * the *top* of the component types: every component is one, and nothing may be
 * passed to one until a caller has said which props it is passing. That is
 * exactly what is known here. The router finds these by dynamic import, and
 * nobody has told it what a page's props are.
 *
 * It cannot be `React.ComponentType<PageRenderProps>`, the props the router
 * actually passes, because Flow's `component` syntax gives a component *exact*
 * props and a page is free to want none of them. This repository's own pages
 * and layouts are `component NotFound()` and
 * `component Layout(children: React.Node)`, and against the props the router
 * hands them that reads:
 *
 *     error[incompatible-type]: property `data`, property `params`, and
 *     property `searchParams` are extra in `PageRenderProps` but missing in
 *     `props of component NotFound`. Exact objects do not accept extra props.
 *
 * React passing a component a prop it did not declare is allowed and always
 * has been. `renderable` is the one line that says so.
 */
type RouteComponent = React.ComponentType<empty>;

/**
 * The props `RouteView` gives the page it renders.
 *
 * The same three as the public `PageProps` in `../index.js`, at the arguments
 * the runtime instantiates it with: the runtime knows the parameters as
 * strings and the loader's data as `mixed`, and a page narrows both by
 * annotating its own props.
 */
type PageRenderProps = {|
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly data: mixed,
|};

/** The props `RouteView` gives each layout, outermost first. */
type LayoutRenderProps = {|
  readonly params: RouteParams,
  readonly children: React.Node,
|};

/** What a page module may export. The component is `default` or `Page`. */
export type PageModule = {
  readonly default?: RouteComponent,
  readonly Page?: RouteComponent,
  readonly loader?: (args: LoaderArgs) => mixed | Promise<mixed>,
  readonly metadata?: Metadata,
  readonly generateMetadata?: (args: MetadataArgs) => Metadata | Promise<Metadata>,
  readonly generateStaticParams?: () =>
    | $ReadOnlyArray<RouteParams>
    | Promise<$ReadOnlyArray<RouteParams>>,
  readonly frontmatter?: { readonly title?: string, readonly description?: string, ... },
  ...
};

/** What a layout module may export. The component is `default` or `Layout`. */
export type LayoutModule = {
  readonly default?: RouteComponent,
  readonly Layout?: RouteComponent,
  readonly metadata?: Metadata,
  ...
};

/**
 * What an error module may export. The component is `default` or `Error`.
 *
 * `Error` shadows the global inside the file that writes it, which is the
 * cost of naming the export after what it is; a file that needs the
 * constructor still has `globalThis.Error`. The alternative was a name the
 * convention would have to explain — `ErrorPage`, `Boundary` — for a file
 * whose whole job is already in its name.
 */
export type ErrorModule = {
  readonly default?: RouteComponent,
  readonly Error?: RouteComponent,
  readonly metadata?: Metadata,
  ...
};

/** Document metadata a page or layout declares. */
export type Metadata = {
  readonly title?: string,
  readonly description?: string,
  readonly openGraph?: {
    readonly title?: string,
    readonly description?: string,
    readonly images?: $ReadOnlyArray<string>,
  },
};

/** Arguments a loader receives. */
export type LoaderArgs = {|
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly pathname: string,
|};

/** Arguments `generateMetadata` receives. */
export type MetadataArgs = {|
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly data: mixed,
|};

/** One entry of the generated route table. */
export type RouteRecord = {|
  readonly path: string,
  readonly params: $ReadOnlyArray<RouteParamSpec>,
  readonly mdx: boolean,
  readonly file: string,
  readonly page: () => Promise<PageModule>,
  readonly layouts: $ReadOnlyArray<() => Promise<LayoutModule>>,
|};

/**
 * One not-found boundary: the page for a path under `path` that matched
 * nothing.
 *
 * `_uf.not-found.js` is a segment file, so `path` is the route path of the
 * directory that declares it and `layouts` are the layouts in scope *there* —
 * which is what the boundary renders inside. A project with one at the router
 * root has one of these; a project whose manual answers its own 404 has two.
 */
export type NotFoundBoundary = {|
  readonly path: string,
  readonly mdx: boolean,
  readonly file: string,
  readonly page: () => Promise<PageModule>,
  readonly layouts: $ReadOnlyArray<() => Promise<LayoutModule>>,
|};

/**
 * One error boundary: what renders in place of the subtree under `path` when
 * something in it throws.
 *
 * The same nearest-ancestor shape as [`NotFoundBoundary`], and `layouts` means
 * the same thing — the layouts in scope where the file is, which stay mounted
 * around the error and are why the rest of the document is still there.
 */
export type ErrorBoundary = {|
  readonly path: string,
  readonly file: string,
  readonly module: () => Promise<ErrorModule>,
  readonly layouts: $ReadOnlyArray<() => Promise<LayoutModule>>,
|};

/**
 * A route table plus the boundaries declared under it.
 *
 * `errors` is the error boundaries a project declared, not failures that
 * happened.
 */
export type RouteTable = {|
  readonly routes: $ReadOnlyArray<RouteRecord>,
  readonly notFound: $ReadOnlyArray<NotFoundBoundary>,
  readonly errors: $ReadOnlyArray<ErrorBoundary>,
|};

/** A URL matched against the table. */
export type RouteMatch = {|
  readonly route: RouteRecord,
  readonly params: RouteParams,
|};

/**
 * Why the router is rendering an error boundary instead of a page.
 *
 * One union rather than one file convention per status. `forbidden()` and
 * `unauthorized()` are not different *kinds* of file to write; they are
 * different sentences an error page says, and `match` over this is where a
 * page says all three and the checker confirms it covered them. Deciding it
 * the other way — `_uf.forbidden.js` and `_uf.unauthorized.js` beside
 * `_uf.error.js`, which is what Next.js does — is three files per segment to
 * express one thing, and nothing would check that any of them handled the
 * case it was named for.
 *
 * The thrown value is carried but deliberately not rendered by the default
 * boundary: a server exception's message is written for the person who
 * deployed the application, not for whoever asks for the page.
 */
export type RouteError =
  | {| readonly kind: "thrown", readonly error: mixed |}
  | {| readonly kind: "unauthorized" |}
  | {| readonly kind: "forbidden" |};

/** The status a `RouteError` answers with. */
export function routeErrorStatus(error: RouteError): 401 | 403 | 500 {
  return match (error) {
    {kind: "unauthorized"} => 401,
    {kind: "forbidden"} => 403,
    {kind: "thrown"} => 500,
  };
}

/**
 * A match whose modules are loaded and whose loader has run — or, when `error`
 * is set, the error page that stands in for it.
 */
export type ResolvedRoute = {|
  readonly pathname: string,
  readonly search: string,
  readonly path: string,
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly page: PageModule,
  readonly layouts: $ReadOnlyArray<LayoutModule>,
  readonly data: mixed,
  readonly metadata: Metadata,
  readonly status: 200 | 401 | 403 | 404 | 500,
  /**
   * Set when this resolution *is* the error page: the loader threw, or the
   * server render did and the renderer resolved again. `null` on the ordinary
   * path.
   */
  readonly error: ?RouteError,
  /**
   * The boundary that would catch a throw while rendering this route.
   *
   * Always present, because every route has an answer for a throw: `module`
   * is `null` when the project declares no `_uf.error.js` above the path, and
   * the framework's own error page renders instead. `above` is how many of
   * `layouts` are outside the boundary — the ones that stay mounted, which is
   * what "the rest of the document is still interactive" means.
   */
  readonly errorBoundary: {|
    readonly module: ?ErrorModule,
    readonly above: number,
  |},
|};

/** Thrown by `notFound()`; the renderer answers with the not-found page. */
export class NotFoundError extends Error {
  constructor() {
    super("not found");
    this.name = "NotFoundError";
  }
}

/** Thrown by `unauthorized()`; the renderer answers with the error boundary. */
export class UnauthorizedError extends Error {
  constructor() {
    super("unauthorized");
    this.name = "UnauthorizedError";
  }
}

/** Thrown by `forbidden()`; the renderer answers with the error boundary. */
export class ForbiddenError extends Error {
  constructor() {
    super("forbidden");
    this.name = "ForbiddenError";
  }
}

/** Thrown by `redirect()`; the renderer answers with a redirect. */
export class RedirectError extends Error {
  to: string;
  permanent: boolean;

  constructor(to: string, permanent: boolean) {
    super(`redirect to ${to}`);
    this.name = "RedirectError";
    this.to = to;
    this.permanent = permanent;
  }
}

// ---------------------------------------------------------------------------
// Matching
// ---------------------------------------------------------------------------

type Segment =
  | {| readonly kind: "static", readonly value: string |}
  | {| readonly kind: "param", readonly name: string |}
  | {| readonly kind: "catchAll", readonly name: string |};

function compile(routePath: string): $ReadOnlyArray<Segment> {
  return routePath
    .split("/")
    .filter((segment) => segment !== "")
    .map((segment): Segment => {
      if (segment.startsWith(":") && segment.endsWith("*")) {
        return { kind: "catchAll", name: segment.slice(1, -1) };
      }
      if (segment.startsWith(":")) {
        return { kind: "param", name: segment.slice(1) };
      }
      return { kind: "static", value: segment };
    });
}

/**
 * How specific a route is, for ranking: a static segment outranks a parameter,
 * which outranks a catch-all, and a longer path outranks a shorter one.
 */
function specificity(segments: $ReadOnlyArray<Segment>): number {
  let score = 0;
  for (const segment of segments) {
    score += match (segment) {
      {kind: "static"} => 3,
      {kind: "param"} => 2,
      {kind: "catchAll"} => 1,
    };
  }
  return score;
}

function matchSegments(
  segments: $ReadOnlyArray<Segment>,
  parts: $ReadOnlyArray<string>,
): ?RouteParams {
  const params: { [string]: string | $ReadOnlyArray<string> } = {};
  let index = 0;
  for (const segment of segments) {
    match (segment) {
      {kind: "static", value: const value} => {
        if (parts[index] !== value) {
          return null;
        }
        index += 1;
      }
      {kind: "param", name: const name} => {
        if (index >= parts.length) {
          return null;
        }
        params[name] = decodeSegment(parts[index]);
        index += 1;
      }
      {kind: "catchAll", name: const name} => {
        params[name] = parts.slice(index).map(decodeSegment);
        index = parts.length;
      }
    }
  }
  return index === parts.length ? params : null;
}

function decodeSegment(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}

/**
 * Match a pathname against the table, preferring the most specific route.
 */
export function matchRoute(routes: $ReadOnlyArray<RouteRecord>, pathname: string): ?RouteMatch {
  const parts = pathname.split("/").filter((part) => part !== "");
  let best: ?RouteMatch = null;
  let bestScore = -1;
  for (const route of routes) {
    const segments = compile(route.path);
    const params = matchSegments(segments, parts);
    if (params == null) {
      continue;
    }
    const score = specificity(segments);
    if (score > bestScore) {
      best = { route, params };
      bestScore = score;
    }
  }
  return best;
}

/**
 * Whether a boundary declared at `segments` is at or above `parts`.
 *
 * The same segment kinds as [`matchSegments`], stopping when the boundary's
 * own segments run out instead of requiring the path to: `/guide` covers
 * `/guide/nope`, and `/guide` covers `/guide` itself.
 */
function covers(segments: $ReadOnlyArray<Segment>, parts: $ReadOnlyArray<string>): boolean {
  let index = 0;
  for (const segment of segments) {
    const next = match (segment) {
      {kind: "static", value: const value} => parts[index] === value ? index + 1 : -1,
      {kind: "param"} => index < parts.length ? index + 1 : -1,
      {kind: "catchAll"} => parts.length,
    };
    if (next === -1) {
      return false;
    }
    index = next;
  }
  return true;
}

/**
 * The nearest boundary above `pathname`, or `null` when none covers it.
 *
 * The one rule both `_uf.not-found.js` and `_uf.error.js` are resolved by, and
 * the same one layouts already follow: nearest means the longest path that
 * covers the URL. It is decided here rather than by the table's order — the
 * table is sorted by path so the generated module is stable, and a resolver
 * that read "nearest" as "first" would silently depend on that sort. Two
 * boundaries can share a path (a route group's directory does not appear in
 * the URL), and then the first in the table wins.
 */
function nearestBoundary<TBoundary: { readonly path: string, ... }>(
  boundaries: $ReadOnlyArray<TBoundary>,
  pathname: string,
): ?TBoundary {
  const parts = pathname.split("/").filter((part) => part !== "");
  let best: ?TBoundary = null;
  let bestDepth = -1;
  for (const boundary of boundaries) {
    const segments = compile(boundary.path);
    if (!covers(segments, parts)) {
      continue;
    }
    if (segments.length > bestDepth) {
      best = boundary;
      bestDepth = segments.length;
    }
  }
  return best;
}

/** Split a URL into its pathname and search string. */
export function splitUrl(url: string): {| readonly pathname: string, readonly search: string |} {
  const hash = url.indexOf("#");
  const withoutHash = hash === -1 ? url : url.slice(0, hash);
  const question = withoutHash.indexOf("?");
  if (question === -1) {
    return { pathname: normalizePathname(withoutHash), search: "" };
  }
  return {
    pathname: normalizePathname(withoutHash.slice(0, question)),
    search: withoutHash.slice(question),
  };
}

function normalizePathname(pathname: string): string {
  if (pathname === "" || pathname === "/") {
    return "/";
  }
  const trimmed = pathname.replace(/\/+$/, "");
  return trimmed === "" ? "/" : trimmed;
}

/** Parse a search string into a flat map; a repeated key keeps its last value. */
export function parseSearch(search: string): SearchParams {
  const params: { [string]: string } = {};
  for (const [key, value] of new URLSearchParams(search)) {
    params[key] = value;
  }
  return params;
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

const moduleCache: Map<() => Promise<mixed>, Promise<mixed>> = new Map();

function loadOnce<T>(load: () => Promise<T>): Promise<T> {
  let pending = moduleCache.get(load);
  if (pending == null) {
    pending = load();
    moduleCache.set(load, pending);
  }
  // $FlowFixMe[incompatible-return] the cache is keyed by the loader, whose result type it stores.
  return pending;
}

/**
 * Load a match's modules and run its loader.
 *
 * `data` is what the loader returned; on the client after hydration it is the
 * value the server embedded, so the loader does not run twice for the first
 * page.
 *
 * # This resolves or redirects; it does not reject
 *
 * Everything a route can go wrong with is a route to render: no match and
 * `notFound()` are the not-found boundary, a loader that threw and
 * `forbidden()`/`unauthorized()` are the error boundary. Only `redirect()`
 * comes back out, because a redirect is a response rather than a page and the
 * caller is what has one to send.
 *
 * That guarantee is the point rather than a convenience. `hydrate` awaits this
 * before `hydrateRoot`, so a rejection there is not an error page — it is no
 * `hydrateRoot` call at all, and the document the server sent stays on screen
 * with nothing attached to it.
 */
export async function resolveMatch(
  table: RouteTable,
  url: string,
  options?: {| readonly data?: mixed, readonly skipLoader?: boolean |},
): Promise<ResolvedRoute> {
  try {
    return await resolveRoute(table, url, options);
  } catch (error) {
    if (error instanceof RedirectError) {
      throw error;
    }
    return resolveFailure(table, url, error);
  }
}

async function resolveRoute(
  table: RouteTable,
  url: string,
  options?: {| readonly data?: mixed, readonly skipLoader?: boolean |},
): Promise<ResolvedRoute> {
  const { pathname, search } = splitUrl(url);
  const searchParams = parseSearch(search);
  const matched = matchRoute(table.routes, pathname);

  if (matched == null) {
    return resolveNotFound(table, pathname, search, searchParams);
  }

  const [page, ...layouts] = await Promise.all([
    loadOnce(matched.route.page),
    ...matched.route.layouts.map((layout) => loadOnce(layout)),
  ]);
  // Started here and awaited at the end: the boundary's module does not depend
  // on the loader, so importing it alongside costs a navigation nothing. It
  // never rejects, so an early throw below leaves no unhandled rejection.
  const boundary = resolveErrorBoundary(table, pathname, matched.route.layouts.length);

  let data: mixed = options?.data;
  if (options?.skipLoader !== true && typeof page.loader === "function") {
    data = await page.loader({ params: matched.params, searchParams, pathname });
  }

  const metadata = await resolveMetadata(page, layouts, {
    params: matched.params,
    searchParams,
    data,
  });
  return {
    pathname,
    search,
    path: matched.route.path,
    params: matched.params,
    searchParams,
    page,
    layouts,
    data,
    metadata,
    status: 200,
    error: null,
    errorBoundary: await boundary,
  };
}

/**
 * The route to render after something threw.
 *
 * Two callers, one behaviour: [`resolveMatch`] when a loader or a module
 * import threw, and `createRenderer` when the *render* did — React's error
 * boundaries do not run in `renderToString`, so the server has to catch it
 * itself and resolve again.
 */
export async function resolveFailure(
  table: RouteTable,
  url: string,
  error: mixed,
): Promise<ResolvedRoute> {
  const { pathname, search } = splitUrl(url);
  const searchParams = parseSearch(search);
  if (error instanceof NotFoundError) {
    try {
      return await resolveNotFound(table, pathname, search, searchParams);
    } catch (failure) {
      // The not-found page itself would not load. Falling through to the error
      // boundary rather than rethrowing is what keeps the promise above: the
      // page a project wrote to explain a 404 is not more load-bearing than
      // the document staying on screen.
      return resolveError(table, pathname, search, searchParams, routeErrorFor(failure));
    }
  }
  return resolveError(table, pathname, search, searchParams, routeErrorFor(error));
}

/** What a thrown value means to the router. */
function routeErrorFor(error: mixed): RouteError {
  if (error instanceof UnauthorizedError) {
    return { kind: "unauthorized" };
  }
  if (error instanceof ForbiddenError) {
    return { kind: "forbidden" };
  }
  return { kind: "thrown", error };
}

/**
 * The error boundary a route renders inside, loaded with the route rather than
 * when it is needed.
 *
 * React decides to show a boundary's fallback synchronously, during the render
 * that threw. A module that still has to be imported is a module that is not
 * there at the only moment it can be used, so this is one more dynamic import
 * per navigation and not a lazy one.
 *
 * `above` is the boundary's own layout count, clamped to the route's. The
 * first attempt compared the two layout arrays for a shared prefix, which is
 * more precise when a `(group)` directory puts a boundary beside a route
 * rather than above it — and it worked by *reference identity* of the loader
 * functions, which holds only because `routesModuleSource` deduplicates them
 * by file. A rule that depends on an invisible property of the generated
 * module is a rule that reads as zero the moment a table is built any other
 * way, and it did: it put the boundary outside the layouts it was written
 * inside. Nesting a boundary per group needs parallel-route trees (#267);
 * until then this is the honest approximation, and it is stated rather than
 * inferred.
 */
async function resolveErrorBoundary(
  table: RouteTable,
  pathname: string,
  layoutCount: number,
): Promise<{| readonly module: ?ErrorModule, readonly above: number |}> {
  const boundary = nearestBoundary(table.errors, pathname);
  if (boundary == null) {
    return { module: null, above: 0 };
  }
  // Clamped, because a route group can leave a route with fewer layouts than
  // the boundary covering it, and an `above` past the end would compose the
  // layouts out of nothing.
  const above = Math.min(boundary.layouts.length, layoutCount);
  try {
    return { module: await loadOnce(boundary.module), above };
  } catch {
    // A boundary whose module will not load cannot be the answer to a throw,
    // and this is why the field is nullable: containment must not itself
    // depend on an import working.
    return { module: null, above: 0 };
  }
}

/**
 * The error page for `pathname`, inside the layouts above the boundary that
 * answers it.
 *
 * The layouts are the boundary's, for the same reason [`resolveNotFound`]
 * gives: they are what stays mounted around the error, and the layouts below
 * the boundary belong to the subtree that just stopped.
 */
async function resolveError(
  table: RouteTable,
  pathname: string,
  search: string,
  searchParams: SearchParams,
  routeError: RouteError,
): Promise<ResolvedRoute> {
  const boundary = nearestBoundary(table.errors, pathname);
  let module: ?ErrorModule = null;
  let layouts: $ReadOnlyArray<LayoutModule> = [];
  if (boundary != null) {
    try {
      [module, layouts] = await Promise.all([
        loadOnce(boundary.module),
        Promise.all(boundary.layouts.map((layout) => loadOnce(layout))),
      ]);
    } catch {
      // See `resolveErrorBoundary`: the framework's own page answers instead.
      module = null;
      layouts = [];
    }
  }

  const declared = await resolveMetadata(
    module?.metadata != null ? { metadata: module.metadata } : {},
    layouts,
    { params: {}, searchParams, data: undefined },
  );
  return {
    pathname,
    search,
    path: "*",
    params: {},
    searchParams,
    page: { default: ResolvedErrorPage },
    layouts,
    data: undefined,
    metadata: declared.title != null ? declared : { ...declared, title: errorTitle(routeError) },
    status: routeErrorStatus(routeError),
    error: routeError,
    // All of the boundary's layouts are above it, and no inner boundary is
    // inserted around a page that already is one; see `RouteView`.
    errorBoundary: { module, above: layouts.length },
  };
}

/**
 * The not-found page for `pathname`, inside the layouts above the boundary
 * that answers it.
 *
 * The layouts are the *boundary's*, not the ones the URL had already matched.
 * Taking the matched route's layouts was the other candidate and it is wrong
 * in both directions: for an unmatched URL there is no matched route to take
 * them from, and for `notFound()` thrown from a page they would keep the
 * layouts *below* the boundary — so `app/guide/[slug]/_uf.layout.js` would
 * wrap a 404 that `app/guide/_uf.not-found.js` answered, which is the layout
 * of the page that just said it does not exist.
 */
async function resolveNotFound(
  table: RouteTable,
  pathname: string,
  search: string,
  searchParams: SearchParams,
): Promise<ResolvedRoute> {
  const record = nearestBoundary(table.notFound, pathname);
  if (record == null) {
    return {
      pathname,
      search,
      path: "*",
      params: {},
      searchParams,
      page: { default: DefaultNotFound },
      layouts: [],
      data: undefined,
      metadata: { title: "Not found" },
      status: 404,
      error: null,
      errorBoundary: await resolveErrorBoundary(table, pathname, 0),
    };
  }
  const [page, ...layouts] = await Promise.all([
    loadOnce(record.page),
    ...record.layouts.map((layout) => loadOnce(layout)),
  ]);
  const metadata = await resolveMetadata(page, layouts, {
    params: {},
    searchParams,
    data: undefined,
  });
  return {
    pathname,
    search,
    path: "*",
    params: {},
    searchParams,
    page,
    layouts,
    data: undefined,
    metadata,
    status: 404,
    error: null,
    // A not-found page is a page: one that throws is contained like any other.
    errorBoundary: await resolveErrorBoundary(table, pathname, layouts.length),
  };
}

async function resolveMetadata(
  page: PageModule,
  layouts: $ReadOnlyArray<LayoutModule>,
  args: MetadataArgs,
): Promise<Metadata> {
  let merged: Metadata = {};
  for (const layout of layouts) {
    if (layout.metadata != null) {
      merged = { ...merged, ...layout.metadata };
    }
  }
  if (page.frontmatter != null) {
    const { title, description } = page.frontmatter;
    merged = {
      ...merged,
      ...(title != null ? { title } : {}),
      ...(description != null ? { description } : {}),
    };
  }
  if (page.metadata != null) {
    merged = { ...merged, ...page.metadata };
  }
  if (typeof page.generateMetadata === "function") {
    merged = { ...merged, ...(await page.generateMetadata(args)) };
  }
  return merged;
}

component DefaultNotFound() {
  return (
    <main>
      <title>Not found</title>
      <h1>404</h1>
      <p>This page does not exist.</p>
    </main>
  );
}

/** The document title an error page gets when nothing declared one. */
function errorTitle(error: RouteError): string {
  return match (error) {
    {kind: "unauthorized"} => "Sign in required",
    {kind: "forbidden"} => "Not allowed",
    {kind: "thrown"} => "Something went wrong",
  };
}

/**
 * The framework's error page, for a project that declares no `_uf.error.js`.
 *
 * It says which of the three happened and offers the reset, and it does *not*
 * print the thrown error: on the server that message is written for whoever
 * deployed the application — a query, a path, a token in a stack — and this
 * markup is sent to whoever asked for the page. `uf dev` reports the throw in
 * the terminal and `uf build` fails the route, which are the places the person
 * who can act on it is looking.
 */
component DefaultRouteError(error: RouteError, reset: () => void) {
  const title = errorTitle(error);
  const detail = match (error) {
    {kind: "unauthorized"} => "This page needs you to be signed in.",
    {kind: "forbidden"} => "You do not have access to this page.",
    {kind: "thrown"} => "This page could not be rendered.",
  };
  return (
    <main>
      <title>{title}</title>
      <h1>{title}</h1>
      <p>{detail}</p>
      <button type="button" onClick={reset}>
        Try again
      </button>
    </main>
  );
}

/** The component an error module renders: `default`, or the named `Error`. */
function errorComponent(module: ErrorModule): React.ComponentType<ErrorRenderProps> {
  const component = module.default ?? module.Error;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: an error module must export a component as `default` or `Error`",
    );
  }
  return renderable(component);
}

/** The props an error boundary's component receives. */
type ErrorRenderProps = {|
  readonly error: RouteError,
  readonly reset: () => void,
|};

/**
 * The error UI, from whichever module is in scope.
 *
 * One component for both ways in — the class boundary below, which catches a
 * throw while the browser renders, and `ResolvedErrorPage`, which is what the
 * server renders because React's boundaries do not run in `renderToString`.
 * Two paths to the same screen is exactly the pair that drifts.
 */
component RouteErrorView(module: ?ErrorModule, error: RouteError, reset: () => void) {
  if (module == null) {
    return <DefaultRouteError error={error} reset={reset} />;
  }
  const Boundary = errorComponent(module);
  return <Boundary error={error} reset={reset} />;
}

/**
 * The page of a route that resolved to an error.
 *
 * A resolved error route carries the error and the module on the route itself,
 * so this is a static component rather than a closure the resolver builds:
 * `RouteView` composes it in its layouts exactly like a page, which is what
 * makes "inside the layouts above the boundary" one code path and not two.
 *
 * `reset()` here is `router.refresh()` — this route resolved to an error
 * because a loader or an import threw, so re-running the resolution is what
 * trying again means. On the server `refresh` does nothing, which is correct:
 * a static render has nothing to re-run.
 */
component ResolvedErrorPage() {
  const { resolved, router } = useRouterState();
  const reset = useCallback(() => {
    router.refresh().catch(() => {});
  }, [router]);

  if (resolved.error == null) {
    // Unreachable: this module is only ever the page of a resolved error route.
    return null;
  }
  return (
    <RouteErrorView module={resolved.errorBoundary.module} error={resolved.error} reset={reset} />
  );
}

type RouteErrorBoundaryProps = {|
  readonly module: ?ErrorModule,
  readonly resetKey: string,
  readonly children: React.Node,
|};

type RouteErrorBoundaryState = {| readonly error: ?RouteError |};

/**
 * The boundary that catches a throw while the browser renders the subtree.
 *
 * A class, because `getDerivedStateFromError` is React's contract for this and
 * there is no hook that does it — this is the one place in the router where
 * following React's public contract means not using a function component.
 *
 * Recovering on navigation is `componentDidUpdate` watching `resetKey`, not
 * `key={pathname}` on the boundary. Keying it remounts the subtree on *every*
 * navigation, error or not, and everything below the boundary goes with it —
 * which is the layouts, whose whole purpose is to survive navigation with
 * their scroll position and their open sections intact.
 */
class RouteErrorBoundary extends React.Component<RouteErrorBoundaryProps, RouteErrorBoundaryState> {
  constructor(props: RouteErrorBoundaryProps) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: mixed): RouteErrorBoundaryState {
    return { error: routeErrorFor(error) };
  }

  componentDidUpdate(previous: RouteErrorBoundaryProps) {
    if (this.state.error != null && previous.resetKey !== this.props.resetKey) {
      this.setState({ error: null });
    }
  }

  render(): React.Node {
    const { error } = this.state;
    if (error == null) {
      return this.props.children;
    }
    return (
      <RouteErrorView
        module={this.props.module}
        error={error}
        reset={() => this.setState({ error: null })}
      />
    );
  }
}

// ---------------------------------------------------------------------------
// The React binding
// ---------------------------------------------------------------------------

/** How a navigation is performed. */
export type NavigateOptions = {| readonly replace?: boolean, readonly scroll?: boolean |};

/** What `useRouter()` returns. */
export type Router = {|
  readonly push: (to: string, options?: NavigateOptions) => Promise<void>,
  readonly replace: (to: string) => Promise<void>,
  readonly prefetch: (to: string) => Promise<void>,
  readonly refresh: () => Promise<void>,
  readonly back: () => void,
  readonly forward: () => void,
|};

/** What `useRoute()` returns. */
export type RouteInfo = {|
  readonly path: string,
  readonly pathname: string,
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly data: mixed,
  readonly pending: boolean,
|};

type RouterState = {|
  readonly resolved: ResolvedRoute,
  readonly router: Router,
  readonly pending: boolean,
|};

const RouterContext: React.Context<?RouterState> = createContext(null);

/** The route table the application was started with. */
let installedTable: ?RouteTable = null;

/** Register the generated route table. Called once by the client and server entries. */
export function installRoutes(table: RouteTable): void {
  installedTable = table;
}

/** The registered table, or a clear error when the entry forgot to install it. */
export function routeTable(): RouteTable {
  if (installedTable == null) {
    throw new Error(
      "@uniflowed/router: no route table is installed; start the app through `uf dev` or `uf build`",
    );
  }
  return installedTable;
}

/** Props the app root receives from the client and server entries. */
export type AppProps = {|
  readonly url: string,
  readonly initial: ResolvedRoute,
|};

const isBrowser = typeof window !== "undefined" && typeof document !== "undefined";

/**
 * Provides the current route to the tree and performs navigation.
 *
 * On the server the route is fixed for the request. In the browser the
 * provider listens to history and to `Link` clicks; a navigation resolves the
 * next route (loading its chunks and running its loader) *before* committing,
 * inside a transition, so the previous page stays interactive meanwhile.
 */
export component RouterProvider(url: string, initial: ResolvedRoute, children: React.Node) {
  const [resolved, setResolved] = useState<ResolvedRoute>(initial);
  const [pending, setPending] = useState<boolean>(false);

  const navigate = useCallback(async (to: string, options?: NavigateOptions): Promise<void> => {
    if (!isBrowser) {
      return;
    }
    const target = new URL(to, window.location.href);
    const next = target.pathname + target.search;
    setPending(true);
    try {
      const nextResolved = await resolveMatch(routeTable(), next);
      if (options?.replace === true) {
        window.history.replaceState(null, "", next + target.hash);
      } else {
        window.history.pushState(null, "", next + target.hash);
      }
      startTransition(() => {
        setResolved(nextResolved);
        setPending(false);
      });
      if (options?.scroll !== false) {
        if (target.hash !== "") {
          const element = document.getElementById(target.hash.slice(1));
          if (element != null) {
            element.scrollIntoView();
            return;
          }
        }
        window.scrollTo(0, 0);
      }
    } catch (error) {
      setPending(false);
      throw error;
    }
  }, []);

  useEffect(() => {
    if (!isBrowser) {
      return undefined;
    }
    const onPopState = () => {
      const next = window.location.pathname + window.location.search;
      resolveMatch(routeTable(), next).then((nextResolved) => {
        startTransition(() => {
          setResolved(nextResolved);
        });
      });
    };
    window.addEventListener("popstate", onPopState);
    return () => {
      window.removeEventListener("popstate", onPopState);
    };
  }, []);

  const router = useMemo<Router>(
    () => ({
      push: (to, options) => navigate(to, options),
      replace: (to) => navigate(to, { replace: true }),
      prefetch: async (to) => {
        if (!isBrowser) {
          return;
        }
        const target = new URL(to, window.location.href);
        const matched = matchRoute(routeTable().routes, target.pathname);
        if (matched == null) {
          return;
        }
        await Promise.all([
          loadOnce(matched.route.page),
          ...matched.route.layouts.map((layout) => loadOnce(layout)),
        ]);
      },
      refresh: async () => {
        if (!isBrowser) {
          return;
        }
        const nextResolved = await resolveMatch(
          routeTable(),
          window.location.pathname + window.location.search,
        );
        startTransition(() => {
          setResolved(nextResolved);
        });
      },
      back: () => {
        if (isBrowser) {
          window.history.back();
        }
      },
      forward: () => {
        if (isBrowser) {
          window.history.forward();
        }
      },
    }),
    [navigate],
  );

  const value = useMemo<RouterState>(
    () => ({ resolved, router, pending }),
    [resolved, router, pending],
  );
  return <RouterContext.Provider value={value}>{children}</RouterContext.Provider>;
}

hook useRouterState(): RouterState {
  const state = useContext(RouterContext);
  if (state == null) {
    throw new Error(
      "@uniflowed/router: this hook must be used inside the app started by `routerView`",
    );
  }
  return state;
}

/** The current route. */
export hook useRoute(): RouteInfo {
  const { resolved, pending } = useRouterState();
  return {
    path: resolved.path,
    pathname: resolved.pathname,
    params: resolved.params,
    searchParams: resolved.searchParams,
    data: resolved.data,
    pending,
  };
}

/** Navigation. */
export hook useRouter(): Router {
  return useRouterState().router;
}

/**
 * The current page's loader data.
 *
 * `mixed`, so the page that reads it says what it is and the checker watches
 * it do so. This was `useLoaderData<T>(): T`, which looks like inference and
 * is a cast a caller writes at a distance: `useLoaderData<Post>()` asserted
 * that a loader three files away returned a `Post` and nothing anywhere
 * checked it, so a loader that changed shape produced a `Post`-shaped
 * `undefined` at the first property read rather than an error where the shape
 * was decided.
 *
 * Narrowing is a line at the top of the page — `if (typeof data !== "object"
 * || data == null) { … }`, or the page's own validator schema, which is what
 * `@uniflowed/validator` is for at exactly this boundary.
 *
 * The type that would need no narrowing is a *generated* one: the route table
 * already produces `RoutePath` and `RouteParams` from the `app/` directory
 * (`crates/uf_router/src/lib.rs`), and a loader's return type belongs in the
 * same file, keyed by route. Until it is there, this says what is true.
 */
export hook useLoaderData(): mixed {
  return useRouterState().resolved.data;
}

/**
 * Renders the matched page inside its layouts, innermost last, with the
 * document metadata as hoistable head elements.
 *
 * # Where the error boundaries go
 *
 * Two, and they are not the same thing twice. The inner one is the project's
 * `_uf.error.js`, placed at the depth the file sits at, so the layouts above
 * it stay mounted and interactive while the subtree below is replaced — that
 * placement *is* the feature. The outer one has no module and so renders the
 * framework's page; it is what stands between a throw in a root layout, or in
 * the error component itself, and an unmounted document. A single boundary
 * cannot be both: put it outside and a page's throw takes the navigation down
 * with it; put it inside and nothing catches the layout above.
 */
export component RouteView() {
  const { resolved } = useRouterState();
  const { module, above } = resolved.errorBoundary;
  const Page = pageComponent(resolved.page);
  let element: React.Node = (
    <Page params={resolved.params} searchParams={resolved.searchParams} data={resolved.data} />
  );
  for (let index = resolved.layouts.length - 1; index >= above; index -= 1) {
    const Layout = layoutComponent(resolved.layouts[index]);
    element = <Layout params={resolved.params}>{element}</Layout>;
  }
  // Not around a route that already resolved to its error page: that page is
  // the boundary's own component, and wrapping it in the same boundary would
  // answer a throw inside it with itself.
  if (module != null && resolved.error == null) {
    element = (
      <RouteErrorBoundary module={module} resetKey={resolved.pathname}>
        {element}
      </RouteErrorBoundary>
    );
  }
  for (let index = above - 1; index >= 0; index -= 1) {
    const Layout = layoutComponent(resolved.layouts[index]);
    element = <Layout params={resolved.params}>{element}</Layout>;
  }
  return (
    <>
      <Head metadata={resolved.metadata} />
      <RouteErrorBoundary module={null} resetKey={resolved.pathname}>
        {element}
      </RouteErrorBoundary>
    </>
  );
}

/**
 * The component a page module renders: its default export, or the named
 * `Page` that `uf create` scaffolds. An MDX page always has a default export.
 */
function pageComponent(module: PageModule): React.ComponentType<PageRenderProps> {
  const component = module.default ?? module.Page;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: a page module must export a component as `default` or `Page`",
    );
  }
  return renderable(component);
}

/** The component a layout module renders: `default`, or the named `Layout`. */
function layoutComponent(module: LayoutModule): React.ComponentType<LayoutRenderProps> {
  const component = module.default ?? module.Layout;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: a layout module must export a component as `default` or `Layout`",
    );
  }
  return renderable(component);
}

/**
 * A route module's component, as the router is about to render it.
 *
 * # The one cast in this file, and why it is here rather than in six places
 *
 * A `RouteComponent` is a component about whose props nothing was claimed, and
 * `RouteView` is about to pass it three. React allows that — a component
 * receives the props its parent wrote and ignores the ones it did not declare
 * — but Flow cannot be told it: a page's props are exact, so no props type but
 * that page's own is assignable, and the router does not know which page it
 * has. `React.ComponentType<any>` on the module types was this same
 * unsoundness spread over six declarations, where it also stopped anyone from
 * checking that `RouteView` passes the props a page is documented to receive.
 * Here it is one line, and everything on either side of it is checked: what a
 * module may export, and what a page is handed.
 */
function renderable<TProps extends { ... }>(
  component: RouteComponent,
): React.ComponentType<TProps> {
  return component as any;
}

component Head(metadata: Metadata) {
  const { title, description, openGraph } = metadata;
  return (
    <>
      {title != null ? <title>{title}</title> : null}
      {description != null ? <meta name="description" content={description} /> : null}
      {openGraph?.title != null ? <meta property="og:title" content={openGraph.title} /> : null}
      {openGraph?.description != null ? (
        <meta property="og:description" content={openGraph.description} />
      ) : null}
      {openGraph?.images != null
        ? openGraph.images.map((image) => <meta key={image} property="og:image" content={image} />)
        : null}
    </>
  );
}

/** When a `Link` loads the route it points at. */
export type LinkPrefetch = "off" | "intent" | "render";

/**
 * A client-side navigation.
 *
 * Renders a real anchor, so the link works before hydration and for a right
 * click, and takes over only a plain left click. `prefetch="intent"` (the
 * default) loads the destination's chunks on hover or focus.
 */
export component Link(
  to: string,
  prefetch?: LinkPrefetch = "intent",
  replace?: boolean = false,
  children?: React.Node,
  className?: string,
  onClick?: (event: SyntheticMouseEvent<HTMLAnchorElement>) => mixed,
  ...rest: { readonly [string]: mixed }
) {
  const router = useRouter();
  const prefetched = React.useRef(false);

  const doPrefetch = () => {
    if (prefetch === "off" || prefetched.current || isExternal(to)) {
      return;
    }
    prefetched.current = true;
    router.prefetch(to).catch(() => {});
  };

  useEffect(() => {
    if (prefetch === "render") {
      doPrefetch();
    }
  });

  const handleClick = (event: SyntheticMouseEvent<HTMLAnchorElement>) => {
    if (onClick != null) {
      onClick(event);
    }
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey ||
      isExternal(to)
    ) {
      return;
    }
    event.preventDefault();
    router.push(to, { replace }).catch((error) => {
      // A failed navigation falls back to the browser doing it.
      console.error(error);
      window.location.assign(to);
    });
  };

  return (
    <a
      {...rest}
      href={to}
      className={className}
      onClick={handleClick}
      onMouseEnter={prefetch === "intent" ? doPrefetch : undefined}
      onFocus={prefetch === "intent" ? doPrefetch : undefined}
    >
      {children}
    </a>
  );
}

function isExternal(to: string): boolean {
  return /^[a-z][a-z0-9+.-]*:/i.test(to) || to.startsWith("//");
}

/**
 * The application root `app.js` exports: `export default routerView("./app")`.
 *
 * The argument documents where the routes live; the table itself is generated
 * from that directory at build time and installed by the entry that starts
 * the app, so the component only has to render it.
 */
export function routerView(root: string): React.ComponentType<AppProps> {
  void root;
  component App(url: string, initial: ResolvedRoute) {
    return (
      <RouterProvider url={url} initial={initial}>
        <RouteView />
      </RouterProvider>
    );
  }
  return App;
}

/** Stop rendering the current page and show the not-found page instead. */
export function notFound(): empty {
  throw new NotFoundError();
}

/** Stop rendering the current page and show the error boundary, as a 401. */
export function unauthorized(): empty {
  throw new UnauthorizedError();
}

/** Stop rendering the current page and show the error boundary, as a 403. */
export function forbidden(): empty {
  throw new ForbiddenError();
}

/** Stop rendering the current page and send the visitor elsewhere. */
export function redirect(to: string): empty {
  throw new RedirectError(to, false);
}

/** `redirect`, with a permanent status. */
export function permanentRedirect(to: string): empty {
  throw new RedirectError(to, true);
}

/**
 * Whether the app is being rendered on the server.
 *
 * Read through `useSyncExternalStore` so a component that branches on it
 * hydrates consistently: the server snapshot is `true`, the client one `false`.
 */
export hook useIsServer(): boolean {
  return useSyncExternalStore(
    () => () => {},
    () => false,
    () => true,
  );
}
