// @noflow
//
// Plain JavaScript: executed by the host that serves a build.
//
// Serving what `uf build` wrote — one request handler, behind two front doors.
//
// `uf build` writes three things: a client bundle and prerendered HTML in
// `dist/`, and a server bundle in `.uf/build/server/server.js` that exports
// `render`, `dispatch`, `runMiddleware`, `routes`, `middleware`, `notFound`
// and `errors`. This module finds them, reads the client manifest, and hands
// both to the handler `uf preview` and `uf start` mount.
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
// # Where the answering actually happens
//
// Not here, any more. Every decision about *what* a request is answered with
// lives in `@uniflowed/server` — `@uniflowed/server/fetch` for the application
// half and `@uniflowed/server/node` for the files and the socket — and this
// module is the part that is genuinely Vite's: finding the build on disk and
// reading the manifest a Vite build wrote.
//
// It moved because of `uf build --adapter`. `tests/library/serve.test.js` said
// what was wrong with the old arrangement while it was still the only one:
// "`internal/serve.js` is the seam a deploy adapter will need, and naming it
// in `exports` before one exists would be promising an interface nothing has
// used yet." An adapter exists now, and it may not import this package —
// `@uniflowed/vite` is the bundler, and the whole claim of deployable output
// is that the host needs neither the bundler nor the toolchain. So the seam is
// a package export of `@uniflowed/server`, and `uf preview`, `uf start` and
// every adapter now answer out of one implementation instead of copies that
// agree until they do not.
//
// # Why those imports are dynamic
//
// `@uniflowed/server` is Flow, and `driver.js` registers the loader hooks that
// make Flow importable *in its body* — after every static import in this graph
// has already been evaluated. So they are reached the same way the server
// bundle is: with `await import`, from [`loadBuild`], which is the point at
// which this process stops being plain JavaScript and starts being the
// project's.
//
// # Who owns the request
//
// The host does, and none of the three handlers below: each of them has a
// `Response` in hand rather than a response on the wire, and what `after()`
// promises is the wire. [`withRequest`] is the shape for a caller that writes
// into a Node response itself — `uf dev` and `uf preview` — and
// `@uniflowed/server/node`'s `nodeListener` does the same thing for `uf start`
// and for the `server.js` an adapter writes. Each of them begins the request
// with `entry.beginRequest`, runs the whole of answering it inside `run`, and
// settles it on the line after the last byte.
//
// It has to be the *entry's* `beginRequest` rather than one imported here: the
// request lives in an `AsyncLocalStorage` belonging to one copy of
// `@uniflowed/server`, and the copy that matters is the one inside the
// application bundle. A host that resolved its own would begin a request the
// application cannot see, and nothing would fail loudly — the guard would run,
// the page would render, and every `cookies()` in it would throw as though no
// host had run at all. See ubugeeei-prod/uf#389.

import { readFile, stat } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

/**
 * `@uniflowed/server`'s two halves, loaded once.
 *
 * Cached as the promise rather than the modules, so two concurrent callers
 * share one import rather than racing to start two.
 */
let deploymentModules = null;
function deployment() {
  deploymentModules ??= Promise.all([
    import("@uniflowed/server/fetch"),
    import("@uniflowed/server/node"),
    import("@uniflowed/server/cache"),
  ]).then(([application, host, cache]) => ({ ...application, ...host, ...cache }));
  return deploymentModules;
}

/**
 * The cache `rendering.cache` describes, or `undefined` for no cache at all.
 *
 * `undefined` rather than a store with both switches off, and the difference is
 * visible from an application: a handler with no `cache` installs nothing on
 * the request, so `revalidateTag()` raises "there is not one here" instead of
 * reporting that it expired nothing. A project that turned the cache off should
 * be told that it did, not handed a cache that quietly does nothing.
 *
 * One store per server process, built when the handler is, which is the whole
 * of what "in memory, per process" means in practice: `uf preview` and
 * `uf start` each hold one, and two of them running at once share nothing.
 *
 * The store's own options are not configurable from `uf.config.js` and are not
 * named here: `rendering.cache` has four keys and no fifth, and a parameter
 * threaded through for a setting nobody can set would be the shape of
 * configurability with none of the substance.
 *
 * @param {{route?: boolean, fetch?: boolean} | undefined} declared
 * @param {(options?: object) => object} createCacheStore
 */
function cacheFor(declared, createCacheStore) {
  const route = declared?.route === true;
  const fetchCache = declared?.fetch === true;
  if (!route && !fetchCache) return undefined;
  return { store: createCacheStore(), route, fetch: fetchCache };
}

