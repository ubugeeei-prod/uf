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
// # Why the request adapter is written out again here
//
// `@uniflowed/vite`'s driver has a `toRequest`/`send` pair that does this same
// job for `uf dev`. Importing it would have been the obvious way to share it,
// and it is the wrong one: `@uniflowed/vite` depends on Vite, so every
// compiled application would carry a bundler it can never use. Forty lines of
// adapter is much cheaper than that, and both copies have a test that drives
// them: `dev_serves_the_docs_site_through_vite` for the dev server's, and
// `tests/library/standalone.test.js` for this one.

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
  readonly render: (
    url: string,
    assets: DocumentAssets,
  ) => Promise<{|
    readonly status: number,
    readonly html: string,
    readonly headers?: { readonly [string]: string },
  |}>,
  readonly dispatch: (request: Request) => Promise<Response | null>,
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
 * The order is the dev server's, with one step added:
 *
 *   1. an embedded file whose path matches exactly — in `uf dev` this is
 *      Vite's own middleware, which runs before uf's and for the same reason:
 *      a request for `/assets/index-a1b2c3.js` is not a route;
 *   2. a route handler, for any method, because a handler is the only thing
 *      that answers a `POST` and may also answer a `GET` for a path with no
 *      page;
 *   3. for `GET` and `HEAD` only, the prerendered document `uf build` already
 *      wrote for this URL — the step `uf dev` has no equivalent of, because
 *      there is nothing prerendered to serve;
 *   4. and otherwise the renderer, which also produces the 404.
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
    }

    const handled = await app.dispatch(toRequest(request, url));
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

    if (pathname != null) {
      const page = files.get(documentKey(pathname));
      if (page != null) {
        sendBytes(response, method, 200, page.type, DOCUMENT_CACHE_CONTROL, page.bytes());
        return;
      }
    }

    const rendered = await app.render(url.pathname + url.search, document);
    response.statusCode = rendered.status;
    response.setHeader("content-type", "text/html; charset=utf-8");
    response.setHeader("cache-control", DOCUMENT_CACHE_CONTROL);
    for (const name of Object.keys(rendered.headers ?? {})) {
      response.setHeader(name, (rendered.headers ?? {})[name]);
    }
    const html = Buffer.from(rendered.html, "utf8");
    response.setHeader("content-length", String(html.byteLength));
    response.end(method === "HEAD" ? undefined : html);
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
  // open-ended body is not read into memory first.
  const reader = result.body.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    outgoing.write(value);
  }
  outgoing.end();
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
