// @flow
//
// `@uniflowed/server/standalone`: the application, serving itself, from one file.
//
// `uf build --compile` links this module with the project's server bundle and
// with an embedded copy of everything `uf build` wrote to `dist/`, then hands
// the result to a JavaScript runtime that appends itself to it. What comes out
// is a file that can be copied into an empty directory and run: no Node
// installation, no `node_modules`, no `dist/` beside it.
//
// # Why `node:http`, and not the host's own server
//
// Bun has `Bun.serve` and Deno has `Deno.serve`, and both are faster than the
// interface they emulate. Neither is a standard. Writing this file against
// either would make it a Bun file or a Deno file, and the runtime that gets
// embedded would stop being a decision `uf build --compile` makes and start
// being a decision this module already made. `node:http` is the one server
// interface all three hosts implement, so it is the one that leaves the choice
// open — which matters more here than the throughput of a shim that spends
// almost all of its time inside React.
//
// # Why this is not `@uniflowed/vite`'s `internal/serve.js`
//
// That module is the handler behind `uf preview` and `uf start`, and it is the
// obvious thing to import rather than write a second one. It is the wrong
// thing to import, for two reasons and either would be enough.
//
// It answers by opening files under `dist/`, and there is no `dist/` here —
// the whole claim of a compiled binary is that it was copied into an empty
// directory. Its static half is therefore not shareable at all, and its
// application half arrives attached to it. And it lives in `@uniflowed/vite`,
// so importing it would link the package named after the bundler into the
// artefact a deployment runs, which is the property `uf start` exists to
// establish and the one a single file makes strongest.
//
// So the code is not shared and the *answer* is. Both resolve a request in the
// same order — a file the build already wrote, then a route handler for any
// method, then a render for whatever is left — and that order is not a
// preference either module gets to hold: `uf preview` is Vite's own server,
// which runs its file middleware before anything uf mounts behind it, so
// `internal/serve.js` matches Vite and this matches `internal/serve.js`. A
// binary that resolved a page/handler collision the other way would behave
// one way when it was checked with `uf preview` and another way once it was
// deployed, which is the trap `uf preview` exists to prevent.
//
// Both copies are driven by a test: `serve.test.js` and
// `preview_and_start_serve_the_whole_of_a_build` for that one,
// `tests/library/standalone.test.js` and
// `compile_writes_one_file_that_serves_the_site_from_an_empty_directory` for
// this one.

import { Buffer } from "node:buffer";
import { createServer } from "node:http";

/**
 * The pieces of a Node request and response this module touches.
 *
 * Declared structurally rather than imported from a `node:http` libdef,
 * because the set is five members wide and naming it here is what lets the
 * same file be read without knowing which host's types are in scope.
 */
type NodeRequest = {
  readonly method?: string,
  readonly url?: string,
  readonly headers: { readonly [string]: string | Array<string> | void },
  ...
};

type NodeResponse = {
  statusCode: number,
  setHeader(name: string, value: string): mixed,
  write(chunk: Uint8Array | string): mixed,
  end(chunk?: Uint8Array | string): mixed,
  // Required rather than optional, because the one case it exists for is the
  // one where nothing else will do: a render that fails after the shell has
  // gone out cannot be answered with a status, and dropping the socket is the
  // only way left to tell the client the document it received is not whole.
  destroy(error?: mixed): mixed,
  // The events a writer has to listen to rather than assume: `drain`, so a body
  // is paced by what the socket will take, and `close`, so a client that hung
  // up stops the producer instead of being written at. Named individually, like
  // `stream.js`'s `NodeDestination`, so that a host missing one of them fails
  // to compile rather than to serve.
  on(event: string, listener: (...args: Array<mixed>) => mixed): mixed,
  once(event: string, listener: (...args: Array<mixed>) => mixed): mixed,
  off(event: string, listener: (...args: Array<mixed>) => mixed): mixed,
  ...
};

/** One file from `dist/`, as `uf build --compile` embedded it. */
export type EmbeddedAsset = {|
  /** The `content-type` to serve it with, decided at build time. */
  readonly type: string,
  /** The file's bytes, base64. */
  readonly body: string,
|};

