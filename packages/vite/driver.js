// @noflow
//
// Plain JavaScript: the host runs this file directly.
//
// The driver `uf dev`, `uf build`, `uf build --compile`, `uf preview` and
// `uf start` spawn.
//
//   <host> driver.js dev     --root <dir> [--host <h>] [--port <n>] [--strict-port]
//   <host> driver.js build   --root <dir> [--out-dir <dir>] [--mode <m>]
//   <host> driver.js compile --root <dir> [--out-dir <dir>] --assets <file> --bundle <dir>
//   <host> driver.js preview --root <dir> [--out-dir <dir>] [--host <h>] [--port <n>]
//   <host> driver.js start   --root <dir> [--out-dir <dir>] [--host <h>] [--port <n>]
//   <host> driver.js config  --root <dir>
//
// `uf` in Rust owns the terminal; this process owns Vite. They talk over
// stdout, one JSON event per line (see `./internal/events.js`), and the driver
// exits when its stdin closes so it cannot outlive the command that started
// it.
//
// `config` loads `uf.config.js` and prints its JSON projection. It is how the
// Rust side reads a config that may hold functions and plugin instances: the
// one host that can evaluate the file evaluates it.

import { createServer as createHttpServer } from "node:http";
import { register } from "node:module";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { emit, errorEvent, eventLogger, reportRenderError } from "./internal/events.js";
import { loadUfConfig, projectConfig } from "./internal/config.js";
import { withProjectConfig } from "./merge.js";
import { VIRTUAL, scanRoutes } from "./internal/routes.js";
import {
  assetsFromManifest,
  createServeHandler,
  loadBuild,
  nodeListener,
  send,
  toRequest,
} from "./internal/serve.js";

function argument(name) {
  const at = process.argv.indexOf(name);
  return at === -1 ? null : process.argv[at + 1];
}

function flag(name) {
  return process.argv.includes(name);
}

const command = process.argv[2];
const root = path.resolve(argument("--root") ?? process.cwd());
// Every transform in this process — the loader hooks, the config loader, the
// Vite plugin — talks to one `uf transform` started at the project root.
process.env.UF_PROJECT_ROOT = root;

// The config imports `@uniflowed/config`, which is Flow. Node needs the loader
// hooks for that; Bun is started with `--preload` on the same package's
// preload instead, and has no `register`.
//
// The hooks live in `@uniflowed/host` rather than here: they are how Flow runs
// on a Capability JS Host, and nothing in them is Vite's. `uf test` reaches for
// the same package, which is what stopped a test run from depending on a
// bundler it never loads.
if (typeof Bun === "undefined" && typeof Deno === "undefined") {
  register("@uniflowed/host/internal/node-hooks.js", import.meta.url, { data: { root } });
}

process.stdin.on("end", () => process.exit(0));
process.stdin.on("error", () => process.exit(0));
process.stdin.resume();

const commands = { dev, build, compile, preview, start, config: printConfig };
const run = commands[command];
if (run == null) {
  emit("error", { message: `unknown driver command ${JSON.stringify(command)}` });
  process.exit(2);
}

run().catch((error) => {
  emit("error", errorEvent(error));
  process.exit(1);
});

/** Load `uf.config.js`, reporting where it was found. */
async function loadConfig() {
  const { config, file } = await loadUfConfig(root);
  emit("config-loaded", { file });
  return config;
}

