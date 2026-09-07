// @noflow
//
// Plain JavaScript: the host runs this file directly.
//
// The driver `uf dev`, `uf build`, `uf build --compile`, `uf preview` and
// `uf start` spawn.
//
//   <host> driver.js dev     --root <dir> [--mode <m>] [--host <h>] [--port <n>] [--strict-port]
//   <host> driver.js build   --root <dir> [--mode <m>] [--out-dir <dir>]
//   <host> driver.js compile --root <dir> [--mode <m>] [--out-dir <dir>] --assets <file> --bundle <dir>
//   <host> driver.js deploy  --root <dir> [--mode <m>] [--out-dir <dir>] --adapter <name> --work <dir> --output <dir>
//   <host> driver.js preview --root <dir> [--mode <m>] [--out-dir <dir>] [--host <h>] [--port <n>]
//   <host> driver.js start   --root <dir> [--out-dir <dir>] [--host <h>] [--port <n>]
//   <host> driver.js config  --root <dir>
//
// `--mode` is what `uf` resolved from `--mode`, `.uniflowed/profile` and
// `env.active`; it is Vite's mode, so it is `import.meta.env.MODE`. The `.env`
// files it selected have already been read, by `uf`, into this process's
// environment — see `viteConfig` below and `crates/uf_config/src/env_files.rs`.
// `start` has no Vite in it and therefore no mode.
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
import { send, toRequest } from "./internal/http.js";
import { withProjectConfig } from "./merge.js";
import { VIRTUAL, scanRoutes } from "./internal/routes.js";
import {
  assetsFromManifest,
  createServeHandler,
  loadBuild,
  nodeListener,
  withRequest,
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

const commands = { dev, build, compile, deploy, preview, start, config: printConfig };
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
  // `envDir: false` turns off Vite's *file* loading, and only that. uf reads
  // the `.env` cascade itself, in Rust, before this process starts — one
  // parser, one precedence, one answer for `uf dev`, `uf build`, `uf start`,
  // `uf test` and `uf run` — and sets what it read in this process's
  // environment. Vite's `loadEnv` still runs with `envDir: false` and still
  // picks every `envPrefix`-matching name out of `process.env`, so the client
  // half is Vite's own, unchanged: the prefixed subset becomes
  // `import.meta.env.*` in the browser bundle and nothing else does. See
  // `crates/uf_config/src/env_files.rs`, `docs/app/guide/env` and #259.
  //
  // A project that would rather Vite read the files can still say
  // `vite: { envDir: "." }` — its own configuration is merged over this one —
  // and then both parsers run, uf's answer still standing. `loadEnv` takes the
  // prefixed names out of the files it read and then copies every prefixed name
  // in `process.env` over the top, and uf put its own there before this process
  // started; so the second parser adds prefixed names uf did not set and
  // changes none that it did.
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
 *   2. run the middleware guarding this path, which may answer instead;
 *   3. render the URL, pointing the client script at the dev entry rather than
 *      at a built asset;
 *   4. hand the HTML to `transformIndexHtml`, which is what injects the HMR
 *      client and lets any Vite plugin see the document.
 *
 * Step 4 is why `uf dev` collects the stream instead of piping it: Vite's HTML
 * hook takes a whole document and any plugin may rewrite any part of it, so
 * there is no first byte to send until it has run. `uf start` and `uf preview`
 * have no such hook and stream — see `internal/serve.js` — and it is worth
 * being clear that this is a property of the development server rather than of
 * the renderer. Streaming through the transform is ubugeeei-prod/uf#374.
 *
 * Anything Vite already serves — a module, a public file — never reaches this,
 * because the middleware runs after Vite's own.
 */
async function dev() {
  const { createServer } = await import("vite");
  const config = await loadConfig();
  // The mode is uf's to decide, not this file's: `uf dev` resolves `--mode`,
  // the profile `uf env use` wrote and `env.active` before it starts anything,
  // and always passes the answer. The fallback is for a driver started by hand.
  const inline = await viteConfig(config, argument("--mode") ?? "development");
  const server = await createServer({ ...inline, appType: "custom" });

  // In dev the browser loads the client entry from Vite, not from a manifest;
  // its stylesheets arrive through that module rather than as <link> tags.
  const assets = { scripts: [`/@id/${VIRTUAL.client}`], styles: [], preloads: [] };

  server.middlewares.use(async (request, response, next) => {
    const url = request.originalUrl ?? request.url ?? "/";
    // Declared out here so the catch below can still settle: a request that
    // failed is a request that happened, and a middleware that logged its
    // arrival is owed its callback either way.
    let lifecycle = null;
    try {
      const entry = await server.ssrLoadModule(VIRTUAL.server);
      const asRequest = await toRequest(request, server.config);

      // The request begins here and ends when the document has been written,
      // which is what `after()` promises and what `uf preview`, `uf start` and
      // a compiled binary all do too — a middleware that logs a response's
      // status has to mean the same thing in development as in production.
      // `entry.beginRequest` rather than an import: the storage that holds the
      // request belongs to the application's own copy of `@uniflowed/server`.
      // See `internal/serve.js` and ubugeeei-prod/uf#389.
      lifecycle = entry.beginRequest(asRequest);
      const answered = await lifecycle.run(async () => {
        // Middleware first, above everything: it guards a subtree, so it has to
        // run for a page, for a route handler, and for a path under it that
        // matches neither. Running it inside the dispatcher and again inside the
        // renderer would have left `/dashboard/typo` unguarded and run it twice
        // for a path that is both.
        const guarded = await entry.runMiddleware(asRequest);
        if (guarded != null) {
          await send(response, guarded);
          return true;
        }

        // A server action next, below the guard and above the handlers. It
        // declines every request that carries no action id, so this costs a
        // page request one header lookup; and it answers every request that
        // carries one, refusals included, so an action can never fall through
        // to a route handler that happens to sit at the URL it was posted to.
        const acted = await entry.callAction(asRequest);
        if (acted != null) {
          await send(response, acted);
          return true;
        }

        // Route handlers next, and for every method: a handler is the only
        // thing that answers a POST, and it may also answer a GET for a path
        // that has no page.
        const handled = await entry.dispatch(asRequest);
        if (handled != null) {
          await send(response, handled);
          return true;
        }

        // Only a navigation reaches the renderer. A page cannot answer a POST,
        // and letting one try would turn a missing handler into a rendered page
        // with a 200 rather than a 404.
        if (request.method !== "GET" && request.method !== "HEAD") {
          return false;
        }

        const result = await entry.render(url, assets, {
          // A boundary that threw after the shell went out. `result.error` cannot
          // carry it — the caller already has the result by then — so the
          // terminal hears about it here or not at all.
          onError: (error) => reportRenderError(server, url, error),
        });
        if (result.error != null) reportRenderError(server, url, result.error);
        const html = await server.transformIndexHtml(url, await result.text());
        response.statusCode = result.status ?? 200;
        response.setHeader("content-type", "text/html; charset=utf-8");
        response.end(html);
        return true;
      });

      if (!answered) {
        // The one path where uf is not the one writing the response: a
        // non-navigation nothing claimed goes back to Vite's chain. The guard
        // has still run and may have deferred work, so `close` — the socket
        // saying the response is over, however it ended — is the only honest
        // signal left that the bytes are out.
        response.once("close", lifecycle.settle);
        next();
        return;
      }
      await lifecycle.settle();
    } catch (error) {
      if (lifecycle != null) await lifecycle.settle();
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
  watchSources(server);

  const shutdown = async () => {
    await server.close();
    process.exit(0);
  };
  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

/**
 * Tell the Rust side when a module under the project root changed.
 *
 * `uf dev` answers questions Vite does not: whether a module is a Server
 * Component, and whether a Server Component reaches for something that only
 * exists in a browser. Those are whole-project answers, so they go stale on
 * any edit and there is no module to recompute them *for* — which is why this
 * event carries no path. What it carries is "ask again".
 *
 * Vite's watcher is the only watcher. A second one over the same tree, in
 * Rust, would be a second answer to "did this file change", and two watchers
 * disagree exactly when an editor writes through a temporary file — which is
 * every editor, and which is not a thing anybody tests.
 *
 * Debounced, because a `git checkout` is one intention and several hundred
 * `change` events, and unrefed so a pending timer cannot keep this process
 * alive after the server has closed.
 */
function watchSources(server) {
  let timer = null;
  const changed = () => {
    if (timer != null) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      emit("source-changed");
    }, 50);
    timer.unref?.();
  };
  const isSource = (file) =>
    (file.endsWith(".js") || file.endsWith(".jsx")) && !file.includes("node_modules");
  for (const event of ["add", "change", "unlink"]) {
    server.watcher.on(event, (file) => {
      if (isSource(file)) changed();
    });
  }
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
  const inline = await viteConfig(config, argument("--mode") ?? "production");
  const build = await loadBuild({
    root,
    outDir: inline.build.outDir,
    serverDir: path.join(".uf", "build", "server"),
  });

  const server = await startPreview({ ...inline, appType: "custom" });
  const handle = createServeHandler({ ...build, cache: config.app?.rendering?.cache });
  server.middlewares.use(async (request, response, next) => {
    try {
      const asRequest = await toRequest(request, server.config);
      // The same lifecycle `uf start` gets from `nodeListener`, spelled out
      // because this door is Vite's connect chain rather than a bare
      // `node:http` server: the whole request runs inside it, and it settles
      // once `send` has returned. A preview whose `after()` fired at a
      // different moment from the production server's would be a preview that
      // is checked and believed and wrong.
      await withRequest(build.entry, asRequest, async () => {
        await send(response, await handle(asRequest));
      });
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
  const server = createHttpServer(
    nodeListener(
      createServeHandler({ ...build, cache: config.app?.rendering?.cache }),
      build.entry,
    ),
  );

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
  //
  // `prerender`, not `render`: a build wants the document React produces once
  // every boundary has resolved, with the content where the fallback was. The
  // streaming renderer would write a file whose slow parts are `<template>`
  // elements waiting for a script — correct in a browser, blank to a crawler
  // and to `curl`, which is most of what a static file is for.
  const failures = [];
  const failed = (url, error) => {
    failures.push(url);
    emit("page-failed", { url, ...errorEvent(error) });
  };
  for (const url of pages) {
    let result;
    try {
      result = await server.prerender(url, assets);
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
  //
  // Through the same two checks as the loop, and for the same reason. A
  // not-found boundary is a component like any other: it can throw, and when it
  // does `prerender` answers with the *error* page's HTML and a non-null
  // `error` rather than rejecting. Writing that HTML and emitting `page` was a
  // build publishing its own failure as `404.html` and exiting 0 — the static
  // host would then serve uf's error page to every visitor who mistyped a URL,
  // and nothing between the throw and the deploy would have mentioned it.
  let attempted = pages.length;
  if (server.notFound.some((boundary) => boundary.path === "/")) {
    attempted += 1;
    // `/404` rather than `/__uf_not_found__`: the internal path is how the
    // router is asked, and the file the reader is looking for is `404.html`.
    let result;
    try {
      result = await server.prerender("/__uf_not_found__", assets);
    } catch (error) {
      failed("/404", error);
      result = null;
    }
    if (result != null && result.error != null) {
      failed("/404", result.error);
      result = null;
    }
    if (result != null) {
      const file = path.join(outDir, "404.html");
      writeFileSync(file, result.html);
      emit("page", {
        url: "/404",
        file: path.relative(root, file),
        status: 404,
        bytes: Buffer.byteLength(result.html),
      });
    }
  }

  if (failures.length > 0) {
    // Emitted rather than thrown, so the message is the routes and not the
    // last exception: each one has already been reported with its own frame.
    //
    // The first line stands on its own, because it is the one `uf build` uses
    // as the headline and the one a CI log's last line will be. It read
    // `... failed:` with the routes below it, and the headline was then a
    // sentence ending in a colon and nothing.
    // `attempted`, not `pages.length`: the root 404 is prerendered too, and
    // counting a failure of it against a total that excludes it produced
    // "1 of 12" for a build that rendered thirteen things.
    emit("error", {
      message: `${failures.length} of ${attempted} prerendered ${plural(
        attempted,
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
 * What each adapter links, and what it links it against.
 *
 * Every entry in this table produces the same `handler.js` — the application
 * as `Request` → `Response`, from `@uniflowed/server/fetch` — and differs only
 * in the file wrapped around it and, for a target whose dependencies have a
 * different build, in the export conditions that pick one. That is the whole
 * of what an adapter is, and keeping the differences in one object is what
 * stops a second one from quietly becoming a second application.
 *
 * `bun`, `deno` and `static` are deliberately absent; `uf_config`'s
 * `DeployAdapter::is_implemented` is the other half of that fact and
 * `docs/app/reference/cli/_uf.page.mdx` says why for each of them.
 */
const ADAPTERS = {
  node: {
    entries: (document, cache) => ({
      handler: handlerEntrySource(document, cache, NODE_CAPABILITIES),
      server: nodeEntrySource("./handler.js"),
    }),
  },
  // The same two files. What `--adapter container` adds is a `Dockerfile` and
  // a `.dockerignore`, and both are plain text that `uf` writes beside this
  // output rather than anything the bundler produces — see `uf_cli`'s
  // `commands::deploy`.
  container: {
    entries: (document, cache) => ({
      handler: handlerEntrySource(document, cache, NODE_CAPABILITIES),
      server: nodeEntrySource("./handler.js"),
    }),
  },
  edge: {
    entries: (document, cache) => ({
      handler: handlerEntrySource(document, cache, EDGE_CAPABILITIES),
      worker: workerEntrySource("./handler.js"),
    }),
    // `workerd` first, so React resolves to the build that has
    // `renderToReadableStream` and no `node:stream`. `browser` and `module`
    // after it are Vite's own SSR defaults, kept so a dependency with no
    // worker condition still resolves the way it does for every other target.
    conditions: ["workerd", "worker", "edge-light", "browser", "module", "import", "default"],
  },
  serverless: {
    entries: (document, cache) => ({
      handler: handlerEntrySource(document, cache, SERVERLESS_CAPABILITIES),
      lambda: lambdaEntrySource("./handler.js"),
    }),
  },
};

/**
 * How each target says what it can do: the module, and the name to call.
 *
 * A pair of strings rather than a value, because this is the module that
 * *writes* `handler.js` and never imports what it writes: the capabilities
 * belong to the deployed application's copy of `@uniflowed/server`, not to the
 * driver's. It is the same reason `beginRequest` is re-exported from the
 * generated file rather than reached for here — see `handlerEntrySource`.
 *
 * Nothing is passed for `websocket` or `queue`, and that is not an oversight:
 * uf defines both and implements neither, so a generated file that invented
 * one would be inventing an upgrade for a runtime it cannot see. What the call
 * does supply is the target's name and its two facts, which is what turns "an
 * upgrade is not available" into "the serverless host cannot hold a socket
 * open" and what lets `@uniflowed/server/lambda` refuse a queue that would be
 * dropped.
 */
const NODE_CAPABILITIES = { module: "@uniflowed/server/node", name: "nodeCapabilities" };
const EDGE_CAPABILITIES = { module: "@uniflowed/server/edge", name: "edgeCapabilities" };
const SERVERLESS_CAPABILITIES = { module: "@uniflowed/server/lambda", name: "lambdaCapabilities" };

/**
 * Link the application into a directory that can be copied, for
 * `uf build --adapter`.
 *
 * `uf start` serves a build and `uf build --compile` puts one inside an
 * executable, and between them is the shape most hosts actually want: a
 * directory that carries everything and nothing that is still in the checkout
 * — no `node_modules`, no source, no `uf`. That is what this writes, for
 * whichever of [`ADAPTERS`] was asked for.
 *
 * It differs from the server build in [`build`] in one way, and that one way
 * is the whole of the difference between a build artefact and a checkout:
 * `ssr.noExternal: true`. The ordinary server build leaves `react`,
 * `react-dom` and every other dependency as bare imports, because the host it
 * runs on has `node_modules` beside it; a copied directory does not, so they
 * come in. (`@uniflowed/*` was never external — `index.js` sets
 * `ssr.noExternal: [/^@uniflowed\//]` because Node cannot import Flow — which
 * is why serving a build has never needed `uf transform` alive, and why the
 * blocker ubugeeei-prod/uf#335 records was not one.)
 *
 * # Two entries, because an adapter is exactly one of them
 *
 * `handler.js` is the application as a Web-standard `fetch` export: a
 * `Request` in, a `Response` out, no filesystem, no socket, no `node:` import
 * that a worker does not already have. That is the seam, and it is the same
 * file for every target in [`ADAPTERS`].
 *
 * The second entry is the wrapper for *this* target — `node:http` for `node`
 * and `container`, `export default { fetch }` for a Worker, `export const
 * handler` for a Lambda — and each of them is a handful of lines around an
 * import from `@uniflowed/server`. That is the point: the work is in the
 * handler, and what a new adapter has to write is the handful of lines, not
 * the application.
 *
 * Both are ordinary entries of one Rolldown build, so the wrapper imports the
 * emitted `handler.js` rather than a second copy of the application.
 *
 * The `static/` directory is *not* written here. `uf` copies it (see
 * `uf_cli`'s `commands::deploy`), because walking an output directory and
 * copying every file in it is bulk work over the whole build, which belongs in
 * Rust rather than in the host process — the same division `--compile` makes
 * with its embedded assets.
 */
async function deploy() {
  const vite = await import("vite");
  const config = await loadConfig();
  const inline = await viteConfig(config, argument("--mode") ?? "production");
  const outDir = path.resolve(root, inline.build.outDir);
  const adapter = argument("--adapter");
  const workArgument = argument("--work");
  const outputArgument = argument("--output");
  if (adapter == null || workArgument == null || outputArgument == null) {
    throw new Error("uf: `driver.js deploy` needs --adapter, --work and --output");
  }
  // The Rust side has already refused every adapter it has no implementation
  // for, by name and with the issue that tracks it. This is the second half of
  // that fact rather than a duplicate of it: the driver may be spawned by a
  // future `uf` that knows an adapter this copy does not, and answering "one
  // moment, here is a directory" for a target nobody wrote would be the silent
  // wrong answer the whole issue is about.
  const shape = ADAPTERS[adapter];
  if (shape == null) {
    throw new Error(
      `uf: this driver implements ${Object.keys(ADAPTERS)
        .map((name) => JSON.stringify(name))
        .join(", ")} and was asked for ${JSON.stringify(adapter)}`,
    );
  }
  const work = path.resolve(root, workArgument);
  const output = path.resolve(root, outputArgument);

  emit("phase", { name: adapter });

  // Written to disk rather than served as virtual modules: they are generated
  // per build — `handler.js` names this build's hashed assets — and a real
  // file is the version a person can open when a deployed directory
  // misbehaves.
  mkdirSync(work, { recursive: true });
  const document = assetsFromManifest(readManifest(outDir));
  const entries = shape.entries(document, config.app?.rendering?.cache);
  const input = {};
  for (const name of Object.keys(entries)) {
    writeFileSync(path.join(work, `${name}.js`), entries[name]);
    input[name] = path.join(work, `${name}.js`);
  }

  const ssr = { ...(inline.ssr ?? {}), noExternal: true };
  if (shape.conditions != null) {
    // Which build of a dependency this target gets, and it is the difference
    // between a worker that renders and one that fails to link. React ships
    // `server.node.js` under the `node` condition and `server.edge.js` under
    // `workerd`; the first one imports `node:stream`, and the router picks its
    // renderer by asking whether `renderToPipeableStream` is there — so the
    // condition list is what decides that, not a flag in the application.
    ssr.resolve = { ...(inline.ssr?.resolve ?? {}), conditions: shape.conditions };
  }

  await vite.build({
    ...inline,
    customLogger: eventLogger("warn"),
    plugins: [...inline.plugins, nativeAddonGuard()],
    ssr,
    build: {
      ...inline.build,
      manifest: false,
      // The map would describe this bundle rather than the source, and nothing
      // downstream reads it. Off is a smaller directory to copy and one less
      // file to explain.
      sourcemap: false,
      ssr: true,
      outDir: output,
      // `uf` has already removed the directory, and `static/` is copied in
      // after this returns; letting Vite empty it would be Vite deciding when
      // that happens.
      emptyOutDir: false,
      rollupOptions: {
        input,
        output: {
          entryFileNames: "[name].js",
          // Route modules are lazy `import()`s, so the server bundle splits
          // whether or not anything asks it to, and the chunks have to land
          // somewhere. `chunks/` rather than the default `assets/`, because
          // `static/assets/` beside it is the *client's* — two directories
          // with one name in a directory whose whole purpose is to be copied
          // and read by a stranger.
          chunkFileNames: "chunks/[name]-[hash].js",
          format: "es",
        },
      },
    },
  });

  emit("done", { outDir: path.relative(root, output), pages: 0 });
  process.exit(0);
}

/**
 * The source of `handler.js`: the application, as one `fetch` export.
 *
 * `export default { fetch }` as well as the named export, because those are
 * the two spellings the hosts this shape exists for actually read — a worker
 * and Deno Deploy want the default export's `fetch`, and a Node or Bun entry
 * wants the name. Writing both costs a line and removes the one thing that
 * would make an otherwise portable file not portable.
 *
 * `beginRequest` is exported beside it, and it is not decoration. `fetch`
 * answers with a `Response`; it does not know when that response reached
 * anybody, and `after()` promises a callback once it has. So the host owns the
 * request: begin it, run `fetch` inside `run`, and `settle` when the bytes are
 * out — `server.js` below does exactly that through
 * `@uniflowed/server/node`, and a worker hands `settle` to `ctx.waitUntil`.
 * It comes from the bundle rather than from the host's own
 * `@uniflowed/server`, because the request lives in an `AsyncLocalStorage`
 * belonging to a module instance and the instance the application reads is the
 * one inlined here. See ubugeeei-prod/uf#389.
 *
 * The document's script and stylesheet URLs are baked in here because they
 * come from the client manifest, which exists at this moment and not in the
 * directory that gets copied.
 *
 * `cache` is `rendering.cache` from `uf.config.js`, and this is where two of
 * its four switches stop being a field in a JSON file: a build that turned
 * `route` or `fetch` on constructs a store here and hands it to the handler,
 * and a build that turned neither on writes the file it always wrote, byte for
 * byte. The store is constructed in the *generated* module rather than reached
 * for inside `@uniflowed/server` for the same reason `beginRequest` is
 * re-exported above — a module-level singleton belongs to whichever copy of the
 * package a bundler happened to give it, and the copy that matters is the one
 * the application resolved. See ubugeeei-prod/uf#277 and #389.
 */
function handlerEntrySource(document, cache, capabilities) {
  const route = cache?.route === true;
  const fetchCache = cache?.fetch === true;
  // Nothing at all when both switches are off, so a default project's
  // `handler.js` is the file it has always been. A cache that appears in
  // generated output nobody asked for is the second half of the complaint
  // #277 makes about the first half.
  const store = route || fetchCache;
  const options = [
    "app",
    `document: ${JSON.stringify(document)}`,
    ...(store ? ["cache"] : []),
    "capabilities",
  ].join(", ");
  const cacheImport = store ? 'import { createCacheStore } from "@uniflowed/server/cache";\n' : "";
  const from = JSON.stringify(capabilities.module);
  const capabilityImport = `import { ${capabilities.name} } from ${from};`;
  return `// Generated by \`uf build --adapter\`. Not checked in, not edited.
import { createFetchHandler } from "@uniflowed/server/fetch";
${cacheImport}${capabilityImport}
import * as app from ${JSON.stringify(VIRTUAL.server)};

${
  store
    ? `// \`rendering.cache\` from uf.config.js. One store per process: it is
// emptied by a restart and is not shared with any other instance of this
// application. See ubugeeei-prod/uf#277.
const cache = { store: createCacheStore(), route: ${String(route)}, fetch: ${String(fetchCache)} };

`
    : ""
}// What this target can do, and it is not the same for all four: whether a
// response body reaches the client as it is produced, and whether the process
// is still there once it has. A route handler that streams events or takes a
// socket asks through this rather than finding out in production. Nothing is
// passed for the upgrade or the queue — uf defines both and implements
// neither. See \`@uniflowed/server/socket\` and \`@uniflowed/server/queue\`.
const capabilities = ${capabilities.name}();

export const fetch = createFetchHandler({ ${options} });
export const beginRequest = app.beginRequest;

export default { fetch, beginRequest };
`;
}

/**
 * The source of `server.js`: the Node socket around that handler.
 *
 * Everything host-specific about serving a build is in
 * `@uniflowed/server/node`, which is the same module `uf start` reaches
 * through `./internal/serve.js` — so a request answered here and the same
 * request answered by `uf start` go through one implementation, not two that
 * agree today.
 */
function nodeEntrySource(handlerSpecifier) {
  return `// Generated by \`uf build --adapter node\`. Not checked in, not edited.
import path from "node:path";
import { fileURLToPath } from "node:url";

import { serve } from "@uniflowed/server/node";

// \`beginRequest\` comes from the handler beside this file rather than from
// \`@uniflowed/server/node\` above, because the request has to be established in
// the storage the *application* reads, which is the copy bundled into
// \`handler.js\`. See ubugeeei-prod/uf#389.
import { beginRequest, fetch } from ${JSON.stringify(handlerSpecifier)};

// Resolved from this file and not from the working directory: a process
// manager, a container entrypoint and a person in a shell each start a server
// from wherever they happen to be, and a directory that only served its own
// assets when it was started from inside itself would be a deployment with a
// trap in it.
const staticDir = path.join(path.dirname(fileURLToPath(import.meta.url)), "static");

// Not \`await serve(...)\` at the top level. uf parses that now
// (ubugeeei-prod/uf#204) and this entry is a module, so it would work; \`.catch\`
// is the better spelling regardless — a server that cannot take its port should
// say so and exit non-zero, rather than die as an unhandled rejection.
serve({ handle: fetch, staticDir, beginRequest }).catch((error) => {
  process.stderr.write(\`uf: \${error?.message ?? String(error)}\\n\`);
  process.exit(1);
});
`;
}

/**
 * The source of `worker.js`: the Cloudflare Workers entry around that handler.
 *
 * `export default { fetch }`, which is the modules-format Worker Cloudflare
 * runs, and everything host-specific is in `@uniflowed/server/edge` — the
 * asset lookup through the `ASSETS` binding `wrangler.json` declares, and the
 * `ctx.waitUntil` that keeps the isolate alive for `after()`.
 *
 * `beginRequest` comes from the handler beside this file for the reason
 * `nodeEntrySource` gives: the request has to be established in the storage the
 * *application* reads. See ubugeeei-prod/uf#389.
 */
function workerEntrySource(handlerSpecifier) {
  return `// Generated by \`uf build --adapter edge\`. Not checked in, not edited.
import { createWorkerFetch } from "@uniflowed/server/edge";

import { beginRequest, fetch as handle } from ${JSON.stringify(handlerSpecifier)};

export default { fetch: createWorkerFetch({ handle, beginRequest }) };
`;
}

/**
 * The source of `lambda.js`: the AWS Lambda entry around that handler.
 *
 * `export const handler`, so the function's configured handler is
 * `lambda.handler`. Everything platform-specific — the payload format 2.0
 * event, the base64 rules, the `cookies` array — is in
 * `@uniflowed/server/lambda`.
 *
 * `staticDir` points at the `static/` copied beside this file, so an uploaded
 * package answers a prerendered document without any other infrastructure
 * existing. That is a starting point rather than a destination, and the module
 * it is passed to says so at length.
 */
function lambdaEntrySource(handlerSpecifier) {
  return `// Generated by \`uf build --adapter serverless\`. Not checked in, not edited.
import path from "node:path";
import { fileURLToPath } from "node:url";

import { createLambdaHandler } from "@uniflowed/server/lambda";

import { beginRequest, fetch as handle } from ${JSON.stringify(handlerSpecifier)};

// Resolved from this file and not from the working directory: Lambda sets the
// working directory to the task root today and is under no obligation to keep
// doing so, and a deployment that only found its own assets by accident is a
// deployment with a trap in it.
const staticDir = path.join(path.dirname(fileURLToPath(import.meta.url)), "static");

export const handler = createLambdaHandler({ handle, beginRequest, staticDir });
`;
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
  // Not `await serve(...)` at the top level. uf parses that now
  // (ubugeeei-prod/uf#204) and this entry is a module, so it would work;
  // `.catch` is the better spelling regardless: a binary that cannot take its
  // port should say which port and exit non-zero, rather than die as an
  // unhandled rejection.
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