/**
 * Everything a served build consists of.
 *
 * Read once at startup rather than per request: the manifest does not change
 * while the server runs, and importing the server bundle again per request
 * would re-evaluate every module in the application.
 *
 * `@uniflowed/server` is loaded here too, and not lazily on the first request:
 * a missing or broken install should fail the command that starts the server,
 * with the message the import raises, rather than a minute later inside
 * whichever request happened to arrive first.
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
  await deployment();
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
 *
 * This is the one piece of serving a build that is genuinely Vite's — a Vite
 * manifest, read the way Vite writes it — which is why it stayed behind when
 * the rest moved to `@uniflowed/server`. `uf build --adapter` calls it too,
 * at build time, and bakes the answer into what it emits.
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
 * Answer one request inside it, and settle it when the answer has been written.
 *
 * `body` is everything that decides the response *and writes it*; this is the
 * line after. `settle` is in a `finally` because a request that failed is
 * still a request that happened: a middleware that logged the arrival is owed
 * its callback whether the render threw or not, and `drainDeferred` already
 * reports a failing task rather than propagating it.
 *
 * `entry.beginRequest` and not an import: the request lives in an
 * `AsyncLocalStorage` belonging to one copy of `@uniflowed/server`, and the
 * copy that matters is the one inside the application bundle. See
 * `serverModuleSource` in `./routes.js`.
 *
 * The one case this cannot be exact about is a request uf hands back rather
 * than answers: a caller whose `catch` is `next(error)` gives the response to
 * Vite's chain, which writes a 500 at a moment nothing here can observe, so
 * such a request settles when uf lets go of it. `nodeListener` and the
 * compiled binary write their own failures and settle after them. It is worth
 * naming rather than papering over, and it is the failure path of a request
 * that already went wrong — not the ordinary one this exists for.
 *
 * @param {{beginRequest: (request: Request) => {context: object, run: <T>(body: () => Promise<T>) => Promise<T>, settle: () => Promise<void>}}} entry
 * @param {Request} request
 * @param {() => Promise<mixed>} body
 */
export async function withRequest(entry, request, body) {
  const lifecycle = entry.beginRequest(request);
  const { run, settle } = lifecycle;
  // What this host can do, put on the request the way `createFetchHandler`
  // puts it on the one it owns. `uf dev` and `uf build --compile` reach a
  // route handler without going through that function, and a handler that
  // streams events or queues work has to get the same answer from all four
  // front doors — a capability that is present under `uf start` and absent
  // under `uf dev` is the difference this whole seam exists to remove.
  //
  // `nodeCapabilities`, because both of those *are* a Node process with a
  // socket: a body reaches the client as it is written, and the process is
  // still there afterwards. Neither passes an upgrader or a queue, because uf
  // defines both and implements neither.
  const { nodeCapabilities } = await deployment();
  lifecycle.context.capabilities ??= nodeCapabilities();
  try {
    return await run(body);
  } finally {
    await settle();
  }
}

/**
 * The application half: route handlers, then rendering.
 *
 * `@uniflowed/server/fetch`'s `createFetchHandler`, reached through the
 * dynamic import above. Kept as a function here — rather than making every
 * caller await the module — because the two servers construct their handler
 * before they take a socket, and an `await` in that position would put the
 * import between the port and the first request rather than before both.
 *
 * It must be called inside a request its caller began; it begins none, because
 * it has a `Response` in hand and not a response on the wire. A caller that
 * forgets is not left to discover it: `entry.runMiddleware` refuses outside a
 * request and names what establishes one. See "Who owns the request" above.
 *
 * `cache` is `rendering.cache` from `uf.config.js`, straight through: this is
 * the point where four switches that used to reach a JSON file and nothing else
 * become a store a request can hit. See ubugeeei-prod/uf#277.
 *
 * @param {{entry: object, assets: object, cache?: object}} build
 */
export function createApplicationHandler({ entry, assets, cache }) {
  const ready = deployment().then(({ createFetchHandler, createCacheStore, nodeCapabilities }) =>
    createFetchHandler({
      app: entry,
      document: assets,
      cache: cacheFor(cache, createCacheStore),
      // `uf preview` and `uf start` are a Node process with a socket, which is
      // what a deployed `--adapter node` build is too — so a route handler
      // that streams events answers the same way in the preview it is checked
      // in and in the deployment it ends up as. See `withRequest` above.
      capabilities: nodeCapabilities(),
    }),
  );
  return async function handle(request) {
    return (await ready)(request);
  };
}

/**
 * The static half: a file under `root`, or `null` for the caller to carry on.
 *
 * `@uniflowed/server/node`'s, for the same reason as above: what a deployment
 * runs and what `uf start` runs have to be the same code, not the same idea.
 */
export function createStaticHandler({ root }) {
  const ready = deployment().then(({ createStaticHandler: create }) => create({ root }));
  return async function serveStatic(request) {
    return (await ready)(request);
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
 * @param {{entry: object, assets: object, distDir: string, cache?: object}} build
 */
export function createServeHandler({ entry, assets, distDir, cache }) {
  const serveStatic = createStaticHandler({ root: distDir });
  const application = createApplicationHandler({ entry, assets, cache });
  return async function handle(request) {
    return (await serveStatic(request)) ?? (await application(request));
  };
}

/**
 * A `Request`/`Response` handler as a Node request listener.
 *
 * `@uniflowed/server/node`'s, which is also what the `server.js` an adapter
 * writes runs — so a request reaching `uf start` and the same request reaching
 * a deployed directory go through one translation rather than two, and settle
 * at one moment rather than at two.
 *
 * `entry` is the second argument rather than something this reaches for: it is
 * the application bundle's own `beginRequest` that has to own the request, for
 * the reason in "Who owns the request" above. It is required, and a listener
 * built without one fails on its first request — the same trade
 * `createFetchHandler` makes about `app.runMiddleware`, and for the same
 * reason: an optional lifecycle is a lifecycle somebody forgets, and what is
 * lost when they do is every `after()` in the application.
 */
export function nodeListener(handle, entry) {
  const ready = deployment().then(({ nodeListener: create }) =>
    create(handle, { beginRequest: entry.beginRequest }),
  );
  return async function listener(incoming, outgoing) {
    return (await ready)(incoming, outgoing);
  };
}
