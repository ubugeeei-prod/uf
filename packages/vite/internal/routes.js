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
 * @property {ReadonlyArray<string>} middleware absolute paths, root first
 * @property {boolean} mdx whether the page is MDX content
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
  let notFound = null;
  if (!isDirectory(appRoot)) return { routes, notFound };

  const walk = (directory, segments, layouts, middleware, depth) => {
    if (depth > MAX_DEPTH) return;
    const entries = readdirSync(directory, { withFileTypes: true }).sort((a, b) =>
      a.name < b.name ? -1 : a.name > b.name ? 1 : 0,
    );

    const ownLayout = findModule(directory, RESERVED.layout, MODULE_EXTENSIONS);
    const ownMiddleware = findModule(directory, RESERVED.middleware, MODULE_EXTENSIONS);
    const nextLayouts = ownLayout ? [...layouts, ownLayout] : layouts;
    const nextMiddleware = ownMiddleware ? [...middleware, ownMiddleware] : middleware;

    const page = findModule(directory, RESERVED.page, PAGE_EXTENSIONS);
    if (page) {
      const { path: routePath, pattern, params } = routeFromSegments(segments);
      routes.push({
        path: routePath,
        pattern,
        params,
        page,
        layouts: nextLayouts,
        middleware: nextMiddleware,
        mdx: page.endsWith(".mdx"),
      });
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
      walk(
        path.join(directory, entry.name),
        [...segments, entry.name],
        nextLayouts,
        nextMiddleware,
        depth + 1,
      );
    }
  };

  walk(appRoot, [], [], [], 0);
  routes.sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
  return { routes, notFound };
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
 * is one dynamic import, not fifty.
 *
 * @param {{routes: Route[], notFound: object | null}} table
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

  return `${layoutImports.join("\n")}
export const routes = [
${entries.join(",\n")}
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
  return `import { createRenderer } from "@uniflowed/router/server";
import { routes, notFound } from ${JSON.stringify(VIRTUAL.routes)};
import App from ${JSON.stringify(appEntry)};
export { routes, notFound };
export const render = createRenderer({ App, routes, notFound });
`;
}