/** The Vite inline config a uf config describes. */
async function viteConfig(config, mode) {
  const { default: uniflowed } = await import("./index.js");
  const dev = config.dev ?? {};
  const build = config.build ?? {};
  const userPlugins = Array.isArray(config.plugins) ? config.plugins : [];
  const host = argument("--host") ?? dev.host ?? "127.0.0.1";
  const port = Number(argument("--port") ?? dev.port ?? 5173);
  const allowedHosts =
    Array.isArray(dev.allowedHosts) && dev.allowedHosts.length > 0 ? dev.allowedHosts : undefined;

  // What uf generates from the semantics it owns: where the project is, which
  // plugins make Flow compile, and the few settings uf enforces rather than
  // merely passes on — `allowedHosts` gates binding a routable address, and
  // `manifest` is how the prerender finds its assets.
  //
  // `envDir: false` rather than `envFile: false`: Vite 8 deprecated the second
  // spelling and prints a line saying so on every dev server and every build,
  // twice in the docs site's. A project turns the loader back on with
  // `vite: { envDir: "." }`, which is the default directory — the project's own
  // configuration is merged over this one, so it wins. See #259 for why uf
  // switches it off at all.
  const generated = {
    root,
    configFile: false,
    envDir: false,
    mode,
    clearScreen: false,
    customLogger: eventLogger(argument("--log-level") ?? "info"),
    plugins: [uniflowed({ root, config })],
    server: {
      host,
      port,
      strictPort: flag("--strict-port") || dev.strictPort === true,
      allowedHosts,
      fs: {
        allow: dev.fs?.allow,
        deny: dev.fs?.deny,
      },
    },
    // Not `server`, and not `dev.port` either. Vite's own default for a
    // preview is 4173 rather than 5173, and the reason is the case this
    // command exists for: somebody comparing a build against the dev server
    // they left running. Taking `dev.port` would have made the two collide,
    // and Vite would have moved the preview to the next free port and served
    // it somewhere nobody was looking.
    preview: {
      host: argument("--host") ?? "127.0.0.1",
      port: Number(argument("--port") ?? 4173),
      strictPort: flag("--strict-port"),
    },
    build: {
      outDir: argument("--out-dir") ?? build.outDir ?? "dist",
      sourcemap: build.sourcemap ?? true,
      manifest: true,
      emptyOutDir: true,
    },
  };

  // Then the project's own Vite configuration, merged over it. uf does not
  // read this and does not need to: an option added to Vite tomorrow works in
  // a uf project tomorrow, rather than after a uf release that names it.
  return withProjectConfig(generated, {
    ...(config.vite ?? {}),
    plugins: [...(config.vite?.plugins ?? []), ...userPlugins],
  });
}

/**
 * The dev server.
 *
 * Vite in middleware mode serves nothing on its own: with no `index.html` at
 * the project root it answers every navigation with "Cannot GET /", which is
 * what `uf dev` used to do for every project it started. A uf project has no
 * `index.html` — the document comes from a layout — so the server has to render
 * it, which is what this middleware does:
 *
 *   1. load the server entry through `ssrLoadModule`, so it is transformed the
 *      same way the browser's copy is and picks up edits without a restart;
 *   2. render the URL, pointing the client script at the dev entry rather than
 *      at a built asset;
 *   3. hand the HTML to `transformIndexHtml`, which is what injects the HMR
 *      client and lets any Vite plugin see the document.
 *
 * Anything Vite already serves — a module, a public file — never reaches this,
 * because the middleware runs after Vite's own.
 */
