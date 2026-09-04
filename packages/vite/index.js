// @noflow
//
// Plain JavaScript: Vite imports this module directly, before any transform.
//
// `@uniflowed/vite` — uf, as Vite plugins.
//
// Vite is the dev server, the module graph, hot module replacement, the
// bundler and the plugin system; uf contributes what is specific to a Flow
// React application and nothing that Vite already does:
//
// * `uf:flow`   — every Flow module goes through `uf transform` (the official
//                 Flow parser, Flow's own lowering rules, the official React
//                 Compiler, oxc), plus the React Fast Refresh wiring in
//                 development and the virtual modules that make a directory
//                 of pages an application: the route table, the client entry
//                 that hydrates it, and the server entry that renders it. In
//                 development it also renders every HTML request on the
//                 server, so `uf dev` serves the same markup `uf build` writes.
// * `uf:mdx`    — `@mdx-js/rollup`, configured for React with GitHub-flavoured
//                 markdown, front matter, heading ids and build-time syntax
//                 highlighting, so `.mdx` works with
//                 no configuration.
//
// `uniflowed(options)` returns the array; a project that wants to add a plugin
// declares it in `uf.config.js` and the driver appends it after these.

import { readdirSync } from "node:fs";
import path from "node:path";

import mdx from "@mdx-js/rollup";
import rehypeSlug from "rehype-slug";

import { highlightPlugin } from "./internal/highlight.js";
import remarkFrontmatter from "remark-frontmatter";
import remarkGfm from "remark-gfm";
import remarkMdxFrontmatter from "remark-mdx-frontmatter";

import {
  RUNTIME_PUBLIC_PATH,
  RUNTIME_RESOLVED_ID,
  addRefreshWrapper,
  preambleCode,
  refreshRuntimeSource,
} from "./internal/refresh.js";
import {
  VIRTUAL,
  clientModuleSource,
  routesModuleSource,
  scanRoutes,
  serverModuleSource,
} from "./internal/routes.js";
import { TransformService, isFlowModule } from "./transform.js";

/** A resolved virtual id: Vite's convention is a leading NUL byte. */
const resolved = (id) => `\0${id}`;
const VIRTUAL_IDS = new Set(Object.values(VIRTUAL));

/**
 * Prefix of the virtual module that carries one source module's StyleX rules.
 *
 * Not NUL-prefixed, unlike the virtual modules above: Vite's CSS pipeline keys
 * off the `.css` extension of a *resolvable* id, and a NUL-prefixed id is
 * excluded from it. The prefix is distinctive enough that nothing else can
 * collide with it.
 */
const STYLE_PREFIX = "uf-style:";

/** The URL a NUL-prefixed module is served at in development. */
export function devUrlFor(id) {
  return `/@id/__x00__${id}`;
}

/**
 * Options for the plugin set.
 *
 * @typedef {object} UniflowedOptions
 * @property {string} [root] absolute project root; Vite's root by default
 * @property {object} [config] the loaded `uf.config.js` object
 * @property {string} [command] the `uf` binary to transform through
 */

/**
 * uf's Vite plugins.
 *
 * @param {UniflowedOptions} [options]
 */
export default function uniflowed(options = {}) {
  const ufConfig = options.config ?? {};
  const app = ufConfig.app ?? {};
  const routerRoot = app.router?.root ?? "app";
  const appEntry = app.router?.entry ?? ufConfig.build?.entries?.[0] ?? "app.js";
  const markdown = app.builtins?.markdown ?? {};

  return [flowPlugin({ routerRoot, appEntry, command: options.command }), mdxPlugin(markdown)];
}

