// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// The file-system router, as the build sees it.
//
// This mirrors `uf_router` in Rust — the same reserved-name grammar
// (`_uf.<role>[.<variant>].js`, plus `.mdx` for pages), the same route path
// syntax (`[param]`, `[...rest]`, `(group)`) and the same sort order — and it
// must keep mirroring it: `uf lint` and `router.js`'s generated types describe
// the routes this module serves, so the two cannot be allowed to disagree.
//
// Everything produced here is a string of JavaScript for a virtual module. The
// route table imports every page and layout lazily, so a route is a chunk of
// its own and the client only downloads what it navigates to.

import { readdirSync, statSync } from "node:fs";
import path from "node:path";

/** The file names the router reserves inside the router root. */
export const RESERVED = Object.freeze({
  layout: "_uf.layout",
  page: "_uf.page",
  middleware: "_uf.middleware",
  notFound: "_uf.not-found",
  error: "_uf.error",
  loading: "_uf.loading",
  route: "_uf.route",
});

/** Extensions a page or layout may use; `.mdx` is a page written as content. */
const PAGE_EXTENSIONS = [".js", ".jsx", ".mdx"];
const MODULE_EXTENSIONS = [".js", ".jsx"];

/** Deepest directory nesting the scan will follow. */
const MAX_DEPTH = 32;

/**
 * One route in the table.
 *
 * @typedef {object} Route
 * @property {string} path route path such as `/docs/:slug`
 * @property {string} pattern the same path with `*` for catch-alls, for humans
 * @property {ReadonlyArray<{name: string, catchAll: boolean}>} params
 * @property {string} page absolute path of the page module
 * @property {ReadonlyArray<string>} layouts absolute paths, root first
 * @property {ReadonlyArray<{above: number, module: string}>} loading the
 *   `<Suspense>` boundaries in scope, root first; `above` is how many of
 *   `layouts` are outside each one
 * @property {boolean} mdx whether the page is MDX content
 */

/**
 * One middleware — everything under a directory, guarded before it answers.
 *
 * A flat table keyed by the directory's route path, rather than an array on
 * every route the way layouts are accumulated. That was the first shape and it
 * left two holes: `/dashboard/typo` matches no route, so a per-route array
 * would have rendered the 404 with the guard skipped, and a route handler is
 * in a table of its own, so guarding pages would have guarded half of them.
 * The path is the matcher, so the path is what the table carries.
 *
 * @typedef {object} Middleware
 * @property {string} path route path of the directory it guards, `/` at the root
 * @property {string} module absolute path of the middleware module
 */

/**
 * One route handler — a path that answers a request instead of rendering.
 *
 * @typedef {object} Handler
 * @property {string} path route path such as `/api/users/:id`
 * @property {string} pattern the same path with `*` for catch-alls
 * @property {ReadonlyArray<{name: string, catchAll: boolean}>} params
 * @property {string} module absolute path of the handler module
 */

/**
 * One not-found boundary — the page a path under `path` gets when nothing
 * there matched.
 *
 * A `_uf.not-found.js` is a segment file like `_uf.layout.js`, so a directory
 * declares the 404 for everything beneath it and the resolver takes the
 * nearest one above the path. `layouts` are the layouts in scope *at that
 * directory*, which is what wraps the boundary when it renders.
 *
 * @typedef {object} NotFoundBoundary
 * @property {string} path route path of the directory that declares it
 * @property {?string} page absolute path of the page module, or `null` for the
 *   record the scan synthesises at the router root when a project declares
 *   none — see `scanRoutes`
 * @property {ReadonlyArray<string>} layouts absolute paths, root first
 * @property {boolean} mdx whether the page is MDX content
 */

/**
 * One error boundary — what renders in place of the subtree under `path` when
 * something in it throws.
 *
 * The same nearest-ancestor shape as a not-found boundary, and deliberately
 * not the same extensions: an error module is handed an error and a `reset`,
 * which is a component's contract. `.mdx` compiles to a component that takes
 * no such thing, so a `_uf.error.mdx` would be a file the router loads and can
 * never hand its arguments to.
 *
 * @typedef {object} ErrorBoundary
 * @property {string} path route path of the directory that declares it
 * @property {?string} module absolute path of the error module, or `null` for
 *   the synthesised root record
 * @property {ReadonlyArray<string>} layouts absolute paths, root first
 */