/** Every embedded file, keyed by its path relative to the output directory. */
export type EmbeddedAssets = { readonly [path: string]: EmbeddedAsset };

/** The script, stylesheet and preload URLs a rendered document references. */
export type DocumentAssets = {|
  readonly scripts: $ReadOnlyArray<string>,
  readonly styles: $ReadOnlyArray<string>,
  readonly preloads: $ReadOnlyArray<string>,
|};

/** What the project's server bundle exports; see `virtual:uf/server`. */
export type StandaloneApp = {|
  /**
   * Render `url`, resolving when the *shell* is ready.
   *
   * The same `{ status, headers?, pipe }` the router hands `uf start` and
   * every adapter — not a finished string. A binary that collected the whole
   * document before answering would be the one deployment target that does not
   * stream, and the reason `renderToString` was replaced is that the wait is
   * the slowest thing on the page.
   */
  readonly render: (
    url: string,
    assets: DocumentAssets,
    options?: {| readonly onError?: (error: mixed) => void |},
  ) => Promise<{|
    readonly status: number,
    readonly headers?: { readonly [string]: string },
    // A promise, and not `void`: `DocumentBody.pipe` resolves on the last byte
    // and rejects when the render fails after the shell. Typing it away was
    // how the rejection below came to be dropped.
    readonly pipe: (destination: NodeResponse) => Promise<void>,
    readonly stream: () => ReadableStream<Uint8Array>,
  |}>,
  readonly dispatch: (request: Request) => Promise<Response | null>,
  /**
   * The guard on the path, run before anything under it answers.
   *
   * Called rather than tested for: a server bundle without it is a `TypeError`
   * on the first request, not an application whose auth check quietly stopped
   * running once it was compiled. See ubugeeei-prod/uf#260, and
   * `@uniflowed/vite`'s `createApplicationHandler`, which says the same thing
   * about `uf preview` and `uf start`.
   */
  readonly runMiddleware: (request: Request) => Promise<Response | null>,
|};

/** Everything an application needs to answer a request, all of it built in. */
export type HandlerOptions = {|
  readonly app: StandaloneApp,
  readonly assets: EmbeddedAssets,
  readonly document: DocumentAssets,
|};

/** What [`serve`] needs: the above, and where to listen. */
export type ServeOptions = {|
  readonly app: StandaloneApp,
  readonly assets: EmbeddedAssets,
  readonly document: DocumentAssets,
  /** Overridden by `--port` and then by `PORT`; defaults to 3000. */
  readonly port?: number,
  /** Overridden by `--host` and then by `HOST`; defaults to loopback. */
  readonly host?: string,
|};

/**
 * How long a browser may keep a file that is not a document.
 *
 * One hour, uniformly, and deliberately not the year-long `immutable` a
 * content-hashed chunk could take: this module cannot tell a hashed chunk from
 * an unhashed file copied out of `public/`, because `dist/` records no such
 * distinction, and getting it wrong in the `immutable` direction pins a stale
 * favicon in every visitor's cache with no way to recall it. A binary behind a
 * CDN should let the CDN decide; an hour is the safe answer for one that is not.
 */
const ASSET_CACHE_CONTROL = "public, max-age=3600";

/** Documents are revalidated every time, because a deploy replaces them. */
const DOCUMENT_CACHE_CONTROL = "no-cache";

/**
 * Serve the application until the process is stopped.
 *
 * Resolves once the socket is listening, with the address it took — a caller
 * that asked for port 0 has no other way to learn which port it got, and the
 * test that drives a compiled binary needs exactly that.
 */
