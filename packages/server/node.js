// @flow
//
// `@uniflowed/server/node`: the Node half of serving a build.
//
// [`../fetch.js`] answers the application's half of a request and touches no
// filesystem, because that is the half a worker runs. This is everything that
// is left, and all of it is host-specific: reading a file out of a directory,
// translating between Node's request objects and the platform's, and taking a
// socket.
//
// Three things use it, and the point of it being one module is that they
// cannot answer differently:
//
//   * `uf start`, through `@uniflowed/vite`'s `internal/serve.js`;
//   * `uf preview`, through the same;
//   * the `server.js` that `uf build --adapter node` writes, which imports
//     this directly — a deployed application must not link the package named
//     after the bundler, and before this module existed the only copy of the
//     static half was inside `@uniflowed/vite`.
//
// The exception is `./standalone.js`, which serves from bytes it carries
// instead of from a directory; its header says why it is a second
// implementation rather than a caller of this one.
//
// # Who owns the request
//
// This module does, for every host that reaches it: [`nodeListener`] begins
// the request, runs the whole of answering it inside `run`, and settles it on
// the line after the last byte — after its own 500, when it wrote one. That is
// what `after()` promises and it is not something a `Request` → `Response`
// handler can promise for itself, because such a handler has a `Response` in
// hand and not a response on the wire.
//
// `beginRequest` is passed in rather than imported from `./internal/context.js`
// beside this file, and that is the one thing about this module that looks
// wrong and is not. The request lives in an `AsyncLocalStorage` belonging to a
// module *instance*, and the instance the application reads is the one bundled
// into `.uf/build/server/server.js` — not the one this file resolves from the
// host's `node_modules`. A host that began a request in the wrong storage
// would fail silently: the guard would run, the page would render, and every
// `cookies()` in it would throw as though no host had run at all. So the
// bundle hands it out, `@uniflowed/router/server` re-exports it, and a caller
// passes it here. See ubugeeei-prod/uf#389.
//
// # The order is Vite's
//
// Static files first, then the application. That is a compatibility
// requirement rather than a preference: Vite's preview server runs its own
// file middleware before anything mounted behind it can see a request, so
// `uf preview` serves a file first whether or not anything here agrees — and a
// deployment that disagreed would mean a project whose handler path collides
// with a file in `public/` behaves one way when it is checked and the other
// way when it is deployed.

