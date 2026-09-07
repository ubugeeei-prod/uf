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
//                 development it also answers every request the way a
//                 deployment does — the guard, a server action, a route
//                 handler, then the renderer — so `uf dev` serves what
//                 `uf start` serves rather than an approximation of it, and it
//                 serves the two `/__uf/` paths a browser reports to (see
//                 `internal/diagnostics.js`).
//                 The client's copy of the route table is not the server's:
//                 `internal/rsc.js` reads the RSC analysis and leaves out the
//                 page of every route no client boundary reaches, so that
//                 route's modules never enter the browser bundle.
// * `uf:mdx`    — `@mdx-js/rollup`, configured for React with GitHub-flavoured
//                 markdown, front matter, heading ids and build-time syntax
//                 highlighting, so `.mdx` works with
//                 no configuration.
// * `uf:asset`  — an imported image is decoded, resized to the widths the
//                 project declares and re-encoded by `uf assets`, and an
//                 imported font is self-hosted with the `@font-face` and the
//                 metric-matched fallback that stop the swap moving the page.
//                 The import evaluates to what `Image` and `Font` need — the
//                 intrinsic size, every emitted variant, the placeholder —
//                 rather than to a URL string. See `internal/assets.js`.
//
// `uniflowed(options)` returns the array; a project that wants to add a plugin
// declares it in `uf.config.js` and the driver appends it after these.

import { readdirSync } from "node:fs";
import path from "node:path";

import mdx from "@mdx-js/rollup";
import rehypeSlug from "rehype-slug";

