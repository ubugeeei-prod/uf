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

/** What a page module may export. The component is `default` or `Page`. */
export type PageModule = {
  readonly default?: React.ComponentType<any>,
  readonly Page?: React.ComponentType<any>,
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
  readonly default?: React.ComponentType<any>,
  readonly Layout?: React.ComponentType<any>,
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

/** The not-found page, when the app declares one. */
export type NotFoundRecord = {|
  readonly mdx: boolean,
  readonly file: string,
  readonly page: () => Promise<PageModule>,
  readonly layouts: $ReadOnlyArray<() => Promise<LayoutModule>>,
|};

/** A route table plus the not-found page. */
export type RouteTable = {|
  readonly routes: $ReadOnlyArray<RouteRecord>,
  readonly notFound: ?NotFoundRecord,
|};

/** A URL matched against the table. */
export type RouteMatch = {|
  readonly route: RouteRecord,
  readonly params: RouteParams,
|};

/** A match whose modules are loaded and whose loader has run. */
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
  readonly status: 200 | 404,
|};

/** Thrown by `notFound()`; the renderer answers with the not-found page. */
export class NotFoundError extends Error {
  constructor() {
    super("not found");
    this.name = "NotFoundError";
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
 */
export async function resolveMatch(
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

  let data: mixed = options?.data;
  if (options?.skipLoader !== true && typeof page.loader === "function") {
    try {
      data = await page.loader({ params: matched.params, searchParams, pathname });
    } catch (error) {
      if (error instanceof NotFoundError) {
        return resolveNotFound(table, pathname, search, searchParams);
      }
      throw error;
    }
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
  };
}

async function resolveNotFound(
  table: RouteTable,
  pathname: string,
  search: string,
  searchParams: SearchParams,
): Promise<ResolvedRoute> {
  const record = table.notFound;
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
      ...title != null ? { title } : {},
      ...description != null ? { description } : {},
    };
  }
  if (page.metadata != null) {
    merged = { ...merged, ...page.metadata };
  }
  if (typeof page.generateMetadata === "function") {
    merged = { ...merged, ...await page.generateMetadata(args) };
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

/** The current page's loader data. */
export hook useLoaderData<T>(): T {
  // $FlowFixMe[unclear-type] loader data is typed by the page that declares the loader.
  return (useRouterState().resolved.data: any);
}

/**
 * Renders the matched page inside its layouts, innermost last, with the
 * document metadata as hoistable head elements.
 */
export component RouteView() {
  const { resolved } = useRouterState();
  const Page = pageComponent(resolved.page);
  let element: React.Node = (
    <Page params={resolved.params} searchParams={resolved.searchParams} data={resolved.data} />
  );
  for (let index = resolved.layouts.length - 1; index >= 0; index -= 1) {
    const Layout = layoutComponent(resolved.layouts[index]);
    element = <Layout params={resolved.params}>{element}</Layout>;
  }
  return (
    <>
      <Head metadata={resolved.metadata} />
      {element}
    </>
  );
}

/**
 * The component a page module renders: its default export, or the named
 * `Page` that `uf create` scaffolds. An MDX page always has a default export.
 */
function pageComponent(module: PageModule): React.ComponentType<any> {
  const component = module.default ?? module.Page;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: a page module must export a component as `default` or `Page`",
    );
  }
  return component;
}

/** The component a layout module renders: `default`, or the named `Layout`. */
function layoutComponent(module: LayoutModule): React.ComponentType<any> {
  const component = module.default ?? module.Layout;
  if (component == null) {
    throw new Error(
      "@uniflowed/router: a layout module must export a component as `default` or `Layout`",
    );
  }
  return component;
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
        ? openGraph.images.map((image) => (
            <meta key={image} property="og:image" content={image} />
          ))
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