function flowPlugin({ routerRoot, appEntry, command }) {
  let root = process.cwd();
  let isProduction = false;
  let base = "/";
  let appRoot = "";
  let entryPath = "";
  /** @type {import("vite").ViteDevServer | null} */
  let server = null;
  /** @type {TransformService | null} */
  let service = null;
  /**
   * Each module's compiled stylesheet, keyed by the virtual id serving it.
   *
   * A map rather than one accumulated sheet: Vite asks for a module's CSS when
   * it loads that module, re-asks when the module changes, and drops it when
   * the module goes away. One shared sheet would have to be invalidated by
   * hand, which is the part that goes wrong.
   */
  const styles = new Map();

  const ensureService = () => {
    service ??= new TransformService({ command, root });
    return service;
  };

  return {
    name: "uf:flow",
    enforce: "pre",

    config(userConfig, env) {
      const projectRoot = path.resolve(userConfig.root ?? process.cwd());
      isProduction = env.mode === "production" || env.command === "build";
      return {
        // uf serves HTML itself; there is no index.html to fall back to.
        appType: "custom",
        resolve: {
          dedupe: ["react", "react-dom"],
        },
        optimizeDeps: {
          include: [
            "react",
            "react/jsx-runtime",
            "react/jsx-dev-runtime",
            "react/compiler-runtime",
            "react-dom",
            "react-dom/client",
          ],
          // uf's packages ship Flow. The dependency optimiser pre-bundles
          // with a JavaScript parser and would reject every one of them.
          exclude: uniflowedPackages(projectRoot),
        },
        ssr: {
          // Same reason on the server: Node cannot import Flow, so these go
          // through the plugin like project code rather than being
          // externalised.
          noExternal: [/^@uniflowed\//],
        },
      };
    },

    configResolved(config) {
      root = config.root;
      base = config.base;
      appRoot = path.resolve(root, routerRoot);
      entryPath = path.resolve(root, appEntry);
    },

    buildStart() {
      ensureService();
    },

    resolveId(id) {
      if (id === RUNTIME_PUBLIC_PATH) return RUNTIME_RESOLVED_ID;
      if (VIRTUAL_IDS.has(id)) return resolved(id);
      // A module's own stylesheet, which `transform` below asked for by
      // importing this id. Returning it unchanged marks it resolved without
      // Vite going to the filesystem for a file that does not exist.
      if (id.startsWith(STYLE_PREFIX)) return id;
      return null;
    },

    load(id) {
      if (id === RUNTIME_RESOLVED_ID) return refreshRuntimeSource();
      if (id === resolved(VIRTUAL.routes)) return routesModuleSource(scanRoutes(appRoot));
      if (id === resolved(VIRTUAL.client)) return clientModuleSource(entryPath);
      if (id === resolved(VIRTUAL.server)) return serverModuleSource(entryPath);
      if (id.startsWith(STYLE_PREFIX)) return styles.get(id) ?? "";
      return null;
    },

    async transform(code, id, transformOptions) {
      if (!isFlowModule(id)) return null;
      const ssr = transformOptions?.ssr === true || this.environment?.name === "ssr";
      const refresh = !isProduction && !ssr && server != null;
      const out = await ensureService().transform(cleanId(id), code, {
        development: !isProduction,
        refresh,
        sourceMap: true,
      });
      if (out == null) return null;
      for (const diagnostic of out.diagnostics) {
        this.warn?.(`${diagnostic.function ?? "a function"}: ${diagnostic.message}`);
      }
      const map = out.map == null ? null : JSON.parse(out.map);
      // StyleX. `uf transform` compiled the module's `stylex.create` calls into
      // class names and handed back the rules they declared; the rules become a
      // module of their own that this one imports.
      //
      // Handing the CSS to Vite as a module, rather than collecting it here and
      // writing a stylesheet at the end, is what keeps uf out of the CSS
      // business: Vite already injects a stylesheet in dev, extracts it in a
      // build, code-splits it per chunk, and replaces it over HMR. A module
      // whose styles are gone stops importing it, and Vite notices.
      let output = out.code;
      if (out.css != null && out.css !== "") {
        const styleId = `${STYLE_PREFIX}${cleanId(id)}.css`;
        styles.set(styleId, out.css);
        output = `import ${JSON.stringify(styleId)};\n${output}`;
      }
      if (!refresh) return { code: output, map };
      const relative = path.relative(root, cleanId(id)).split(path.sep).join("/");
      return addRefreshWrapper(output, map, relative);
    },

    buildEnd() {
      // A dev server keeps its service for the whole session; a build is
      // done with it here.
      if (server == null) {
        service?.close();
        service = null;
      }
    },

    transformIndexHtml() {
      if (isProduction) return [];
      return [
        {
          tag: "script",
          attrs: { type: "module" },
          children: preambleCode(base),
          injectTo: "head-prepend",
        },
      ];
    },

    configureServer(devServer) {
      server = devServer;
      devServer.httpServer?.once("close", () => {
        service?.close();
        service = null;
      });

      // A page or layout appearing or disappearing changes the route table,
      // which lives in a virtual module the watcher knows nothing about.
      const reserved = /\/_uf\.(page|layout|middleware|not-found)(\.[a-z]+)?\.(js|jsx|mdx)$/;
      const onRouteFile = (file) => {
        if (!reserved.test(file) || !file.startsWith(appRoot)) return;
        const routes = devServer.moduleGraph.getModuleById(resolved(VIRTUAL.routes));
        if (routes) devServer.moduleGraph.invalidateModule(routes);
        devServer.ws.send({ type: "full-reload", path: "*" });
      };
      devServer.watcher.on("add", onRouteFile);
      devServer.watcher.on("unlink", onRouteFile);

      // After Vite's own middlewares, so `/@vite/client`, `/@id/...` and
      // static files are served first and only a document request reaches
      // the renderer.
      return () => {
        devServer.middlewares.use(async (request, response, next) => {
          if (!wantsDocument(request)) return next();
          try {
            const url = request.url ?? "/";
            const { render } = await importServerEntry(devServer);
            const result = await render(url, {
              scripts: [devUrlFor(VIRTUAL.client)],
              styles: [],
              preloads: [],
            });
            const html = await devServer.transformIndexHtml(url, result.html);
            response.statusCode = result.status;
            response.setHeader("Content-Type", "text/html; charset=utf-8");
            for (const [name, value] of Object.entries(result.headers ?? {})) {
              response.setHeader(name, value);
            }
            response.end(html);
          } catch (error) {
            devServer.ssrFixStacktrace(error);
            next(error);
          }
        });
      };
    },
  };
}

function mdxPlugin(markdown) {
  const mdxConfig = markdown.mdx ?? {};
  if (mdxConfig.enabled === false) return { name: "uf:mdx" };

  // Highlighting is on unless a project turns it off, and it happens here
  // rather than in the browser: the colours are in the HTML, so a code sample
  // is readable before any JavaScript loads and no highlighter is shipped.
  const highlight = highlightPlugin(mdxConfig.highlight);
  const rehypePlugins = highlight == null ? [rehypeSlug] : [rehypeSlug, highlight];

  return {
    enforce: "pre",
    ...mdx({
      jsxImportSource: "react",
      remarkPlugins: [
        remarkGfm,
        remarkFrontmatter,
        [remarkMdxFrontmatter, { name: "frontmatter" }],
      ],
      rehypePlugins,
    }),
    name: "uf:mdx",
  };
}

/**
 * Import `virtual:uf/server` through the dev server's module runner.
 *
 * Vite 6 introduced the environment API and its module runner; `ssrLoadModule`
 * is the older path and is kept as the fallback.
 */
async function importServerEntry(devServer) {
  const ssr = devServer.environments?.ssr;
  if (ssr != null) {
    if (ssr.runner == null) {
      const { createServerModuleRunner } = await import("vite");
      ssr.runner = createServerModuleRunner(ssr, { hmr: { logger: false } });
    }
    return ssr.runner.import(VIRTUAL.server);
  }
  return devServer.ssrLoadModule(VIRTUAL.server);
}

function wantsDocument(request) {
  if (request.method !== "GET" && request.method !== "HEAD") return false;
  const url = request.url ?? "/";
  if (url.startsWith("/@") || url.startsWith("/node_modules/")) return false;
  const accept = request.headers.accept ?? "";
  if (!accept.includes("text/html")) return false;
  const pathname = url.split("?")[0];
  // A request for a file — `/favicon.svg`, `/assets/x.js` — that no static
  // middleware answered is a 404, not a page.
  return !/\.[a-z0-9]+$/i.test(pathname);
}

function cleanId(id) {
  const at = id.indexOf("?");
  return at === -1 ? id : id.slice(0, at);
}

/**
 * Every `@uniflowed/*` package the project can resolve, for
 * `optimizeDeps.exclude`, which takes names rather than patterns.
 */
function uniflowedPackages(root) {
  const names = new Set();
  let directory = root;
  for (let depth = 0; depth < 16; depth += 1) {
    const scope = path.join(directory, "node_modules", "@uniflowed");
    try {
      for (const entry of readdirSync(scope)) names.add(`@uniflowed/${entry}`);
    } catch {
      // no packages at this level
    }
    const parent = path.dirname(directory);
    if (parent === directory) break;
    directory = parent;
  }
  return [...names].sort();
}