/**
 * One loading boundary — the fallback for the segment that declares it.
 *
 * Not the nearest-ancestor shape the other two boundaries have, and the
 * difference is the whole of what a fallback is. A not-found or an error
 * boundary is *chosen*: one of them renders, and the resolver picks the
 * nearest above the path. Loading boundaries *nest*: `app/_uf.loading.js` and
 * `app/docs/_uf.loading.js` are two `<Suspense>` elements on one route, one
 * inside the other, and both are in the tree at once. So they accumulate down
 * the walk the way layouts do rather than being matched afterwards, and each
 * route carries the list that applies to it.
 *
 * `above` is the count of the route's `layouts` that sit outside the boundary
 * — the layouts that render immediately, which is what "the shell around a
 * slow page" means. It is the same number, spelled the same way, as
 * `ResolvedRoute["errorBoundary"].above` in the router runtime.
 *
 * @typedef {object} LoadingBoundary
 * @property {number} above how many of the route's layouts are outside it
 * @property {string} module absolute path of the loading module
 */

/**
 * Scan `appRoot` for routes.
 *
 * Returns routes sorted by path, which is the order `uf_router` uses too.
 * Directories that do not exist yield an empty table rather than an error: a
 * library project has no router root, and that is not a mistake.
 *
 * @param {string} appRoot absolute path of the router root (`app/`)
 * @returns {{
 *   routes: Route[],
 *   handlers: Handler[],
 *   middleware: Middleware[],
 *   notFound: NotFoundBoundary[],
 *   errors: ErrorBoundary[],
 * }}
 */