async function dev() {
  const { createServer } = await import("vite");
  const config = await loadConfig();
  const inline = await viteConfig(config, "development");
  const server = await createServer({ ...inline, appType: "custom" });

  // In dev the browser loads the client entry from Vite, not from a manifest;
  // its stylesheets arrive through that module rather than as <link> tags.
  const assets = { scripts: [`/@id/${VIRTUAL.client}`], styles: [], preloads: [] };

  server.middlewares.use(async (request, response, next) => {
    const url = request.originalUrl ?? request.url ?? "/";
    try {
      const entry = await server.ssrLoadModule(VIRTUAL.server);

      // Route handlers first, and for every method: a handler is the only
      // thing that answers a POST, and it may also answer a GET for a path
      // that has no page.
      const handled = await entry.dispatch(await toRequest(request, server.config));
      if (handled != null) {
        await send(response, handled);
        return;
      }

      // Only a navigation reaches the renderer. A page cannot answer a POST,
      // and letting one try would turn a missing handler into a rendered page
      // with a 200 rather than a 404.
      if (request.method !== "GET" && request.method !== "HEAD") {
        next();
        return;
      }

      const result = await entry.render(url, assets);
      if (result.error != null) reportRenderError(server, url, result.error);
      const html = await server.transformIndexHtml(url, result.html);
      response.statusCode = result.status ?? 200;
      response.setHeader("content-type", "text/html; charset=utf-8");
      response.end(html);
    } catch (error) {
      // Map the stack back onto the Flow source before it reaches the overlay.
      if (error instanceof Error) server.ssrFixStacktrace(error);
      next(error);
    }
  });

  await server.listen();
  const urls = server.resolvedUrls ?? { local: [], network: [] };
  emit("listening", {
    local: urls.local,
    network: urls.network,
    routes: scanRoutes(path.resolve(root, config.app?.router?.root ?? "app")).routes.map(
      (route) => route.path,
    ),
  });

  const shutdown = async () => {
    await server.close();
    process.exit(0);
  };
  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

/**
 * The preview server: the build, as Vite serves it.
 *
 * Vite's `preview()` is a static file server, and a uf build is not only
 * static files — a route handler answers a `POST` and a route with parameters
 * and no `generateStaticParams` was never prerendered. On its own it would
 * therefore 404 every request the interesting half of an application exists to
 * answer, which is worse than having no preview at all, because a preview is
 * checked and believed.
 *
 * So the application handler is mounted behind it, and `appType: "custom"` is
 * what makes that reachable: with Vite's default `spa` it inserts an
 * index.html fallback and a 404 middleware of its own, so every unmatched path
 * would have been answered with the home page — a 200 for a path that does not
 * exist — before anything of uf's ran.
 *
 * The static middleware still runs first, and that is deliberate rather than
 * incidental; see `internal/serve.js` for why `uf start` orders itself the
 * same way.
 */
async function preview() {
  const { preview: startPreview } = await import("vite");
  const config = await loadConfig();
  const inline = await viteConfig(config, "production");
  const build = await loadBuild({
    root,
    outDir: inline.build.outDir,
    serverDir: path.join(".uf", "build", "server"),
  });

  const server = await startPreview({ ...inline, appType: "custom" });
  const handle = createServeHandler(build);
  server.middlewares.use(async (request, response, next) => {
    try {
      await send(response, await handle(await toRequest(request, server.config)));
    } catch (error) {
      next(error);
    }
  });

  const urls = server.resolvedUrls ?? { local: [], network: [] };
  emit("listening", {
    local: urls.local,
    network: urls.network,
    routes: build.entry.routes.map((route) => route.path),
    handlers: build.entry.handlers.map((handler) => handler.path),
  });

  const shutdown = async () => {
    await server.close();
    process.exit(0);
  };
  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

/**
 * The production server: the build, with no bundler in the process.
 *
 * `preview` proves the build works through Vite. This is the thing that is
 * actually deployed, and it imports `vite` nowhere — a host running a built
 * application should not need the bundler that produced it, and the moment it
 * does, "portable output" is a claim rather than a property.
 *
 * There is no `--strict-port` here and there is nothing to add: this server
 * binds the port it was given or fails, where Vite's would have quietly moved
 * to the next free one. `PORT` and `HOST` are read from the environment
 * because that is how every process manager and container platform says which
 * socket to take, and a production server that could only be told on the
 * command line would need a wrapper script everywhere it ran.
 *
 * It is not the only thing that can be deployed. `uf build --compile` puts
 * this same application behind this same resolution order inside a single
 * executable, for a host that should not have to have a JavaScript runtime
 * installed at all; see [`compile`] for what that costs and what it shares.
 */
async function start() {
  const config = await loadConfig();
  const outDir = argument("--out-dir") ?? config.build?.outDir ?? "dist";
  const build = await loadBuild({
    root,
    outDir,
    serverDir: path.join(".uf", "build", "server"),
  });

  const host = argument("--host") ?? process.env.HOST ?? "0.0.0.0";
  const port = Number(argument("--port") ?? process.env.PORT ?? 3000);
  const server = createHttpServer(nodeListener(createServeHandler(build)));

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, host, resolve);
  });

  const bound = server.address();
  // `0.0.0.0` is not a URL anybody can open, so the loopback spelling is what
  // is printed as `local` and the bound address is reported as the network
  // one — the same split `uf dev` prints, and for the same reason: one of the
  // two is a link and the other is a fact about the socket.
  const shown = `${bound.address}:${bound.port}`;
  const wildcard = bound.address === "0.0.0.0" || bound.address === "::";
  emit("listening", {
    local: [`http://${wildcard ? `localhost:${bound.port}` : shown}/`],
    network: wildcard ? [`http://${shown}/`] : [],
    routes: build.entry.routes.map((route) => route.path),
    handlers: build.entry.handlers.map((handler) => handler.path),
  });

  const shutdown = () => {
    server.close(() => process.exit(0));
  };
  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

