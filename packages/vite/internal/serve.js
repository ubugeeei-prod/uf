// @noflow
//
// Plain JavaScript: executed by the host that serves a build.
//
// Serving what `uf build` wrote — one request handler, behind two front doors.
//
// `uf build` writes three things and, until this module existed, nothing could
// answer a request with any of them: a client bundle and prerendered HTML in
// `dist/`, and a server bundle in `.uf/build/server/server.js` that exports
// `render`, `dispatch`, `runMiddleware`, `routes`, `middleware`, `notFound`
// and `errors`. The prerendered half could be put on a static host; the other
// half could only be reached from `uf dev`, so a route handler, a middleware
// and a route with parameters and no `generateStaticParams` worked in
// development and did not exist in a build.
//
// `uf preview` and `uf start` are the two front doors, and they share
// everything below on purpose. A preview whose answers differ from the
// production server's is worse than no preview, because it is checked and
// believed. The difference between the two commands is which socket the
// handler is bolted to, not what it decides:
//
//   preview — Vite's own preview server, with this handler behind its static
//             middleware, so `vite.preview.proxy`, `vite.preview.https`,
//             `headers` and `cors` are in effect and what is being checked is
//             the build *as Vite serves it*.
//   start   — `node:http`, with no bundler in the process, because a host
//             running a production build should not need Vite installed to
//             answer a request.
//
// # Why this is JavaScript
//
// The rest of uf's hot paths are Rust, and this one deliberately is not: it
// runs in the deployed application rather than in the build, and
// `ubugeeei-redundancy.md` is explicit that a deployment must not inherit a
// native dependency from the toolchain that produced it. An edge or serverless
// target that cannot run a Rust binary still has to be able to run this.
//
// # The seam the adapters need
//
// [`createApplicationHandler`] takes a `Request` and returns a `Response` and
// touches no filesystem, so it is the part that ports to a worker unchanged.
// [`createStaticHandler`] reads files and is therefore host-specific, which is
// exactly the split a deploy adapter has to make: on a CDN-backed target the
// static half is not the application's job at all.

import { createReadStream } from "node:fs";
import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import { Readable } from "node:stream";
import { pathToFileURL } from "node:url";

// The Node-to-platform translation lives in one module and is re-exported
// here, because `nodeListener` needs it and every host that serves a build
// reaches for it through this one. Two copies is how the dev server and the
// production server would come to disagree about a request.
import { send, toRequest } from "./http.js";

export { send, toRequest };

/**
 * Content types for what a uf build emits.
 *
 * A closed table rather than a dependency, and deliberately short: every entry
 * is an extension `uf build` actually writes or a project actually puts in
 * `public/`. Anything else is `application/octet-stream`, which a browser
 * downloads rather than executes — the safe answer for a file whose type we do
 * not know, and the reason this is not a guess based on the bytes.
 */
const CONTENT_TYPES = Object.freeze({
  ".avif": "image/avif",
  ".css": "text/css; charset=utf-8",
  ".gif": "image/gif",
  ".html": "text/html; charset=utf-8",
  ".ico": "image/x-icon",
  ".jpeg": "image/jpeg",
  ".jpg": "image/jpeg",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".map": "application/json; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".txt": "text/plain; charset=utf-8",
  ".webmanifest": "application/manifest+json",
  ".webp": "image/webp",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".xml": "application/xml; charset=utf-8",
});

/**
 * Everything a served build consists of.
 *
 * Read once at startup rather than per request: the manifest does not change
 * while the server runs, and importing the server bundle again per request
 * would re-evaluate every module in the application.
 *
 * @param {{root: string, outDir: string, serverDir: string}} build
 */