export function scanRoutes(appRoot) {
  const routes = [];
  const handlers = [];
  const middleware = [];
  const notFound = [];
  const errors = [];
  if (!isDirectory(appRoot)) return { routes, handlers, middleware, notFound, errors };

  // The layouts in scope at the router root, kept because the two synthesised
  // records below are made of them. See the note beside them.
  let rootLayouts = [];

  const walk = (directory, segments, layouts, loading, depth) => {
    if (depth > MAX_DEPTH) return;
    const entries = readdirSync(directory, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
    );

    const ownLayout = findModule(directory, RESERVED.layout, MODULE_EXTENSIONS);
    const nextLayouts = ownLayout ? [...layouts, ownLayout] : layouts;
    if (depth === 0) {
      rootLayouts = nextLayouts;
    }

    // Inside this directory's own layout, which is where Next.js puts it and
    // the only placement that makes sense: the fallback is what shows *within*
    // the frame this segment draws, so the frame has to be outside it.
    // `nextLayouts.length` is therefore the count taken after the own layout is
    // added, not before. A segment with a loading file and no layout of its own
    // still gets a boundary — it just shares its parent's frame.
    const ownLoading = findModule(directory, RESERVED.loading, MODULE_EXTENSIONS);
    const nextLoading = ownLoading
      ? [...loading, { above: nextLayouts.length, module: ownLoading }]
      : loading;

    // A middleware guards this directory and everything below it, whether or
    // not this directory is itself a route: `app/dashboard/_uf.middleware.js`
    // with no `_uf.page.js` beside it still guards `/dashboard/settings`.
    const ownMiddleware = findModule(directory, RESERVED.middleware, MODULE_EXTENSIONS);
    if (ownMiddleware) {
      middleware.push({ path: routeFromSegments(segments).path, module: ownMiddleware });
    }

    const page = findModule(directory, RESERVED.page, PAGE_EXTENSIONS);
    if (page) {
      const { path: routePath, pattern, params } = routeFromSegments(segments);
      routes.push({
        path: routePath,
        pattern,
        params,
        page,
        layouts: nextLayouts,
        loading: nextLoading,
        mdx: page.endsWith(".mdx"),
      });
    }
    // A handler answers the request itself, so it takes no layouts and is not
    // MDX. It may sit beside a page: `/feed` can render for a browser and
    // `/feed.xml` answer for a reader, and both are the same directory tree.
    const handler = findModule(directory, RESERVED.route, MODULE_EXTENSIONS);
    if (handler) {
      const { path: routePath, pattern, params } = routeFromSegments(segments);
      handlers.push({ path: routePath, pattern, params, module: handler });
    }

    // At every depth, not only the root. This read `if (depth === 0)`, so
    // `app/guide/_uf.not-found.js` was never looked for and a reader who
    // followed a stale link into the manual was answered by the site's root
    // 404, outside the manual's own layout. See ubugeeei-prod/uf#263.
    const ownNotFound = findModule(directory, RESERVED.notFound, PAGE_EXTENSIONS);
    if (ownNotFound) {
      notFound.push({
        path: routeFromSegments(segments).path,
        page: ownNotFound,
        layouts: nextLayouts,
        mdx: ownNotFound.endsWith(".mdx"),
      });
    }

    // `errors` is the boundaries a project declares, not failures that
    // happened: one entry per directory holding an `_uf.error.js`.
    const ownError = findModule(directory, RESERVED.error, MODULE_EXTENSIONS);
    if (ownError) {
      errors.push({
        path: routeFromSegments(segments).path,
        module: ownError,
        layouts: nextLayouts,
      });
    }

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      // A leading dot or underscore is private to the author: `_components/`
      // beside a page is a place to put things, not a route.
      if (entry.name.startsWith(".") || entry.name.startsWith("_")) continue;
      walk(
        path.join(directory, entry.name),
        [...segments, entry.name],
        nextLayouts,
        nextLoading,
        depth + 1,
      );
    }
  };

  walk(appRoot, [], [], [], 0);

  // A boundary at the router root for a project that declared none, carrying
  // the root's layouts and no module of its own.
  //
  // Without it the router had no record to answer an unmatched URL with, so it
  // answered with the framework's page and `layouts: []` — and a site whose
  // root layout owns the masthead, the stylesheet and often `<html>` itself
  // replied to a stale link with a white page saying 404, with no way to leave
  // it. That was never the nearest-ancestor rule failing: the rule had nothing
  // to find. `uf create` scaffolds neither boundary, so this is the state every
  // new project is in until it writes one. See ubugeeei-prod/uf#351.
  //
  // Only when nothing is at `/` already. A `(group)` directory is not a URL
  // segment, so `app/(marketing)/_uf.not-found.js` is a boundary at `/` too and
  // adding a second one there would put a second answer at a path the URL
  // cannot choose between.
  const atRoot = (boundaries) => boundaries.some((boundary) => boundary.path === "/");
  if (!atRoot(notFound)) {
    notFound.push({ path: "/", page: null, layouts: rootLayouts, mdx: false });
  }
  if (!atRoot(errors)) {
    errors.push({ path: "/", module: null, layouts: rootLayouts });
  }

  const byPath = (a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  routes.sort(byPath);
  handlers.sort(byPath);
  // Sorted for a table that does not churn between builds, and for nothing
  // else: `createMiddlewareRunner` re-orders the table root first, because
  // what a chain of guards runs in is depth, not name.
  middleware.sort(byPath);
  // Sorted by path, not by which is nearest: the resolver picks the longest
  // path that covers the URL, so it does not depend on this order, and sorting
  // by nearness would hide that.
  //
  // Two boundaries can share a path, because a `(group)` directory is not a URL
  // segment — `app/_uf.not-found.js` and `app/(marketing)/_uf.not-found.js` are
  // both at `/`, and the URL cannot say which tree it is in. The sort is stable
  // and `walk` records a directory's own boundary before descending, so the
  // shallower file wins, which is the one that is the site's own 404 rather
  // than one section's idea of it. Letting each group own a boundary needs the
  // parallel-route trees uf does not have yet; see ubugeeei-prod/uf#267.
  notFound.sort(byPath);
  errors.sort(byPath);
  return { routes, handlers, middleware, notFound, errors };
}