async function build() {
  const vite = await import("vite");
  const config = await loadConfig();
  const mode = argument("--mode") ?? "production";
  const inline = await viteConfig(config, mode);
  const outDir = path.resolve(root, inline.build.outDir);
  const serverDir = path.join(root, ".uf", "build", "server");

  // 1. The client: everything the browser loads, with a manifest so the
  //    server render knows which script and stylesheet tags to write.
  emit("phase", { name: "client" });
  await vite.build({
    ...inline,
    build: {
      ...inline.build,
      rollupOptions: { input: { client: VIRTUAL.client } },
    },
  });
  const manifest = readManifest(outDir);

  // 2. The server entry, bundled for the host, outside `dist/` so it is never
  //    deployed by accident.
  emit("phase", { name: "server" });
  rmSync(serverDir, { recursive: true, force: true });
  await vite.build({
    ...inline,
    customLogger: eventLogger("warn"),
    build: {
      ...inline.build,
      manifest: false,
      ssr: true,
      outDir: serverDir,
      rollupOptions: {
        input: { server: VIRTUAL.server },
        output: { entryFileNames: "server.js", format: "es" },
      },
    },
  });

  // 3. Every static route, rendered to an HTML document.
  emit("phase", { name: "prerender" });
  const server = await import(pathToFileURL(path.join(serverDir, "server.js")).href);
  const assets = assetsFromManifest(manifest);
  const pages = await staticPaths(server.routes);

  // A route that throws fails *that route*, and the rest of the build still
  // happens. This loop had no `try`: the first page to throw rejected out of
  // `build()`, `run().catch` reported the exception, and which URL was being
  // rendered was a local variable nobody could see. One broken page was the
  // whole build, and the message named a stack rather than a route.
  //
  // The render itself no longer throws for an ordinary component failure —
  // `createRenderer` renders the error boundary and reports the exception on
  // the result — so both are checked here. Neither writes a file: an error
  // page written into `dist/` is a build that shipped its own failure.
  const failures = [];
  const failed = (url, error) => {
    failures.push(url);
    emit("page-failed", { url, ...errorEvent(error) });
  };
  for (const url of pages) {
    let result;
    try {
      result = await server.render(url, assets);
    } catch (error) {
      failed(url, error);
      continue;
    }
    if (result.error != null) {
      failed(url, result.error);
      continue;
    }
    const file = htmlPathFor(outDir, url);
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, result.html);
    emit("page", {
      url,
      file: path.relative(root, file),
      status: result.status,
      bytes: Buffer.byteLength(result.html),
    });
  }
  // One `404.html`, from the boundary at the router root: a static host serves
  // a single error document for the whole site, so the nested boundaries a
  // project declares are the server's and the client's to render, not
  // something this loop can write a file for.
  //
  // The condition is "there is a root boundary", not "there is any boundary",
  // because `/__uf_not_found__` is a path at the root: a project whose only
  // `_uf.not-found.js` is in `app/guide/` would otherwise get a `404.html`
  // rendered from the framework's bare default, which is worse than the file
  // it used to write, which was none.
  if (server.notFound.some((boundary) => boundary.path === "/")) {
    const result = await server.render("/__uf_not_found__", assets);
    const file = path.join(outDir, "404.html");
    writeFileSync(file, result.html);
    emit("page", {
      url: "/404",
      file: path.relative(root, file),
      status: 404,
      bytes: Buffer.byteLength(result.html),
    });
  }

  if (failures.length > 0) {
    // Emitted rather than thrown, so the message is the routes and not the
    // last exception: each one has already been reported with its own frame.
    //
    // The first line stands on its own, because it is the one `uf build` uses
    // as the headline and the one a CI log's last line will be. It read
    // `... failed:` with the routes below it, and the headline was then a
    // sentence ending in a colon and nothing.
    emit("error", {
      message: `${failures.length} of ${pages.length} prerendered ${plural(
        pages.length,
        "route",
      )} failed\n${failures.map((url) => `  ${url}`).join("\n")}`,
    });
    process.exit(1);
  }

  emit("done", { outDir: path.relative(root, outDir), pages: pages.length });
  process.exit(0);
}

