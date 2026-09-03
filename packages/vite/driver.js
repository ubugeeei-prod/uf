// Plain JavaScript: the host runs this file directly.
//
// The driver `uf dev`, `uf build` and `uf preview` spawn.
//
//   <host> driver.js dev     --root <dir> [--host <h>] [--port <n>] [--strict-port]
//   <host> driver.js build   --root <dir> [--out-dir <dir>] [--mode <m>]
//   <host> driver.js preview --root <dir> [--host <h>] [--port <n>]
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

import { register } from "node:module";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { emit, errorEvent, eventLogger } from "./internal/events.js";
import { loadUfConfig, projectConfig } from "./internal/config.js";
import { VIRTUAL, scanRoutes } from "./internal/routes.js";

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
// hooks for that; Bun is started with `--preload ./bun-preload.js` instead
// and has no `register`.
if (typeof Bun === "undefined" && typeof Deno === "undefined") {
  register("./internal/node-hooks.js", import.meta.url, { data: { root } });
}

process.stdin.on("end", () => process.exit(0));
process.stdin.on("error", () => process.exit(0));
process.stdin.resume();

const commands = { dev, build, preview, config: printConfig };
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
  const allowedHosts = Array.isArray(dev.allowedHosts) && dev.allowedHosts.length > 0 ? dev.allowedHosts : undefined;

  return {
    root,
    configFile: false,
    envFile: false,
    mode,
    clearScreen: false,
    customLogger: eventLogger(argument("--log-level") ?? "info"),
    plugins: [uniflowed({ root, config }), ...userPlugins],
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
    preview: { host, port },
    build: {
      outDir: argument("--out-dir") ?? build.outDir ?? "dist",
      sourcemap: build.sourcemap ?? true,
      manifest: true,
      emptyOutDir: true,
    },
  };
}

async function dev() {
  const { createServer } = await import("vite");
  const config = await loadConfig();
  const server = await createServer(await viteConfig(config, "development"));
  await server.listen();
  const urls = server.resolvedUrls ?? { local: [], network: [] };
  emit("listening", {
    local: urls.local,
    network: urls.network,
    routes: scanRoutes(path.resolve(root, config.app?.router?.root ?? "app")).routes.map((route) => route.path),
  });

  const shutdown = async () => {
    await server.close();
    process.exit(0);
  };
  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

async function preview() {
  const { preview: startPreview } = await import("vite");
  const config = await loadConfig();
  const server = await startPreview(await viteConfig(config, "production"));
  const urls = server.resolvedUrls ?? { local: [], network: [] };
  emit("listening", { local: urls.local, network: urls.network, routes: [] });
  const shutdown = async () => {
    await server.close();
    process.exit(0);
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
  for (const url of pages) {
    const result = await server.render(url, assets);
    const file = htmlPathFor(outDir, url);
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, result.html);
    emit("page", { url, file: path.relative(root, file), status: result.status, bytes: Buffer.byteLength(result.html) });
  }
  if (server.notFound != null) {
    const result = await server.render("/__uf_not_found__", assets);
    const file = path.join(outDir, "404.html");
    writeFileSync(file, result.html);
    emit("page", { url: "/404", file: path.relative(root, file), status: 404, bytes: Buffer.byteLength(result.html) });
  }

  emit("done", { outDir: path.relative(root, outDir), pages: pages.length });
  process.exit(0);
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
 * Script, stylesheet and preload URLs for the client entry chunk.
 *
 * The entry is found by its `isEntry` flag rather than by key, because a
 * virtual module's manifest key is an implementation detail of the bundler.
 */
/**
 * The tags a prerendered document needs.
 *
 * Two walks over the manifest, because the two answers are different. A
 * `modulepreload` is worth emitting only for a chunk this document will
 * certainly load, which is the entry's *static* imports. A stylesheet has to
 * be emitted for anything the page might render, and the router loads every
 * route module dynamically — so a stylesheet imported by a layout is reached
 * through `dynamicImports` and through nothing else. Following only the static
 * graph, as this did, meant a layout could import a stylesheet and the built
 * HTML would silently ship without it.
 *
 * The cost is that a project with per-route stylesheets links all of them on
 * every page. Narrowing that needs the route table to say which chunk each
 * route came from, which the manifest alone cannot tell us.
 */
function assetsFromManifest(manifest) {
  const entry = Object.values(manifest).find((chunk) => chunk.isEntry);
  if (entry == null) throw new Error("uf: the client manifest has no entry chunk");

  const styles = new Set(entry.css ?? []);
  const seen = new Set();
  const collectStyles = (chunk) => {
    for (const imported of [...(chunk.imports ?? []), ...(chunk.dynamicImports ?? [])]) {
      if (seen.has(imported)) continue;
      seen.add(imported);
      const dependency = manifest[imported];
      if (dependency == null) continue;
      for (const css of dependency.css ?? []) styles.add(css);
      collectStyles(dependency);
    }
  };
  collectStyles(entry);

  const preloads = new Set();
  const collectPreloads = (chunk) => {
    for (const imported of chunk.imports ?? []) {
      const dependency = manifest[imported];
      if (dependency == null || preloads.has(dependency.file)) continue;
      preloads.add(dependency.file);
      collectPreloads(dependency);
    }
  };
  collectPreloads(entry);

  return {
    scripts: [`/${entry.file}`],
    styles: [...styles].map((file) => `/${file}`),
    preloads: [...preloads].map((file) => `/${file}`),
  };
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
        return Array.isArray(value) ? value.map(encodeURIComponent).join("/") : encodeURIComponent(String(value ?? ""));
      }
      if (segment.startsWith(":")) return encodeURIComponent(String(params[segment.slice(1)] ?? ""));
      return segment;
    })
    .join("/");
}

function htmlPathFor(outDir, url) {
  const pathname = url.split("?")[0].replace(/^\/+/, "");
  return pathname === "" ? path.join(outDir, "index.html") : path.join(outDir, pathname, "index.html");
}