function isDirectory(candidate) {
  try {
    return statSync(candidate).isDirectory();
  } catch {
    return false;
  }
}

function findModule(directory, stem, extensions) {
  for (const extension of extensions) {
    const candidate = path.join(directory, stem + extension);
    try {
      if (statSync(candidate).isFile()) return candidate;
    } catch {
      // keep looking
    }
  }
  return null;
}

/**
 * Turn directory segments into a route path and its parameters.
 *
 * `(group)` segments organise files without appearing in the URL, `[name]`
 * captures one segment, and `[...name]` captures the rest of the path.
 */
export function routeFromSegments(segments) {
  const params = [];
  const out = [];
  for (const segment of segments) {
    if (segment.startsWith("(") && segment.endsWith(")")) continue;
    if (segment.startsWith("[...") && segment.endsWith("]")) {
      const name = segment.slice(4, -1);
      params.push({ name, catchAll: true });
      out.push(`:${name}*`);
      continue;
    }
    if (segment.startsWith("[") && segment.endsWith("]")) {
      const name = segment.slice(1, -1);
      params.push({ name, catchAll: false });
      out.push(`:${name}`);
      continue;
    }
    out.push(segment);
  }
  const routePath = out.length === 0 ? "/" : `/${out.join("/")}`;
  return { path: routePath, pattern: routePath.replace(/:(\w+)\*/g, "*$1"), params };
}

/**
 * Virtual module ids the router plugin serves.
 *
 * `actions` is the one that does not come from this file's directory scan: it
 * is generated from the RSC manifest by `internal/rsc.js`, because which
 * `"use server"` exports are callable endpoints is an answer about the module
 * graph and not about the filesystem. It is here because it is a virtual
 * module id and this is where they are named, and because
 * `serverModuleSource` below is the only thing that imports it.
 */
export const VIRTUAL = Object.freeze({
  routes: "virtual:uf/routes",
  client: "virtual:uf/client",
  server: "virtual:uf/server",
  actions: "virtual:uf/actions",
});

/**
 * The source of `virtual:uf/routes`.
 *
 * Each page and layout is a lazy `import()`, so a route is a chunk of its own.
 * Layouts are deduplicated into one table so a layout shared by fifty routes
 * is one dynamic import, not fifty. Middleware needs no deduplication: it is
 * already one entry per file, keyed by the path it guards.
 *
 * # The client's copy is not the server's
 *
 * `shipsPage` is how the server/client split reaches the bundle. A route it
 * answers `false` for keeps its path and its parameters — the router still has
 * to *match* the URL, so that a link into it can hand the navigation back to
 * the browser — and loses its `page`, its `layouts` and its `loading`
 * boundaries, which are the only `import()` calls in this table. Nothing in
 * the browser can then reach the module through the router, so Rollup emits no
 * chunk for it and none for anything only it reached.
 *
 * Omitted by leaving the key out rather than by writing `page: null`, because
 * the two say different things to a bundler: a property whose value is an
 * `import()` is a chunk whether or not anything reads it.
 *
 * # Except for its styles
 *
 * A route that ships no JavaScript still has to *look* right, and a uf build
 * takes its stylesheets from the client graph: `assetsFromManifest` walks the
 * client entry's imports and links the CSS it finds, so a module removed from
 * that graph takes its rules out of every page in the site. That is a silent
 * visual break, and it is worse than shipping the module.
 *
 * So each module a dropped route was the only reader of comes back at the top
 * of this file as a bare `import <file>;` — a side-effect import, with no
 * binding read from it. Its stylesheet is a side effect and survives; its
 * components, its helpers and everything only they referenced are unused
 * exports and do not. A layout a *kept* route still uses is left out of that
 * list: it is already here as a lazy import, and a static one as well would
 * pull it into the entry chunk.
 *
 * The default answers `true` for every route, which is the whole table, no
 * side-effect imports, and exactly what this emitted before the split existed.
 * `virtual:uf/server` is generated with the default and always will be: the
 * server renders every route, so its table is the complete one.
 *
 * # `relativeTo`, and the one string in this table a browser can read
 *
 * Every `import()` here is a specifier Vite resolves and rewrites to a chunk
 * URL, so no absolute path survives the build — except `file`, which is a
 * string. It is the route's source path, kept for diagnostics: the middleware
 * table's is what names a module in an error, and `router.js`'s generated
 * types are about the same files.
 *
 * The server's table can hold an absolute path; it is read on the machine that
 * has those files. The browser's cannot, because that table is downloaded:
 * uf's own manual shipped `/home/<user>/…/docs/app/guide/cache/_uf.page.mdx`
 * for each of thirty-four routes to every visitor, which publishes the build
 * machine's layout and its user's name for nothing — the browser has no
 * filesystem to resolve them against and reads them only in a message.
 *
 * So the client call passes the project root and every `file` here is emitted
 * relative to it. Diagnostics keep a path a person can act on — a shorter one
 * — and a deploy stops describing the machine it was built on.
 *
 * @param {{
 *   routes: Route[],
 *   handlers?: Handler[],
 *   middleware?: Middleware[],
 *   notFound?: NotFoundBoundary[],
 *   errors?: ErrorBoundary[],
 * }} table
 * @param {{
 *   shipsPage?: (route: Route) => boolean,
 *   relativeTo?: string,
 * }} [options]
 */