/**
 * Link the whole application into one JavaScript file, for `uf build --compile`.
 *
 * This runs after `build`, on a `dist/` that is already complete, and produces
 * the module a runtime is wrapped around. It differs from the server build in
 * `build()` in exactly three ways, and each of them is what "one file" means:
 *
 *   * `ssr.noExternal: true` — the server build leaves `react`, `react-dom`
 *     and every other dependency as bare imports, because the host it runs on
 *     has `node_modules` beside it. A binary does not, so they come in.
 *   * `codeSplitting: false` — a route is a lazy `import()` so that the browser
 *     can fetch one chunk per page. On the server that split buys nothing and
 *     costs everything: chunks are separate files, and separate files are the
 *     one thing this output may not have.
 *   * the native-addon guard below, which turns "cannot resolve" into a
 *     sentence naming the package that cannot be compiled.
 *
 * The embedded copy of `dist/` is *not* built here. `uf` writes it (see
 * `uf_bundle::embed`) and passes its path in `--assets`, because walking an
 * output directory and encoding every file in it is bulk work over the whole
 * build, which belongs in Rust rather than in the host process.
 *
 * # Three front doors onto one build, and why they are not one function
 *
 * `preview` and `start` above serve `dist/` from disk, and they share a single
 * handler in `./internal/serve.js` for the express purpose of being unable to
 * answer differently. What is linked here is a third front door onto the same
 * build, and it deliberately does *not* import that module. Two reasons, and
 * either would be enough: `internal/serve.js` answers by opening files under
 * `dist/`, and a compiled binary has no `dist/` to open — it carries the bytes
 * — so the half that reads a request would arrive with a half that cannot run;
 * and it lives in `@uniflowed/vite`, so linking it would put the package named
 * after the bundler inside the artefact a deployment runs, which is the one
 * thing `start` exists to avoid.
 *
 * What a binary uses instead is `@uniflowed/server/standalone`, and the thing
 * that is shared between the three is not code but the *answer*: an asset or a
 * prerendered document first, then a route handler, then a render for whatever
 * is left. That order is not a preference. `preview` cannot deviate from it —
 * Vite's preview server runs its own file middleware before anything uf mounts
 * behind it — so `start` matches Vite, and the binary matches `start`. A
 * compiled application that resolved a collision the other way would be the
 * trap `preview` exists to prevent, one deployment further along, and the only
 * copy nobody can check with `uf preview` first.
 */
async function compile() {
  const vite = await import("vite");
  const config = await loadConfig();
  const inline = await viteConfig(config, argument("--mode") ?? "production");
  const outDir = path.resolve(root, inline.build.outDir);
  const assetsArgument = argument("--assets");
  const bundleArgument = argument("--bundle");
  if (assetsArgument == null || bundleArgument == null) {
    throw new Error("uf: `driver.js compile` needs both --assets and --bundle");
  }
  const assets = path.resolve(root, assetsArgument);
  const bundleDir = path.resolve(root, bundleArgument);

  emit("phase", { name: "standalone" });

  // The entry is written to disk rather than served as another virtual module:
  // it is generated per build (it names this build's asset file), and a real
  // file is the version a person can open when a compiled binary misbehaves.
  const entry = path.join(bundleDir, "entry.js");
  mkdirSync(bundleDir, { recursive: true });
  const specifier = `./${path.relative(bundleDir, assets)}`;
  writeFileSync(entry, entrySource(specifier, assetsFromManifest(readManifest(outDir))));

  await vite.build({
    ...inline,
    customLogger: eventLogger("warn"),
    plugins: [...inline.plugins, nativeAddonGuard()],
    ssr: { ...(inline.ssr ?? {}), noExternal: true },
    build: {
      ...inline.build,
      manifest: false,
      // The map would describe this intermediate bundle rather than the
      // binary, and nothing downstream reads it. Turning it off is a smaller
      // `.uf/` and one less file to explain.
      sourcemap: false,
      ssr: true,
      outDir: bundleDir,
      emptyOutDir: false,
      rollupOptions: {
        input: { server: entry },
        output: { entryFileNames: "server.js", format: "es", codeSplitting: false },
      },
    },
  });

  emit("done", { outDir: path.relative(root, bundleDir), pages: 0 });
  process.exit(0);
}

/**
 * The source of the module a runtime gets wrapped around.
 *
 * Three imports and one call: the shim that serves, the application, and the
 * bytes of `dist/`. The document's script and stylesheet URLs are baked in
 * here because they come from the client manifest, which exists at this moment
 * and not inside the binary.
 */