export async function serve(options: ServeOptions): Promise<{|
  readonly host: string,
  readonly port: number,
  readonly close: () => Promise<void>,
|}> {
  const handle = createHandler(options);

  // Said before the socket, not after it, and that ordering is the point. It
  // is the one fact about a compiled binary that cannot be checked from
  // outside it — whether `dist/` really came along — and a count printed only
  // on a successful bind is a count nobody can see on a machine that is not
  // allowed to bind. `compile_writes_one_file_that_carries_the_whole_site`
  // reads this line and nothing else.
  process.stdout.write(`uf: ${String(Object.keys(options.assets).length)} embedded files\n`);

  const server = createServer((request: NodeRequest, response: NodeResponse) => {
    handle(request, response).catch((error: mixed) => {
      // A request that throws is this server's last chance to say so: there is
      // no framework above it and no log drain beside it. Report it on stderr
      // and answer 500, rather than letting the host's unhandled-rejection
      // policy decide whether the process survives.
      process.stderr.write(`uf: ${String(error?.stack ?? error)}\n`);
      try {
        response.statusCode = 500;
        response.setHeader("content-type", "text/plain; charset=utf-8");
        response.end("internal server error\n");
      } catch {
        // The response was already partly written; nothing left to say.
      }
    });
  });

  const host = options.host ?? argument("--host") ?? process.env.HOST ?? "127.0.0.1";
  const port = options.port ?? Number(argument("--port") ?? process.env.PORT ?? 3000);

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, host, resolve);
  });

  const address = server.address();
  const bound = typeof address === "object" && address != null ? address.port : port;
  process.stdout.write(`uf: listening on http://${host}:${String(bound)}\n`);

  return {
    host,
    port: bound,
    close: () =>
      new Promise((resolve) => {
        server.close(() => resolve());
      }),
  };
}

/**
 * The embedded files, as a map.
 *
 * A `Map` rather than the generated object itself, because a lookup keyed by a
 * request path must not be able to find `__proto__` or `constructor`. Building
 * it costs one pass over a few hundred entries at startup and removes the
 * question entirely.
 *
 * The bytes are decoded lazily and then kept: a build that embeds a hundred
 * megabytes of sourcemaps should not spend the startup decoding the ones this
 * process will never be asked for.
 */
function index(assets: EmbeddedAssets): Map<string, {| +type: string, +bytes: () => Buffer |}> {
  const files = new Map();
  for (const path of Object.keys(assets)) {
    const asset = assets[path];
    let decoded: Buffer | null = null;
    files.set(path, {
      type: asset.type,
      bytes: () => {
        if (decoded == null) {
          // `Buffer.from` rather than `atob`: `atob` answers with a string of
          // char codes, and turning megabytes of that into bytes is a loop in
          // JavaScript. Every host that has `node:http` has `node:buffer`.
          decoded = Buffer.from(asset.body, "base64");
        }
        return decoded;
      },
    });
  }
  return files;
}

/**
 * One request, answered — exported so an application can be mounted rather
 * than only run.
 *
 * `serve` is the whole of a compiled binary, and it is not the whole of what
 * anybody wants: a uf application behind an existing Node server, or beside
 * other routes in one process, needs the request handling without the socket.
 * That is this. It is also what the tests drive, which is not a coincidence —
 * a request handler that can only be reached through a listening socket is one
 * that can only be tested on a machine allowed to bind one.
 *
 * The order is `internal/serve.js`'s, which is Vite's:
 *
 *   1. a file `uf build` already wrote, for `GET` and `HEAD` only — an
 *      embedded path that matches exactly, such as `/assets/index-a1b2c3.js`
 *      or anything copied out of `public/`, and then the prerendered document
 *      for this URL, because `/guide` was written as `guide/index.html`;
 *   2. a route handler, for any method, because a handler is the only thing
 *      that answers a `POST` and may also answer a `GET` for a path with no
 *      page;
 *   3. and otherwise the renderer, which also produces the 404.
 *
 * The prerendered document is looked up *before* the dispatcher, and that is
 * the one place this used to disagree with `uf preview` and `uf start`. The
 * router allows a handler to sit beside a page in the same directory, so a
 * path can have both — and Vite's preview server serves the file first with no
 * say in the matter, so a binary that let the handler win would answer
 * differently from the command a build is checked with. Answering the same
 * wrong-looking way as the other two is worth more than answering a better way
 * alone.
 *
 * A page never answers a `POST`: letting one try turns a missing handler into
 * a rendered page with a 200 where the caller expected a 405.
 */