import { assetPlugin } from "./internal/assets.js";
import { emit, reportRenderError } from "./internal/events.js";
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
  RSC_MANIFEST_ENV,
  actionReferenceSource,
  actionsModuleSource,
  clientRouteFilter,
  readRscManifest,
  rscManifestKey,
  serverActionModules,
  serverActionTable,
} from "./internal/rsc.js";
import {
  RESERVED,
  VIRTUAL,
  clientModuleSource,
  routesModuleSource,
  scanRoutes,
  serverModuleSource,
} from "./internal/routes.js";
import { TransformService, isFlowModule } from "@uniflowed/host/transform";
import { createChannelMiddleware } from "./internal/diagnostics.js";
import { send, toRequest } from "./internal/http.js";
import { beginRequest } from "./internal/serve.js";

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
  const builtins = app.builtins ?? {};

  return [
    flowPlugin({ routerRoot, appEntry, command: options.command }),
    mdxPlugin(markdown),
    assetPlugin({
      images: builtins.images ?? {},
      fonts: builtins.fonts ?? {},
      command: options.command,
    }),
  ];
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

  /**
   * The browser's copy of the route table.
   *
   * The manifest is read here rather than once at start-up because `uf dev`
   * rewrites it whenever the graph moves, and this hook runs again when it
   * does — a table built from a manifest read at start-up would be the answer
   * for the project as it was when the server started.
   *
   * The count is emitted rather than computed on the Rust side, and that is
   * the point of it: `uf build` prints what the table it just generated
   * contains, not what a second implementation of this decision predicted it
   * would. Only for a build — a dev server has no summary to be true in.
   */
  const clientRoutesModule = (table) => {
    const shipsPage = clientRouteFilter(
      readRscManifest(process.env[RSC_MANIFEST_ENV]),
      root,
      table,
    );
    const kept = new Set(table.routes.filter(shipsPage));
    if (server == null) {
      emit("rsc-split", { pages: kept.size, routes: table.routes.length });
    }
    // `relativeTo` only here, and never for the server's copy below. Every
    // `import()` in this table becomes a chunk URL, but `file` is a string and
    // survives the build — so the browser's table was shipping the absolute
    // path of every page on the machine that built the site, to every visitor.
    // The server's table is read where those files are and keeps them.
    return routesModuleSource(table, {
      shipsPage: (route) => kept.has(route),
      relativeTo: root,
    });
  };

  /**
   * The manifest's two action tables, re-read only when the file changes.
   *
   * `load` runs for every module in the graph and has to ask "is this a
   * `"use server"` module?" about each one, so parsing the manifest per call
   * would put a JSON parse of the whole graph between Vite and every file it
   * opens. Size and modification time are the identity — the same pair
   * `.uf/cache/transform` keys on — and `uf dev` drops the memo outright when
   * its watcher sees the file change, so a rewrite inside one millisecond is
   * still seen.
   */
  let actionMemo = null;
  const forgetActions = () => {
    actionMemo = null;
  };
  const actionTables = () => {
    const file = process.env[RSC_MANIFEST_ENV];
    const key = rscManifestKey(file);
    if (actionMemo == null || actionMemo.key !== key) {
      const manifest = readRscManifest(file);
      actionMemo = {
        key,
        modules: serverActionModules(manifest, root),
        table: serverActionTable(manifest, root),
      };
    }
    return actionMemo;
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

    load(id, loadOptions) {
      if (id === RUNTIME_RESOLVED_ID) return refreshRuntimeSource();
      if (id === resolved(VIRTUAL.routes)) {
        const table = scanRoutes(appRoot);
        // The server renders every route, so the server's table is the whole
        // one and is generated with no filter at all. Only the browser's copy
        // is split.
        if (isSsr(this, loadOptions)) return routesModuleSource(table);
        return clientRoutesModule(table);
      }
      if (id === resolved(VIRTUAL.client)) return clientModuleSource(entryPath);
      if (id === resolved(VIRTUAL.server)) return serverModuleSource(entryPath);
      // Only `virtual:uf/server` imports this, so it is only ever asked for in
      // the server environment — but the table it carries is every callable
      // endpoint of the build, so it is worth saying that a browser asking for
      // it gets nothing rather than getting the list.
      if (id === resolved(VIRTUAL.actions)) {
        if (!isSsr(this, loadOptions))
          return "export const actions = [];\nexport default actions;\n";
        return actionsModuleSource(actionTables().table);
      }
      if (id.startsWith(STYLE_PREFIX)) return styles.get(id) ?? "";

      // A `"use server"` module, in the browser's graph only: what the client
      // gets is one `createServerReference` per callable export, and never the
      // file. This is where the second half of the RSC split actually happens
      // — the route filter above decides which *pages* the browser is given,
      // and this decides that an action module's body, its imports and
      // everything only they reached are not the browser's business at all.
      //
      // Substituting the source rather than rewriting it: a transform that
      // stripped the body would have to be right about every way a module can
      // name something, and being wrong once means shipping a database handle.
      // The exports the reference module declares come from the manifest, so
      // they are exactly the exports `uf_rsc` decided are callable endpoints
      // and an import of anything else is a build error rather than a silent
      // `undefined`. `crates/uf_rsc/src/graph/build.rs` colours these modules
      // server for the same reason, so the analysis and the bundle agree.
      if (!isSsr(this, loadOptions)) {
        const references = actionTables().modules.get(cleanId(id));
        if (references != null) return actionReferenceSource(references);
      }
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

      // The same problem one level up. Adding `"use client"` to a module, or
      // deleting the import that reached it, changes which routes the browser
      // is given a page for — and touches no reserved file name, so nothing
      // above notices. `uf dev` rewrites the RSC manifest when the analysis
      // moves and only then, so this fires when the answer changed rather than
      // on every keystroke. Watched explicitly because the file is uf's own
      // artefact and is in no module graph.
      const manifestFile = process.env[RSC_MANIFEST_ENV];
      if (manifestFile != null && manifestFile !== "") {
        const manifestPath = path.resolve(manifestFile);
        devServer.watcher.add(manifestPath);
        const onManifest = (file) => {
          if (path.resolve(file) !== manifestPath) return;
          // The action tables are read from the same file and are memoised on
          // its size and modification time, which is a pair two writes inside
          // one millisecond can share. This is the answer that does not
          // depend on a clock.
          forgetActions();
          const routes = devServer.moduleGraph.getModuleById(resolved(VIRTUAL.routes));
          if (routes) devServer.moduleGraph.invalidateModule(routes);
          const actions = devServer.moduleGraph.getModuleById(resolved(VIRTUAL.actions));
          if (actions) devServer.moduleGraph.invalidateModule(actions);
          devServer.ws.send({ type: "full-reload", path: "*" });
        };
        devServer.watcher.on("add", onManifest);
        devServer.watcher.on("change", onManifest);
      }

      // After Vite's own middlewares, so `/@vite/client`, `/@id/...` and
      // static files are served first and only what Vite declined reaches uf.
      //
      // # One renderer
      //
      // This is the only middleware that renders a document under `uf dev`,
      // and that is worth stating because there were two. `driver.js` added a
      // second one after `createServer` had returned, which put it *later* in
      // the connect stack than this one — so for every request this one
      // claimed, this one decided, and the other was reached only for what
      // this one declined. Route-handler dispatch was in the other. A
      // `GET /feed` from a browser is `Accept: text/html` with no extension,
      // so it looked like a document, so `app/feed/_uf.route.js` was never
      // asked and the reader got the route table's page — or the not-found
      // page — for a path that had a handler. See ubugeeei-prod/uf#349.
      //
      // Two renderers is also how the two came to disagree about the render
      // result's `headers`: this one wrote them and the driver's did not, so a
      // loader calling `redirect()` answered a browser with a `307` carrying
      // no `Location` and a meta-refresh body — a redirect that works when you
      // deploy it and not while you are writing it. See
      // ubugeeei-prod/uf#338.
      //
      // So the driver's middleware is gone and everything it did is here: it
      // claims every request, runs the guard, the action endpoint and the
      // dispatcher for every method, and hands back to Vite's chain what none
      // of them answered.
      //
      // # What is still not `createFetchHandler`
      //
      // One middleware rather than two, and still not the function every
      // deployment runs. It cannot be: `transformIndexHtml` takes a whole
      // document, so the render has to be collected here rather than streamed
      // (ubugeeei-prod/uf#374), and a request nothing claimed has to go back
      // to Vite's chain rather than become a 404 — neither of which a handler
      // that always answers with a `Response` can do.
      //
      // What that still costs, written down so the next reader does not have
      // to find it: `createFetchHandler` renders inside a cache scope even
      // with no cache configured, so a component calling `cacheLife` states a
      // lifetime nobody honours; here there is no scope, so the same component
      // throws under `uf dev` and renders under `uf start`. Closing that means
      // the generated server entry handing the host a scope the way it already
      // hands it `beginRequest` — see `serverModuleSource` in
      // `./internal/routes.js` — and it is the next thing to remove from this
      // list rather than something this middleware can decide on its own.
      return () => {
        // The browser's own reporting channel, mounted above the application
        // so that a report never reaches a project's `_uf.middleware.js` or
        // its route table. See `internal/diagnostics.js`.
        devServer.middlewares.use(
          createChannelMiddleware((diagnostic) => emit("diagnostic", diagnostic)),
        );

        devServer.middlewares.use(async (request, response, next) => {
          // `request.url` and not `originalUrl`, which is the URL Vite's base
          // middleware has already stripped the base from — and the route
          // table's paths have no base in them either.
          const url = request.url ?? "/";
          // Declared out here so the catch below can still settle: a request
          // that failed is a request that happened, and a middleware that
          // logged its arrival is owed its callback either way.
          let lifecycle = null;
          try {
            const entry = await importServerEntry(devServer);
            const asRequest = await toRequest(request, devServer.config);

            // The request begins here and ends when the response has been
            // written, which is what `after()` promises and what `uf preview`,
            // `uf start` and a compiled binary all do too — a middleware that
            // logs a response's status has to mean the same thing in
            // development as in production. See `internal/serve.js` and
            // ubugeeei-prod/uf#389.
            lifecycle = await beginRequest(entry, asRequest);
            const answered = await lifecycle.run(async () => {
              // Before anything answers: a middleware guards a subtree, so it
              // has to run for a page, for a route handler, and for a path
              // under it that matches neither. Running it inside the
              // dispatcher and again inside the renderer would have left
              // `/dashboard/typo` unguarded and run it twice for a path that
              // is both. See ubugeeei-prod/uf#260.
              const guarded = await entry.runMiddleware(asRequest);
              if (guarded != null) {
                await send(response, guarded);
                return true;
              }

              // Then a server action, below the guard and above the handlers.
              // It declines every request that carries no action id, so this
              // costs an ordinary request one `headers.get`; and it answers
              // every request that carries one, refusals included, so an
              // action can never fall through to a route handler that happens
              // to sit at the URL it was posted to. The same two lines are in
              // `@uniflowed/server`'s `fetch.js`, which is what every
              // deployment runs.
              const acted = await entry.callAction(asRequest);
              if (acted != null) {
                await send(response, acted);
                return true;
              }

              // Then the route handlers, above the renderer and for every
              // method: a path that answers a request is not a document,
              // whatever the client said it would accept. `curl /api/thing`
              // and a `<form action>` navigation both send `Accept:
              // text/html`, and both want the handler's answer — which is the
              // whole of ubugeeei-prod/uf#349.
              const handled = await entry.dispatch(asRequest);
              if (handled != null) {
                await send(response, handled);
                return true;
              }

              // Only a navigation reaches the renderer. A page cannot answer a
              // `POST`, and letting one try would turn a missing handler into
              // a rendered page with a 200 rather than a 404.
              //
              // `wantsDocument` is stricter than the production handler, which
              // renders anything a static file did not answer, and the
              // difference is Vite's chain: `/@id/…`, `/node_modules/…` and
              // any path with an extension belong to the module server, and a
              // request one of those declined has to go back to it rather than
              // become a rendered 404 page. What it costs is that
              // `/favicon.svg` on a project that has none is a bare 404 here
              // and the project's own not-found *page* under `uf preview` and
              // `uf start` — a difference in the body of a 404 for a path that
              // is an asset request in the first place.
              if (!wantsDocument(request)) return false;

              const result = await entry.render(
                url,
                { scripts: [devUrlFor(VIRTUAL.client)], styles: [], preloads: [] },
                {
                  // A boundary that threw after the shell went out.
                  // `result.error` cannot carry it — the caller already has the
                  // result by then — so the terminal hears about it here or not
                  // at all.
                  onError: (error) => reportRenderError(devServer, url, error),
                },
              );
              if (result.error != null) reportRenderError(devServer, url, result.error);
              // Collected rather than piped: `transformIndexHtml` is a
              // whole-document hook, so there is no first byte to send until it
              // has run. `uf start` and `uf preview` stream — see
              // `internal/serve.js` — and that is a property of the development
              // server rather than of the renderer. ubugeeei-prod/uf#374.
              const html = await devServer.transformIndexHtml(url, await result.text());
              response.statusCode = result.status ?? 200;
              // The render's own headers, then the content type over the top:
              // exactly the order `@uniflowed/server`'s `fetch.js` writes them
              // in, so a `Location` from `redirect()` survives here and a
              // render cannot claim to be something other than a document.
              for (const [name, value] of Object.entries(result.headers ?? {})) {
                response.setHeader(name, value);
              }
              response.setHeader("content-type", "text/html; charset=utf-8");
              response.end(html);
              return true;
            });

            if (!answered) {
              // The one path where uf is not the one writing the response: a
              // request nothing claimed goes back to Vite's chain. The guard
              // has still run and may have deferred work, so `close` — the
              // socket saying the response is over, however it ended — is the
              // only honest signal left that the bytes are out.
              response.once("close", lifecycle.settle);
              next();
              return;
            }
            await lifecycle.settle();
          } catch (error) {
            if (lifecycle != null) await lifecycle.settle();
            // Map the stack back onto the Flow source before it reaches the
            // overlay.
            if (error instanceof Error) devServer.ssrFixStacktrace(error);
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

/**
 * Whether a hook is running for the server environment.
 *
 * Both spellings, for the reason `transform` above checks both: Vite 6 moved
 * the answer onto the plugin context and the `ssr` option is the older one.
 */
function isSsr(context, options) {
  return options?.ssr === true || context?.environment?.name === "ssr";
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