function entrySource(assetsSpecifier, document) {
  // Not `await serve(...)` at the top level. Node runs top-level `await`
  // happily and the Flow parser uf vendors does not parse it (ubugeeei-prod/uf#204),
  // so the generated entry would fail its own transform. `.catch` is the better
  // spelling anyway: a binary that cannot take its port should say which port
  // and exit non-zero, rather than die as an unhandled rejection.
  return `// Generated by \`uf build --compile\`. Not checked in, not edited.
import { serve } from "@uniflowed/server/standalone";
import { assets } from ${JSON.stringify(assetsSpecifier)};
import * as app from ${JSON.stringify(VIRTUAL.server)};

serve({ app, assets, document: ${JSON.stringify(document)} }).catch((error) => {
  process.stderr.write(\`uf: \${error?.message ?? String(error)}\n\`);
  process.exit(1);
});
`;
}

/**
 * Refuse a native addon by name instead of by stack trace.
 *
 * A `.node` file is a compiled shared object for one platform: it cannot be
 * inlined into a JavaScript bundle, and a binary that carried one would stop
 * being a single file. Without this, `ssr.noExternal: true` hands the addon to
 * Rolldown and the build fails somewhere inside the bundler with a message
 * about an unexpected character — which is true, and useless. Failing here
 * with the addon's path and the importer that reached it is the difference
 * between a feature and a trap.
 *
 * It catches what can be caught: a static `import` or `require` that resolves
 * to a `.node` file. An addon loaded through a runtime string — `process.dlopen`,
 * or `require(variable)` — is not visible to any bundler, so such a project
 * still compiles and still fails on the first request that reaches the addon.
 * That limit is real, it is not fixable from inside a bundler, and it is
 * written down in the CLI reference rather than papered over.
 */
function nativeAddonGuard() {
  return {
    name: "uf:no-native-addons",
    enforce: "pre",
    resolveId(source, importer) {
      if (!source.endsWith(".node")) return null;
      const from = importer == null ? "the application" : path.relative(root, importer);
      throw new Error(
        `${from} loads the native addon ${source}, and \`uf build --compile\` cannot put one ` +
          "inside a single executable: a `.node` file is a shared object built for one " +
          "platform, and embedding it would make the output two files rather than one. " +
          "Build without `--compile` and deploy `dist/` with a runtime, or replace the " +
          "dependency with one that has no native addon.",
      );
    },
  };
}

/** `word`, pluralised for `count`. */
function plural(count, word) {
  return count === 1 ? word : `${word}s`;
}

async function printConfig() {
  const config = await loadConfig();
  emit("config", { config: projectConfig(config) });
  process.exit(0);
}

function readManifest(outDir) {
  const file = path.join(outDir, ".vite", "manifest.json");
  if (!existsSync(file)) throw new Error(`uf: the client build wrote no manifest at ${file}`);
  return JSON.parse(readFileSync(file, "utf8"));
}

/**
 * The URLs to prerender: every route without parameters, plus every set of
 * parameters a page's `generateStaticParams` returns.
 */
async function staticPaths(routes) {
  const urls = [];
  for (const route of routes) {
    if (route.params.length === 0) {
      urls.push(route.path);
      continue;
    }
    const module = await route.page();
    const generate = module.generateStaticParams;
    if (typeof generate !== "function") continue;
    for (const params of await generate()) {
      urls.push(fillParams(route.path, params));
    }
  }
  return urls;
}

function fillParams(routePath, params) {
  return routePath
    .split("/")
    .map((segment) => {
      if (segment.endsWith("*")) {
        const value = params[segment.slice(1, -1)];
        return Array.isArray(value)
          ? value.map(encodeURIComponent).join("/")
          : encodeURIComponent(String(value ?? ""));
      }
      if (segment.startsWith(":"))
        return encodeURIComponent(String(params[segment.slice(1)] ?? ""));
      return segment;
    })
    .join("/");
}

function htmlPathFor(outDir, url) {
  const pathname = url.split("?")[0].replace(/^\/+/, "");
  return pathname === ""
    ? path.join(outDir, "index.html")
    : path.join(outDir, pathname, "index.html");
}