export function routesModuleSource(table, options = {}) {
  const shipsPage = options.shipsPage ?? (() => true);
  const relativeTo = options.relativeTo ?? null;
  /**
   * A `file` as this table should state it.
   *
   * Relative even when that means leading `..` segments — a module outside the
   * project root is rare and a `../` path still says where it is without
   * saying where the machine is, which is the whole property. Separators are
   * POSIX because this string is read wherever the bundle is opened rather
   * than where it was written.
   */
  const displayFile = (file) => {
    if (relativeTo == null || file == null) {
      return file;
    }
    return path.relative(relativeTo, file).split(path.sep).join("/");
  };
  const layoutIds = new Map();
  const layoutImports = [];
  const layoutId = (file) => {
    let id = layoutIds.get(file);
    if (id === undefined) {
      id = `layout${layoutIds.size}`;
      layoutIds.set(file, id);
      layoutImports.push(`const ${id} = () => import(${JSON.stringify(file)});`);
    }
    return id;
  };

  // Loading modules are deduplicated into a table of their own, for the reason
  // layouts are: one `app/_uf.loading.js` is the fallback of every route under
  // it, and fifty copies of the same `import()` would be fifty chunks of the
  // same file.
  //
  // They are static imports rather than lazy ones, and that is not an
  // oversight. React decides to show a fallback *synchronously*, during the
  // render that suspended, so a fallback still waiting on its own `import()` is
  // a fallback that is not there at the only moment it is wanted — the same
  // reasoning as the error boundaries below, arrived at from the other
  // direction. `resolveMatch` awaits them with the layouts, before it renders.
  const loadingIds = new Map();
  const loadingImports = [];
  const loadingId = (file) => {
    let id = loadingIds.get(file);
    if (id === undefined) {
      id = `loading${loadingIds.size}`;
      loadingIds.set(file, id);
      loadingImports.push(`const ${id} = () => import(${JSON.stringify(file)});`);
    }
    return id;
  };

  const entries = table.routes.map((route) => {
    if (!shipsPage(route)) {
      return `  {
    path: ${JSON.stringify(route.path)},
    params: ${JSON.stringify(route.params)},
    mdx: ${route.mdx},
    file: ${JSON.stringify(displayFile(route.page))},
    layouts: [],
    loading: [],
  }`;
    }
    const layouts = route.layouts.map(layoutId);
    const loading = (route.loading ?? []).map(
      (boundary) => `{ above: ${boundary.above}, module: ${loadingId(boundary.module)} }`,
    );
    return `  {
    path: ${JSON.stringify(route.path)},
    params: ${JSON.stringify(route.params)},
    mdx: ${route.mdx},
    file: ${JSON.stringify(displayFile(route.page))},
    page: () => import(${JSON.stringify(route.page)}),
    layouts: [${layouts.join(", ")}],
    loading: [${loading.join(", ")}],
  }`;
  });

  // A boundary the scan synthesised has no module to import — the framework's
  // own page renders in its place — so it emits `null` where a declared one
  // emits a loader, and a name for `file` rather than a path nothing wrote.
  // See the note in `scanRoutes` and ubugeeei-prod/uf#351.
  const SYNTHESISED = JSON.stringify("@uniflowed/router");
  const boundaryModule = (file) =>
    file == null ? "null" : `() => import(${JSON.stringify(file)})`;
  const boundaryFile = (file) => (file == null ? SYNTHESISED : JSON.stringify(displayFile(file)));

  // A list, because a not-found is a segment file: every directory may declare
  // one and the router takes the nearest above the path. `layoutId` is the
  // same table the routes use, so a boundary that shares a layout with a page
  // shares its dynamic import too.
  const notFoundEntries = (table.notFound ?? []).map(
    (boundary) => `  {
    path: ${JSON.stringify(boundary.path)},
    mdx: ${boundary.mdx},
    file: ${boundaryFile(boundary.page)},
    page: ${boundaryModule(boundary.page)},
    layouts: [${boundary.layouts.map(layoutId).join(", ")}],
  }`,
  );

  // An error boundary is loaded with the route it guards rather than when it
  // is needed: React decides to render a boundary's fallback synchronously,
  // during the render that threw, so a module that still has to be imported is
  // a module that is not there when the only chance to use it arrives.
  const errorEntries = (table.errors ?? []).map(
    (boundary) => `  {
    path: ${JSON.stringify(boundary.path)},
    file: ${boundaryFile(boundary.module)},
    module: ${boundaryModule(boundary.module)},
    layouts: [${boundary.layouts.map(layoutId).join(", ")}],
  }`,
  );

  // Handlers are a separate table because nothing on the client wants them:
  // a route handler answers a request, so shipping its module to the browser
  // would ship server code to the page.
  const handlerEntries = (table.handlers ?? []).map(
    (handler) => `  {
    path: ${JSON.stringify(handler.path)},
    params: ${JSON.stringify(handler.params)},
    file: ${JSON.stringify(displayFile(handler.module))},
    load: () => import(${JSON.stringify(handler.module)}),
  }`,
  );

  // Middleware is a table of its own for the same reason, and for a stronger
  // one: it is where an application puts the check it does not want a user to
  // read. `clientModuleSource` imports `routes`, `notFound` and `errors` and
  // nothing else, so a middleware module is reachable from the server entry
  // alone.
  const middlewareEntries = (table.middleware ?? []).map(
    (entry) => `  {
    path: ${JSON.stringify(entry.path)},
    file: ${JSON.stringify(displayFile(entry.module))},
    load: () => import(${JSON.stringify(entry.module)}),
  }`,
  );

  // Last, because it is defined by what everything above did *not* import: a
  // layout a kept route also uses is already in the graph as a lazy chunk, and
  // importing it here as well would pull it into the entry chunk instead.
  const carried = new Set([...layoutIds.keys(), ...loadingIds.keys()]);
  const styleOnlyImports = [];
  for (const route of table.routes) {
    if (shipsPage(route)) {
      continue;
    }
    const files = [route.page, ...route.layouts, ...(route.loading ?? []).map((it) => it.module)];
    for (const file of files) {
      if (carried.has(file)) {
        continue;
      }
      carried.add(file);
      styleOnlyImports.push(`import ${JSON.stringify(file)};`);
    }
  }

  return `${[...styleOnlyImports, ...layoutImports, ...loadingImports].join("\n")}
export const routes = [
${entries.join(",\n")}
];
export const handlers = [
${handlerEntries.join(",\n")}
];
export const middleware = [
${middlewareEntries.join(",\n")}
];
export const notFound = [
${notFoundEntries.join(",\n")}
];
export const errors = [
${errorEntries.join(",\n")}
];
export default routes;
`;
}

