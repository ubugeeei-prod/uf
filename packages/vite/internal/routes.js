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
 * Scan `appRoot` for routes.
 *
 * Returns routes sorted by path, which is the order `uf_router` uses too.
 * Directories that do not exist yield an empty table rather than an error: a
 * library project has no router root, and that is not a mistake.
 *
 * @param {string} appRoot absolute path of the router root (`app/`)
 * @returns {Route[]}
 */
export function scanRoutes(appRoot) {
  const routes = [];
  const handlers = [];
  const middleware = [];
  let notFound = null;
  if (!isDirectory(appRoot)) return { routes, handlers, middleware, notFound };

  const walk = (directory, segments, layouts, depth) => {
    if (depth > MAX_DEPTH) return;
    const entries = readdirSync(directory, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
    );

    const ownLayout = findModule(directory, RESERVED.layout, MODULE_EXTENSIONS);
    const nextLayouts = ownLayout ? [...layouts, ownLayout] : layouts;

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

    if (depth === 0) {
      const own = findModule(directory, RESERVED.notFound, PAGE_EXTENSIONS);
      if (own) notFound = { page: own, layouts: nextLayouts, mdx: own.endsWith(".mdx") };
    }

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      // A leading dot or underscore is private to the author: `_components/`
      // beside a page is a place to put things, not a route.
      if (entry.name.startsWith(".") || entry.name.startsWith("_")) continue;
      walk(path.join(directory, entry.name), [...segments, entry.name], nextLayouts, depth + 1);
    }
  };

  walk(appRoot, [], [], 0);
  const byPath = (a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  routes.sort(byPath);
  handlers.sort(byPath);
  middleware.sort(byPath);
  return { routes, handlers, middleware, notFound };
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

/** Virtual module ids the router plugin serves. */
export const VIRTUAL = Object.freeze({
  routes: "virtual:uf/routes",
  client: "virtual:uf/client",
  server: "virtual:uf/server",
});

/**
 * The source of `virtual:uf/routes`.
 *
 * Each page and layout is a lazy `import()`, so a route is a chunk of its own.
 * Layouts are deduplicated into one table so a layout shared by fifty routes
 * is one dynamic import, not fifty. Middleware needs no deduplication: it is
 * already one entry per file, keyed by the path it guards.
 *
 * @param {{routes: Route[], handlers: Handler[], middleware: Middleware[], notFound: object | null}} table
 */
export function routesModuleSource(table) {
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

  const entries = table.routes.map((route) => {
    const layouts = route.layouts.map(layoutId);
    return `  {
    path: ${JSON.stringify(route.path)},
    params: ${JSON.stringify(route.params)},
    mdx: ${route.mdx},
    file: ${JSON.stringify(route.page)},
    page: () => import(${JSON.stringify(route.page)}),
    layouts: [${layouts.join(", ")}],
  }`;
  });

  const notFound = table.notFound
    ? `{
  mdx: ${table.notFound.mdx},
  file: ${JSON.stringify(table.notFound.page)},
  page: () => import(${JSON.stringify(table.notFound.page)}),
  layouts: [${table.notFound.layouts.map(layoutId).join(", ")}],
}`
    : "null";

  // Handlers are a separate table because nothing on the client wants them:
  // a route handler answers a request, so shipping its module to the browser
  // would ship server code to the page.
  const handlerEntries = (table.handlers ?? []).map(
    (handler) => `  {
    path: ${JSON.stringify(handler.path)},
    params: ${JSON.stringify(handler.params)},
    file: ${JSON.stringify(handler.module)},
    load: () => import(${JSON.stringify(handler.module)}),
  }`,
  );

  // Middleware is a table of its own for the same reason, and for a stronger
  // one: it is where an application puts the check it does not want a user to
  // read. `clientModuleSource` imports `routes` and `notFound` and nothing
  // else, so a middleware module is reachable from the server entry alone.
  const middlewareEntries = (table.middleware ?? []).map(
    (entry) => `  {
    path: ${JSON.stringify(entry.path)},
    file: ${JSON.stringify(entry.module)},
    load: () => import(${JSON.stringify(entry.module)}),
  }`,
  );

  return `${layoutImports.join("\n")}
export const routes = [
${entries.join(",\n")}
];
export const handlers = [
${handlerEntries.join(",\n")}
];
export const middleware = [
${middlewareEntries.join(",\n")}
];
export const notFound = ${notFound};
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
import { routes, notFound } from ${JSON.stringify(VIRTUAL.routes)};
import App from ${JSON.stringify(appEntry)};
hydrate({ App, routes, notFound });
`;
}

/**
 * The source of `virtual:uf/server`: render one URL to HTML.
 */
export function serverModuleSource(appEntry) {
  return `import {
  createDispatcher,
  createMiddlewareRunner,
  createRenderer,
} from "@uniflowed/router/server";
import { routes, handlers, middleware, notFound } from ${JSON.stringify(VIRTUAL.routes)};
import App from ${JSON.stringify(appEntry)};
export { routes, handlers, middleware, notFound };
export const render = createRenderer({ App, routes, notFound });
export const dispatch = createDispatcher({ handlers });
export const runMiddleware = createMiddlewareRunner({ middleware });
`;
}