import { createReadStream } from "node:fs";
import { realpath, stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";
import { Readable } from "node:stream";

import type { CapabilityOptions, ServerCapabilities } from "./internal/capabilities.js";
import { assertCapable, capabilitiesFor } from "./internal/capabilities.js";
import type { RequestLifecycle } from "./internal/context.js";

export type { RequestLifecycle } from "./internal/context.js";

/**
 * What a Node host can do, plus whatever the deployment supplied.
 *
 * Both flags are `true` and neither is a formality. A Node response is a
 * socket, so a body reaches the client as it is written — which is what an
 * event stream needs. And the process is still there once the response has
 * gone, which is what makes an in-process queue a legitimate small deployment
 * here and nowhere else in this package.
 *
 * `websocket` is still the deployment's to pass. Node has no server-side
 * `WebSocket`: taking one means framing it over the raw socket from
 * `server.on("upgrade")`, and which library does that is a choice uf must not
 * make on a project's behalf — see `./socket.js` and `docs/red-lines.md` rule 3.
 */
export function nodeCapabilities(options?: CapabilityOptions): ServerCapabilities {
  return assertCapable(capabilitiesFor("node", { stream: true, persistent: true }, options));
}

/**
 * Content types for what a uf build emits.
 *
 * A closed table rather than a dependency, and deliberately short: every entry
 * is an extension `uf build` actually writes or a project actually puts in
 * `public/`. Anything else is `application/octet-stream`, which a browser
 * downloads rather than executes — the safe answer for a file whose type we do
 * not know, and the reason this is not a guess based on the bytes.
 */
const CONTENT_TYPES: { readonly [string]: string } = Object.freeze({
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
 * The pieces of a Node request this module touches.
 *
 * Declared structurally rather than imported from a `node:http` libdef, for
 * the same reason `./standalone.js` does it: the set is small, and naming it
 * here is what lets the file be read without knowing which host's types are in
 * scope. `originalUrl` is Connect's, and it is here because `uf preview` runs
 * this behind Vite's middleware stack, which sets it.
 */
export type NodeRequest = {
  readonly method?: string,
  readonly url?: string,
  readonly originalUrl?: string,
  readonly headers: { readonly [string]: string | Array<string> | void },
  ...
};

/** The pieces of a Node response this module writes. */
export type NodeResponse = {
  statusCode: number,
  statusMessage: string,
  headersSent: boolean,
  setHeader(name: string, value: string): mixed,
  write(chunk: Uint8Array | string): boolean,
  end(chunk?: Uint8Array | string): mixed,
  destroy(error?: mixed): mixed,
  // `send` paces itself against the socket and stops when the client hangs
  // up, so it needs the events as well as the writes. `write` returns a
  // `boolean` for the same reason — `mixed` would have made the back-pressure
  // check unwritable, which is one way this was lost.
  on(event: string, listener: () => mixed): mixed,
  once(event: string, listener: () => mixed): mixed,
  off(event: string, listener: () => mixed): mixed,
  ...
};

/**
 * A Node request as a `Request`.
 *
 * The body is passed as a stream where the host allows it, so a handler that
 * accepts an upload does not need the whole thing buffered before it starts.
 * `duplex` is required by the specification whenever a body is a stream, and
 * Node throws without it.
 */
export function toRequest(
  incoming: NodeRequest,
  options?: {| readonly secure?: boolean |},
): Request {
  const host = incoming.headers.host;
  const authority = typeof host === "string" && host !== "" ? host : "localhost";
  const protocol = options?.secure === true ? "https" : "http";
  const url = new URL(incoming.originalUrl ?? incoming.url ?? "/", `${protocol}://${authority}`);

  const headers = new Headers();
  for (const name of Object.keys(incoming.headers)) {
    const value = incoming.headers[name];
    if (value == null) continue;
    for (const entry of Array.isArray(value) ? value : [value]) {
      headers.append(name, entry);
    }
  }

  const method = (incoming.method ?? "GET").toUpperCase();
  const init: { [string]: mixed } = { method, headers };
  if (method !== "GET" && method !== "HEAD") {
    init.body = incoming;
    init.duplex = "half";
  }
  // $FlowFixMe[incompatible-call] - `incoming` is a stream, which `Request` accepts.
  return new Request(url, init);
}

/** Write a `Response` to a Node response. */
export async function send(outgoing: NodeResponse, result: Response): Promise<void> {
  outgoing.statusCode = result.status;
  if (result.statusText !== "") {
    outgoing.statusMessage = result.statusText;
  }
  for (const [name, value] of result.headers) {
    outgoing.setHeader(name, value);
  }
  if (result.body == null) {
    outgoing.end();
    return;
  }
  // Streamed rather than buffered, so a handler returning a large or
  // open-ended body is not read into memory first.
  //
  // Which was only half true while this loop read as fast as the body would
  // give: `write` answers `false` when the kernel buffer is full and the rest
  // is being held in *this process's* memory, and a reader that ignores that
  // turns a slow client into a heap the size of everything it has not
  // acknowledged. Streaming in shape and buffering in fact — the same failure
  // `ChunkQueue` exists to avoid a layer up, in the renderer.
  const reader = result.body.getReader();
  // And a client that hangs up is the other half. Nothing written after that
  // goes anywhere, and the producer behind the body — a render, a proxied
  // upstream, an event stream — keeps producing for a reader that is never
  // coming back. `cancel()` is what tells it to stop; `releaseLock()` would
  // only detach this end.
  let open = true;
  const onClose = () => {
    open = false;
  };
  outgoing.on("close", onClose);
  try {
    while (open) {
      const { done, value } = await reader.read();
      if (done === true || !open) break;
      if (value != null && outgoing.write(value) === false) {
        await writable(outgoing);
      }
    }
  } finally {
    outgoing.off("close", onClose);
  }
  if (open) {
    outgoing.end();
    return;
  }
  // Best effort, and the only place in this function where a rejection is
  // dropped: the connection is already gone, so there is nobody left to report
  // to and no response left to fail.
  await reader.cancel().catch(() => {});
}

/**
 * Resolve once `outgoing` can take more — or once it cannot ever again.
 *
 * `drain` alone would be a deadlock waiting to happen: a client that hangs up
 * while the buffer is full emits `close` and never `drain`, and a writer
 * waiting only for the latter waits for the life of the process, holding the
 * body's producer open with it.
 */
function writable(outgoing: NodeResponse): Promise<void> {
  return new Promise((resolve) => {
    const settle = () => {
      outgoing.off("drain", settle);
      outgoing.off("close", settle);
      resolve();
    };
    outgoing.once("drain", settle);
    outgoing.once("close", settle);
  });
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
export function createStaticHandler(options: {|
  readonly root: string,
|}): (request: Request) => Promise<Response | null> {
  const rootDir = path.resolve(options.root);
  // Resolved once, lazily: the root is fixed for the life of the handler, and a
  // checkout reached through a symlink — which is every macOS `/tmp` — would
  // otherwise fail its own containment test. Lazily because this is a
  // constructor and it cannot await.
  let realRoot: string | null = null;

  return async function serveStatic(request: Request): Promise<Response | null> {
    const method = request.method.toUpperCase();
    if (method !== "GET" && method !== "HEAD") return null;

    const pathname = decodePathname(new URL(request.url).pathname);
    if (pathname == null) return null;

    const resolved = path.resolve(rootDir, `.${pathname}`);
    if (resolved !== rootDir && !resolved.startsWith(rootDir + path.sep)) return null;

    realRoot ??= (await realpathOrNull(rootDir)) ?? rootDir;
    const root = realRoot;

    // `/guide/` and `/guide` are the same prerendered document, and neither
    // spelling is the one a person types. `<path>.html` is last because a
    // build writes `guide/index.html`, and only a hand-placed file in
    // `public/` is ever `guide.html`.
    const candidates =
      pathname.endsWith("/") === true
        ? [path.join(resolved, "index.html")]
        : [resolved, path.join(resolved, "index.html"), `${resolved}.html`];

    for (const candidate of candidates) {
      const info = await statFile(candidate);
      if (info == null || !info.isFile()) continue;
      // The containment check above is textual, and a symlink is how a path
      // that reads as inside the root opens a file outside it. `dist/` is
      // written by the build, but `public/` is copied verbatim from whatever
      // the author — or a dependency's install script — put there, so the
      // check has to be made again on the value that is actually opened.
      // ubugeeei-prod/uf#550; the same shape as the tarball traversals in
      // `docs/security.md`, at the serving end.
      //
      // A link that stays inside the root still works, because that is a thing
      // people do on purpose. One that leaves is a 404, indistinguishable from
      // a file that is not there — which is what it should look like.
      const opened = await realpathOrNull(candidate);
      if (opened == null || (opened !== root && !opened.startsWith(root + path.sep))) {
        continue;
      }
      const headers = {
        "content-type":
          CONTENT_TYPES[path.extname(candidate).toLowerCase()] ?? "application/octet-stream",
        "content-length": String(info.size),
      };
      if (method === "HEAD") return new Response(null, { headers });
      // Streamed rather than read into memory, so serving a large asset costs
      // a buffer rather than the file.
      // $FlowFixMe[incompatible-call] - a Node web stream is a `BodyInit`.
      return new Response(Readable.toWeb(createReadStream(candidate)), { headers });
    }
    return null;
  };
}

function decodePathname(pathname: string): string | null {
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

/**
 * `fs.realpath`, or `null` when the path cannot be resolved.
 *
 * A broken symlink, a component that is not a directory, or a permission the
 * process does not have all end here — and all of them mean the same thing to
 * the caller: this is not a file to serve.
 */
async function realpathOrNull(file: string): Promise<string | null> {
  try {
    return await realpath(file);
  } catch {
    return null;
  }
}

async function statFile(file: string) {
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
 * than in every host that wants to run one.
 *
 * `beginRequest` is required, and it comes from the application bundle for the
 * reason in "Who owns the request" above. A listener built without one fails on
 * its first request, which is the same trade `createFetchHandler` makes about
 * `app.runMiddleware`: an optional lifecycle is a lifecycle somebody forgets,
 * and what is lost when they do is every `after()` in the application.
 *
 * A handler that throws is answered with a bare 500 and reported on stderr:
 * the body must not carry the stack, because the body goes to whoever asked,
 * and stderr is where the operator is already looking. The drain is in a
 * `finally` below the `catch`, so a middleware that logged the request sees its
 * callback run once that 500 is on the wire rather than once the handler gave
 * up — and a request that failed is still a request that happened, which is why
 * it is drained at all.
 */
export function nodeListener(
  handle: (request: Request) => Promise<Response>,
  options: {|
    readonly beginRequest: (request: Request) => RequestLifecycle,
    readonly secure?: boolean,
  |},
): (incoming: NodeRequest, outgoing: NodeResponse) => Promise<void> {
  return async function listener(incoming: NodeRequest, outgoing: NodeResponse): Promise<void> {
    // Declared out here because `toRequest` is inside the `try`: a request that
    // could not even be built has no lifecycle to settle.
    let lifecycle: RequestLifecycle | null = null;
    try {
      const request = toRequest(incoming, options);
      lifecycle = options.beginRequest(request);
      await lifecycle.run(async () => {
        await send(outgoing, await handle(request));
      });
    } catch (error) {
      console.error(error);
      if (outgoing.headersSent) {
        outgoing.destroy();
      } else {
        outgoing.statusCode = 500;
        outgoing.setHeader("content-type", "text/plain; charset=utf-8");
        outgoing.end("500 Internal Server Error\n");
      }
    } finally {
      if (lifecycle != null) await lifecycle.settle();
    }
  };
}

/**
 * Static files, then the application: the whole of what a built uf app serves.
 *
 * `staticDir` is the directory `uf build` wrote — `dist/` in a checkout, and
 * the `static/` copied beside `server.js` in an adapter's output.
 */
export function createServeHandler(options: {|
  readonly staticDir: string,
  readonly handle: (request: Request) => Promise<Response>,
|}): (request: Request) => Promise<Response> {
  const serveStatic = createStaticHandler({ root: options.staticDir });
  return async function handle(request: Request): Promise<Response> {
    return (await serveStatic(request)) ?? (await options.handle(request));
  };
}

/**
 * Serve the application until the process is stopped.
 *
 * What `uf build --adapter node` writes calls this and nothing else. It
 * resolves once the socket is listening, with the address it took, because a
 * caller that asked for port 0 has no other way to learn which port it got —
 * and because a test that has to drive a deployed directory needs exactly
 * that.
 *
 * `PORT` and `HOST` are read from the environment because that is how every
 * process manager and container platform says which socket to take, and a
 * production server that could only be told on the command line would need a
 * wrapper script everywhere it ran. The command line wins over both, and the
 * default address is every interface: a container that bound loopback would be
 * a container nothing outside it can reach.
 */
export async function serve(options: {|
  readonly staticDir: string,
  readonly handle: (request: Request) => Promise<Response>,
  /**
   * The application bundle's own `beginRequest`.
   *
   * The generated `handler.js` re-exports it beside `fetch` so that
   * `server.js` has one to pass; see "Who owns the request" above for why it
   * cannot be imported here instead.
   */
  readonly beginRequest: (request: Request) => RequestLifecycle,
  readonly host?: string,
  readonly port?: number,
|}): Promise<{|
  readonly host: string,
  readonly port: number,
  readonly close: () => Promise<void>,
|}> {
  const listener = nodeListener(
    createServeHandler({ staticDir: options.staticDir, handle: options.handle }),
    { beginRequest: options.beginRequest },
  );
  const server = createServer((request, response) => {
    void listener(request, response);
  });

  const host = options.host ?? argument("--host") ?? process.env.HOST ?? "0.0.0.0";
  const port = options.port ?? Number(argument("--port") ?? process.env.PORT ?? 3000);

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, host, resolve);
  });

  const address = server.address();
  const bound = typeof address === "object" && address != null ? address.port : port;
  // `0.0.0.0` is not a URL anybody can open, so the loopback spelling is what
  // is printed — the same split `uf dev` and `uf start` print, and for the
  // same reason: one of the two is a link and the other is a fact about the
  // socket.
  const shown = host === "0.0.0.0" || host === "::" ? "localhost" : host;
  process.stdout.write(`uf: listening on http://${shown}:${String(bound)}\n`);

  return {
    host,
    port: bound,
    close: () =>
      new Promise((resolve) => {
        server.close(() => resolve());
      }),
  };
}

/** The value of a `--flag value` pair on the command line, if it is there. */
function argument(name: string): string | null {
  const at = process.argv.indexOf(name);
  return at === -1 ? null : (process.argv[at + 1] ?? null);
}