export function createHandler(
  options: HandlerOptions,
): (NodeRequest, NodeResponse) => Promise<void> {
  const { app, document } = options;
  const files = index(options.assets);

  return async function handle(request: NodeRequest, response: NodeResponse): Promise<void> {
    const method = (request.method ?? "GET").toUpperCase();
    const url = new URL(request.url ?? "/", "http://localhost");
    const pathname = decodePath(url.pathname);

    if (pathname != null && (method === "GET" || method === "HEAD")) {
      const file = files.get(assetKey(pathname));
      if (file != null) {
        sendBytes(response, method, 200, file.type, ASSET_CACHE_CONTROL, file.bytes());
        return;
      }
      // A document is revalidated where an asset is cached, because a deploy
      // replaces documents and gives assets a new hashed name.
      const page = files.get(documentKey(pathname));
      if (page != null) {
        sendBytes(response, method, 200, page.type, DOCUMENT_CACHE_CONTROL, page.bytes());
        return;
      }
    }

    // Middleware above the dispatcher and above the render, and below the two
    // lookups on purpose. It guards a path, so it must run for a page, for a
    // route handler, and for a path under it that matches neither — but an
    // embedded asset and a prerendered document are answered before it, which
    // is exactly what `uf preview` does, because Vite's file middleware runs
    // before anything mounted behind it. The three front doors have to give
    // one answer; that a prerendered page under a guard ships unguarded is
    // true of all of them and is ubugeeei-prod/uf#342.
    const asRequest = toRequest(request, url);
    const guarded = await app.runMiddleware(asRequest);
    if (guarded != null) {
      await send(response, method, guarded);
      return;
    }

    const handled = await app.dispatch(asRequest);
    if (handled != null) {
      await send(response, method, handled);
      return;
    }

    if (method !== "GET" && method !== "HEAD") {
      // A page supports exactly `GET` and `HEAD`, which is why the `Allow` the
      // specification requires on every 405 can be written here even though
      // this side of the handler knows nothing about methods. A *handler* path
      // with the wrong method never reaches this line: the dispatcher answers
      // that one itself, with the methods that module really exports.
      response.setHeader("allow", "GET, HEAD");
      sendBytes(
        response,
        method,
        405,
        "text/plain; charset=utf-8",
        DOCUMENT_CACHE_CONTROL,
        Buffer.from("method not allowed\n"),
      );
      return;
    }

    const rendered = await app.render(url.pathname + url.search, document, {
      // There is no terminal to render into: this is a binary somebody started
      // with `./app`, possibly under a supervisor. The console is where a
      // supervisor looks, and losing a boundary's exception entirely would be
      // worse — it is the only trace a page that failed after its first byte
      // leaves anywhere.
      onError: (error) => {
        console.error(error);
      },
    });
    response.statusCode = rendered.status;
    response.setHeader("content-type", "text/html; charset=utf-8");
    response.setHeader("cache-control", DOCUMENT_CACHE_CONTROL);
    for (const name of Object.keys(rendered.headers ?? {})) {
      response.setHeader(name, (rendered.headers ?? {})[name]);
    }
    // No `content-length`: the length is not known until the last byte, and
    // waiting for it is the whole of what streaming is not. `HEAD` gets the
    // status and the headers, and the stream is cancelled rather than dropped
    // so the render behind it stops instead of filling its queue and waiting
    // for a reader that is never coming.
    if (method === "HEAD") {
      await rendered.stream().cancel();
      response.end();
      return;
    }
    // Awaited, because `pipe` rejects: React hands a post-shell failure to the
    // destination's `destroy(error)`, `ChunkQueue.fail` records it, and the
    // generator `pipe` is iterating rethrows it. Called and dropped, that
    // rejection escapes this handler — `serve`'s `handle(…).catch` has already
    // resolved — and lands on the process, where `--unhandled-rejections=throw`
    // is the default and a binary someone started with `./app` exits in the
    // middle of a request that was otherwise recoverable.
    //
    // It cannot become a 500. The shell went out with its status and headers
    // long before this, and `pipe`'s own `finally` has already called `end()`.
    // What is left is to say so where a supervisor looks, and to drop the
    // socket: a chunked response that is closed cleanly is a client being told
    // a truncated document is the whole document, which is the failure this
    // pull request is named after.
    try {
      await rendered.pipe(response);
    } catch (error) {
      process.stderr.write(`uf: ${String(error?.stack ?? error)}\n`);
      response.destroy(error);
    }
  };
}