/**
 * The source of `virtual:uf/client`: hydrate the document with the app.
 *
 * The current route's modules are loaded *before* hydration so the first
 * render is synchronous and matches the server's HTML; a lazy import during
 * hydration would suspend and React would fall back to a client render.
 */
export function clientModuleSource(appEntry) {
  return `import { hydrate } from "@uniflowed/router/client";
import { routes, notFound, errors } from ${JSON.stringify(VIRTUAL.routes)};
import App from ${JSON.stringify(appEntry)};
hydrate({ App, routes, notFound, errors });
`;
}

/**
 * The source of `virtual:uf/server`: answer one request.
 *
 * Three exports, and the order a host calls them in is the whole of how the
 * two halves of the table compose. `runMiddleware` first, because a middleware
 * guards a *path* — it has to run for a page, for a route handler, and for a
 * path under it that matches neither, so it belongs above route resolution
 * rather than inside it. `notFound` and `errors` go the other way: they are
 * boundaries chosen *during* a render, once resolution knows which route was
 * asked for and whether it threw, which is why they are `createRenderer`'s
 * arguments and not a step of their own. The two never compete for the same
 * request — one decides whether the router is reached at all, the others
 * decide what the router renders when it is.
 *
 * `callAction` goes between the two, and its position is the same argument
 * made twice. Below `runMiddleware`, because an action call is a request to a
 * path and the guard on that path is owed the same say over it as over the
 * page — which is why the call is a `POST` to the page's own URL rather than
 * to a reserved one. Above `dispatch`, because a request that names an action
 * has named it: letting it fall through to a route handler that happens to sit
 * at the same path would answer somebody's action with somebody else's
 * function. It declines every request that carries no action id, so a project
 * with no actions pays one `headers.get` per request and nothing else.
 *
 * `internal/serve.js` and `driver.js` call them in that order, and
 * `packages/vite/index.js` does the same for a project driving Vite itself.
 *
 * `render` and `prerender` are two exports rather than one with a flag, because
 * a host is one or the other: a server streams, a build writes files. See the
 * header of `packages/router/server.js` for why React needs both told apart.
 *
 * `beginRequest` is the fourth, and it is re-exported rather than imported by
 * the host for a reason that is easy to get wrong: `@uniflowed/server` keeps
 * the request in an `AsyncLocalStorage` held by *its module*, and a bundled
 * application has its own copy of that module inlined. A host that imported
 * `beginRequest` from its own `node_modules` would establish a request in a
 * second storage, and every `cookies()` in the application would still be
 * outside one. So the bundle hands the host the entry point that belongs to
 * the bundle. `uf preview`, `uf start`, `uf dev` and the compiled binary all
 * take it from here; see ubugeeei-prod/uf#389.
 *
 * Through `@uniflowed/router/server` rather than `@uniflowed/server/host`,
 * because this source is resolved from the *project's* directory and a project
 * depends on the router, not on the router's own dependency. It is also the
 * shorter proof of the paragraph above: the copy the router dispatches and
 * renders with is by construction the copy the host is handed.
 */
export function serverModuleSource(appEntry) {
  return `import {
  createActionDispatcher,
  createDispatcher,
  createMiddlewareRunner,
  createRenderer,
} from "@uniflowed/router/server";
import { routes, handlers, middleware, notFound, errors } from ${JSON.stringify(VIRTUAL.routes)};
import { actions } from ${JSON.stringify(VIRTUAL.actions)};
import App from ${JSON.stringify(appEntry)};
export { routes, handlers, middleware, notFound, errors };
export { beginRequest } from "@uniflowed/router/server";
const renderer = createRenderer({ App, routes, notFound, errors });
export const render = renderer.render;
export const prerender = renderer.prerender;
export const dispatch = createDispatcher({ handlers });
export const callAction = createActionDispatcher({ actions });
export const runMiddleware = createMiddlewareRunner({ middleware });
`;
}
