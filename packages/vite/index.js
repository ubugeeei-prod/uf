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

import { reportRenderError } from "./internal/events.js";
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
  RESERVED,
  VIRTUAL,
  clientModuleSource,
  routesModuleSource,
  scanRoutes,
  serverModuleSource,
} from "./internal/routes.js";
import { TransformService, isFlowModule } from "@uniflowed/host/transform";
import { send, toRequest } from "./internal/http.js";
import { withRequest } from "./internal/serve.js";

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
  /**
   * The React Compiler findings already reported, so each is said once.
   *
   * `uf build` runs Vite twice — once for the browser bundle and once for the
   * server one — over the same modules, so every finding was made twice and
   * printed twice. An entry records which environment reported a module's
   * findings first: a re-transform in *that* environment (a dev server, after
   * an edit) clears it and reports again, and the other environment's pass over
   * the same module stays quiet. Keying on the environment rather than on a
   * flag is what keeps the second half true without making the first half
   * false.
   *
   * @type {Map<string, { environment: string, signatures: Set<string> }>}
   */
  const reported = new Map();
  /** Findings held back as a dependency's, waiting to be counted out loud. */
  let suppressed = [];

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
      reportDiagnostics(this, {
        id: cleanId(id),
        root,
        diagnostics: out.diagnostics,
        environment: ssr ? "ssr" : "client",
        reported,
        suppressed,
      });
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
      const styled = out.css != null && out.css !== "";
      let output = out.code;
      if (styled) {
        const styleId = `${STYLE_PREFIX}${cleanId(id)}.css`;
        styles.set(styleId, out.css);
        output = `import ${JSON.stringify(styleId)};\n${output}`;
      }
      // A module that compiled a stylesheet has a side effect, whatever its
      // package says. `@uniflowed/stylex` declares `sideEffects: false` and is
      // right about its source: `tokens.stylex.js` only exports a token set.
      // What it exports after this transform is a token set *and* a `:root`
      // block, and the page that imports `ufTokens` no longer names it at
      // runtime — the compiler turned every read into the `var(--…)` it minted.
      // So the import was unused, a side-effect-free module with no used
      // exports was dropped, and the custom properties every one of those
      // `var()`s resolves against went with it: rules that referred to nothing.
      // Declaring the side effect here rather than editing the package is
      // deliberate — the side effect is one this plugin added, so it is this
      // plugin's to admit to. See ubugeeei-prod/uf#306.
      const moduleSideEffects = styled ? true : undefined;
      if (!refresh) return { code: output, map, moduleSideEffects };
      const relative = path.relative(root, cleanId(id)).split(path.sep).join("/");
      return { ...addRefreshWrapper(output, map, relative), moduleSideEffects };
    },

    buildEnd() {
      summariseSuppressed(this, suppressed);
      suppressed = [];
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

      // A reserved file appearing or disappearing changes the route table,
      // which lives in a virtual module the watcher knows nothing about.
      //
      // Built from `RESERVED` rather than written out. It used to be the
      // literal `(page|layout|middleware|not-found)`, which is a fourth
      // spelling of a grammar that already has three, and it was already
      // missing `route` — so adding a route handler to a running dev server
      // did not rebuild the table and the handler stayed invisible until a
      // restart. A list that has to match another list has to be that list.
      const stems = Object.values(RESERVED)
        .map((stem) => stem.replaceAll(".", "\\."))
        .join("|");
      const reserved = new RegExp(`/(${stems})(\\.[a-z]+)?\\.(js|jsx|mdx)$`);
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
            const entry = await importServerEntry(devServer);
            const asRequest = await toRequest(request, devServer.config);

            // One request, owned here and settled once the document has been
            // written — the same lifecycle `driver.js` gives `uf dev` and
            // `internal/serve.js` gives `uf preview` and `uf start`. A project
            // driving Vite itself must not get a different answer about when
            // `after()` runs than the same project run through `uf dev`; see
            // `internal/serve.js` and ubugeeei-prod/uf#389.
            //
            // Only document requests reach here, so unlike `driver.js` there is
            // no path where uf hands the response back to Vite's chain: what is
            // below either writes it or throws.
            await withRequest(entry, asRequest, async () => {
              // Before the page: a middleware guards a subtree, and a page
              // rendered while the guard on it had not run is the whole of
              // ubugeeei-prod/uf#260. Only document requests reach here, so this
              // is the page half of the guarantee; `driver.js` makes the same
              // call above the route handlers, for every method.
              const guarded = await entry.runMiddleware(asRequest);
              if (guarded != null) {
                await send(response, guarded);
                return;
              }

              const result = await entry.render(
                url,
                { scripts: [devUrlFor(VIRTUAL.client)], styles: [], preloads: [] },
                { onError: (error) => reportRenderError(devServer, url, error) },
              );
              if (result.error != null) reportRenderError(devServer, url, result.error);
              // Collected rather than piped, for the reason `driver.js` gives at
              // step 4: `transformIndexHtml` is a whole-document hook.
              const html = await devServer.transformIndexHtml(url, await result.text());
              response.statusCode = result.status;
              response.setHeader("Content-Type", "text/html; charset=utf-8");
              for (const [name, value] of Object.entries(result.headers ?? {})) {
                response.setHeader(name, value);
              }
              response.end(html);
            });
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

/** The name of the environment variable that turns every finding back on. */
const ALL_DIAGNOSTICS = "UF_REACT_COMPILER_DIAGNOSTICS";

/**
 * Report what the React Compiler said about one module.
 *
 * Every finding used to be printed as `a function: <message>` — no file, no
 * line, no column, and the fallback string doing all the work because the
 * compiler names an inner function about as often as not. The transform hook
 * knows the module and the compiler gives a position for most findings, so
 * both go into the message: Vite prints a plugin warning's `message` and
 * nothing else, so a location that is not in the string is a location the
 * reader never sees. `id` and `loc` go along for anything reading the log
 * object rather than the line. See ubugeeei-prod/uf#307.
 *
 * A dependency's findings are held back. A React Compiler bailout inside
 * `@uniflowed/form` is not something the person running the build can fix, and
 * a channel carrying forty of them on every build is a channel people stop
 * reading — which costs them the one finding that was theirs. They are counted
 * and said once instead, and `UF_REACT_COMPILER_DIAGNOSTICS=all` prints every
 * one for whoever is fixing the dependency.
 */
function reportDiagnostics(context, { id, root, diagnostics, environment, reported, suppressed }) {
  if (diagnostics.length === 0) return;
  // A module transformed again by the environment that first reported it has
  // been edited; anything else is the second bundle passing over the same file.
  const previous = reported.get(id);
  const ledger =
    previous != null && previous.environment !== environment
      ? previous
      : { environment, signatures: new Set() };
  reported.set(id, ledger);

  const file = relativeId(root, id);
  const mine = isProjectModule(root, id) || process.env[ALL_DIAGNOSTICS] === "all";
  for (const diagnostic of diagnostics) {
    // Everything a reader would be shown, so two findings that would print as
    // the same line collapse into one. The compiler reports "Cannot access refs
    // during render" once per pass that noticed it — three times for one `ref`
    // — and three identical lines are not three things to fix.
    const signature = `${diagnostic.kind}\0${diagnostic.line}\0${diagnostic.column}\0${diagnostic.message}`;
    if (ledger.signatures.has(signature)) continue;
    ledger.signatures.add(signature);
    if (!mine) {
      suppressed.push(file);
      continue;
    }
    // Two conventions, both honoured. uf's own frames count columns from one
    // (`uf_term::diagnostic`) and so does every editor a reader will paste
    // `file:line:column` into; Rollup's `loc.column` counts from zero, which is
    // what the compiler already gave us. The string gets the reader's number
    // and the log object gets Rollup's.
    const at = diagnostic.line == null ? "" : `:${diagnostic.line}:${(diagnostic.column ?? 0) + 1}`;
    const who = diagnostic.function == null ? "" : ` (in ${diagnostic.function})`;
    context.warn?.({
      message: `${file}${at}: ${diagnostic.message}${who}`,
      id,
      loc:
        diagnostic.line == null
          ? undefined
          : { file: id, line: diagnostic.line, column: diagnostic.column ?? 0 },
    });
  }
}

/**
 * Say how many findings were a dependency's, and whose.
 *
 * Held back is not the same as hidden: a build that quietly drops forty
 * findings is a build that has decided for the reader that uf has no bugs. One
 * line names the packages and how to see the rest.
 */
function summariseSuppressed(context, suppressed) {
  if (suppressed.length === 0) return;
  const packages = [...new Set(suppressed.map(packageOf))].sort();
  const count = suppressed.length;
  context.warn?.(
    `${count} React Compiler ${count === 1 ? "finding" : "findings"} in ` +
      `${packages.join(", ")} — not this application's to fix; ` +
      `set ${ALL_DIAGNOSTICS}=all to see them`,
  );
}

/**
 * Whether a finding about this module is the application author's to act on.
 *
 * Inside the project root *and* outside `node_modules`, rather than
 * `node_modules` alone: uf's own packages reach an application through a
 * workspace link in this repository and through `node_modules` everywhere
 * else, and they are no more the reader's code in one case than the other.
 */
function isProjectModule(root, id) {
  const relative = path.relative(root, id);
  if (relative.startsWith("..") || path.isAbsolute(relative)) return false;
  return !relative.split(path.sep).includes("node_modules");
}

/** A module's path as a reader would write it: relative, with forward slashes. */
function relativeId(root, id) {
  const relative = path.relative(root, id);
  return relative === "" ? id : relative.split(path.sep).join("/");
}

/** The package a module belongs to, for the one line that names them. */
function packageOf(file) {
  const parts = file.split("/");
  const at = parts.lastIndexOf("node_modules");
  if (at !== -1) {
    const scoped = parts[at + 1]?.startsWith("@");
    return parts.slice(at + 1, at + (scoped ? 3 : 2)).join("/");
  }
  // No `node_modules` in the path: a workspace link, resolved to a checkout.
  // The directory the module hangs off is the closest thing to a package name
  // that is true without reading its `package.json` from a warning path.
  const up = parts.lastIndexOf("packages");
  return up === -1 ? parts.slice(0, -1).join("/") || file : parts.slice(up, up + 2).join("/");
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