/**
 * A request path as an embedded key, or `null` when it cannot be one.
 *
 * Percent-decoding happens here rather than at the lookup, because a path that
 * does not decode is a malformed request and not a missing file.
 */
function decodePath(pathname: string): string | null {
  try {
    return decodeURIComponent(pathname);
  } catch {
    return null;
  }
}

/** `/assets/x.js` is the embedded `assets/x.js`. */
function assetKey(pathname: string): string {
  return pathname.replace(/^\/+/, "");
}

/**
 * The prerendered document for a URL.
 *
 * `uf build` writes `/guide` as `guide/index.html`, and `/` as `index.html`,
 * so both spellings of a directory URL find the same file.
 */
function documentKey(pathname: string): string {
  const key = assetKey(pathname).replace(/\/+$/, "");
  return key === "" ? "index.html" : `${key}/index.html`;
}

/**
 * A Node request as a `Request`.
 *
 * The body is passed as a stream where the host allows it, so a handler that
 * accepts an upload does not need the whole thing buffered before it starts.
 * `duplex` is required by the specification whenever a body is a stream, and
 * Node throws without it.
 */
function toRequest(incoming: NodeRequest, url: URL): Request {
  const headers = new Headers();
  for (const name of Object.keys(incoming.headers)) {
    const value = incoming.headers[name];
    if (value == null) continue;
    for (const entry of Array.isArray(value) ? value : [value]) {
      headers.append(name, entry);
    }
  }

  const host = headers.get("host");
  if (host != null && host !== "") {
    url.host = host;
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
async function send(outgoing: NodeResponse, method: string, result: Response): Promise<void> {
  outgoing.statusCode = result.status;
  for (const [name, value] of result.headers) {
    outgoing.setHeader(name, value);
  }
  if (result.body == null || method === "HEAD") {
    outgoing.end();
    return;
  }
  // Streamed rather than buffered, so a handler returning a large or
  // open-ended body is not read into memory first — and paced by what the
  // socket will take, or it is only streamed in shape. `write` answers `false`
  // when the kernel buffer is full and the remainder is being held in this
  // process, so a loop that read on regardless turned a slow client into a heap
  // the size of everything it had not acknowledged.
  //
  // The same loop as `@uniflowed/vite`'s `internal/http.js`, and the same
  // comments, because this is the third copy of "write a `Response` to a Node
  // response" and the three have to answer alike: a binary that buffered where
  // `uf start` paced would be the one deployment target whose memory profile
  // nobody had measured. The code is not shared for the reason at the top of
  // this file — importing `@uniflowed/vite` into the artefact a deployment runs
  // is the property `uf start` exists to establish.
  const reader = result.body.getReader();
  // A client that hangs up is the other half: nothing written after that goes
  // anywhere, and the producer behind the body keeps producing for a reader
  // that is never coming back.
  let open = true;
  const onClose = () => {
    open = false;
  };
  outgoing.on("close", onClose);
  try {
    while (open) {
      const { done, value } = await reader.read();
      if (done === true || !open) break;
      if (outgoing.write(value) === false) {
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
  // Best effort: the connection is already gone, so there is nobody left to
  // report a failed cancellation to and no response left to fail.
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

/** Write one embedded file, with the length a client needs to reuse a socket. */
function sendBytes(
  outgoing: NodeResponse,
  method: string,
  status: number,
  type: string,
  cacheControl: string,
  bytes: Buffer,
): void {
  outgoing.statusCode = status;
  outgoing.setHeader("content-type", type);
  outgoing.setHeader("cache-control", cacheControl);
  outgoing.setHeader("content-length", String(bytes.byteLength));
  outgoing.end(method === "HEAD" ? undefined : bytes);
}

/** The value of a `--flag value` pair on the command line, if it is there. */
function argument(name: string): string | null {
  const at = process.argv.indexOf(name);
  return at === -1 ? null : (process.argv[at + 1] ?? null);
}