export async function loadBuild({ root, outDir, serverDir }) {
  const distDir = path.resolve(root, outDir);
  const entryFile = path.join(path.resolve(root, serverDir), "server.js");

  // Named separately, because the two failures have different fixes and a
  // combined "run uf build" would be wrong for one of them: a `dist/` with no
  // server bundle beside it is what a `.uf/` that was cleaned looks like.
  await readable(entryFile, `the server bundle is missing at ${entryFile}`);
  const manifestFile = path.join(distDir, ".vite", "manifest.json");
  await readable(manifestFile, `the client manifest is missing at ${manifestFile}`);

  const manifest = JSON.parse(await readFile(manifestFile, "utf8"));
  const entry = await import(pathToFileURL(entryFile).href);
  return { entry, assets: assetsFromManifest(manifest), distDir };
}

async function readable(file, message) {
  try {
    await stat(file);
  } catch {
    throw new Error(`uf: ${message}; run \`uf build\` first`);
  }
}

/**
 * The tags a rendered document needs, from the client build's manifest.
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
 *
 * The entry is found by its `isEntry` flag rather than by key, because a
 * virtual module's manifest key is an implementation detail of the bundler.
 */
export function assetsFromManifest(manifest) {
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
 * The application half: route handlers, then rendering.
 *
 * Touches no filesystem and holds no Node types, so this is the function a
 * deploy adapter for a worker or a serverless function wraps. Returns `null`
 * for nothing, ever — a request that matches no handler and no route is a
 * rendered 404, because the renderer is what knows what the project's
 * `_uf.not-found` page says.
 *
 * The order is the dev server's, and has to stay the dev server's: middleware
 * first, then handlers for every method, because a handler is the only thing
 * that can answer a `POST` and it may also answer a `GET` for a path that has
 * no page. A page cannot answer a `POST`, so a non-navigation that no handler
 * claimed is a 404 rather than a rendered page with a 200.
 *
 * Middleware above both, and not inside either: it guards a path, so it has to
 * run for a page, for a route handler, and for a path under it that matches
 * neither — `/dashboard/typo` is a 404 that the guard on `/dashboard` still
 * answers. `entry.runMiddleware` is called rather than tested for, so a server
 * bundle without it is a `TypeError` on the first request instead of an
 * application whose auth check quietly stopped running once it was built.
 * That is the whole of ubugeeei-prod/uf#260, and `uf preview` and `uf start`
 * are two more places it could have happened.
 *
 * @param {{entry: object, assets: object}} build
 */
export function createApplicationHandler({ entry, assets }) {
  return async function handle(request) {
    const guarded = await entry.runMiddleware(request);
    if (guarded != null) return guarded;

    const handled = await entry.dispatch(request);
    if (handled != null) return handled;

    const method = request.method.toUpperCase();
    if (method !== "GET" && method !== "HEAD") {
      return new Response(null, { status: 404 });
    }

    const url = new URL(request.url);
    const result = await entry.render(url.pathname + url.search, assets, {
      // Nothing better than the console here: this is the handler a worker or a
      // serverless function wraps, and it has no terminal of its own. Losing a
      // boundary's exception entirely would be worse — it is the only trace a
      // page that failed after its first byte leaves anywhere.
      onError: (error) => {
        console.error(error);
      },
    });
    const headers = new Headers(result.headers ?? {});
    headers.set("content-type", "text/html; charset=utf-8");
    // A `HEAD` gets the status and the headers and no body, which is what the
    // renderer cannot know to do for itself. The stream is cancelled rather
    // than dropped, so the render behind it stops instead of filling its queue
    // and waiting for a reader that is never coming.
    if (method === "HEAD") {
      await result.stream().cancel();
      return new Response(null, { status: result.status ?? 200, headers });
    }
    // The body is a stream, so the layouts and any `<Suspense>` fallback reach
    // the browser while the page they surround is still resolving.
    return new Response(result.stream(), { status: result.status ?? 200, headers });
  };
}

/**
 * The static half: a file under `root`, or `null` for the caller to carry on.
 *
 * `GET` and `HEAD` only. A `POST` to a path that happens to have a file under
 * it belongs to a route handler, and answering it with the file's bytes would
 * be the same mistake as rendering a page for it.
 *
 * # The path is checked once, after it is resolved
 *
 * `docs/security.md` rule 2: never authorize against a raw request string or a
 * partially decoded path. The pathname is decoded first, then resolved against
 * the root, and *then* checked to be inside it — so `%2e%2e%2f`, a backslash
 * on Windows, and a symlinked directory all reduce to the same question, asked
 * once, of the value that is actually opened.
 */
export function createStaticHandler({ root }) {
  const distDir = path.resolve(root);

  return async function serveStatic(request) {
    const method = request.method.toUpperCase();
    if (method !== "GET" && method !== "HEAD") return null;

    const pathname = decodePathname(new URL(request.url).pathname);
    if (pathname == null) return null;

    const resolved = path.resolve(distDir, `.${pathname}`);
    if (resolved !== distDir && !resolved.startsWith(distDir + path.sep)) return null;

    // `/guide/` and `/guide` are the same prerendered document, and neither
    // spelling is the one a person types. `<path>.html` is last because a
    // build writes `guide/index.html`, and only a hand-placed file in
    // `public/` is ever `guide.html`.
    const candidates = pathname.endsWith("/")
      ? [path.join(resolved, "index.html")]
      : [resolved, path.join(resolved, "index.html"), `${resolved}.html`];

    for (const candidate of candidates) {
      const info = await statFile(candidate);
      if (info == null || !info.isFile()) continue;
      const headers = {
        "content-type":
          CONTENT_TYPES[path.extname(candidate).toLowerCase()] ?? "application/octet-stream",
        "content-length": String(info.size),
      };
      if (method === "HEAD") return new Response(null, { headers });
      // Streamed rather than read into memory, so serving a large asset costs
      // a buffer rather than the file.
      return new Response(Readable.toWeb(createReadStream(candidate)), { headers });
    }
    return null;
  };
}

/**
 * Static files, then the application: the whole of what a built uf app serves.
 *
 * Static first, and that ordering is a compatibility requirement rather than a
 * preference. Vite's preview server runs its own file middleware before
 * anything added afterwards can see the request, so `uf preview` serves a file
 * first whether or not this agrees — and `uf start` disagreeing would mean a
 * project whose handler path collides with a file in `public/` behaves one way
 * when it is checked and the other way when it is deployed.
 *
 * @param {{entry: object, assets: object, distDir: string}} build
 */
export function createServeHandler({ entry, assets, distDir }) {
  const serveStatic = createStaticHandler({ root: distDir });
  const application = createApplicationHandler({ entry, assets });
  return async function handle(request) {
    return (await serveStatic(request)) ?? (await application(request));
  };
}

function decodePathname(pathname) {
  try {
    const decoded = decodeURIComponent(pathname);
    // A NUL truncates the name every C-level `open` sees, so a path holding
    // one is refused rather than normalised into something shorter.
    return decoded.includes("\0") ? null : decoded;
  } catch {
    // A percent escape that is not one. There is no file behind it.
    return null;
  }
}

async function statFile(file) {
  try {
    return await stat(file);
  } catch {
    return null;
  }
}

/**
 * A `Request`/`Response` handler as a Node request listener.
 *
 * The handler contract is the platform's, so this adapter belongs here rather
 * than in every host that wants to run one — `uf dev`'s middleware, `uf
 * preview`'s, and `uf start`'s own server all reach for the same two halves.
 *
 * A handler that throws is answered with a bare 500 and reported on stderr:
 * the body must not carry the stack, because the body goes to whoever asked,
 * and stderr is where the operator is already looking. It is not an event on
 * stdout because a request failing is the application's news, not the driver's
 * — the driver's stdout says what the *server* is doing.
 */
export function nodeListener(handle) {
  return async function listener(incoming, outgoing) {
    try {
      await send(outgoing, await handle(await toRequest(incoming)));
    } catch (error) {
      console.error(error);
      if (outgoing.headersSent) {
        outgoing.destroy();
        return;
      }
      outgoing.statusCode = 500;
      outgoing.setHeader("content-type", "text/plain; charset=utf-8");
      outgoing.end("500 Internal Server Error\n");
    }
  };
}
