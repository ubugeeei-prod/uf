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
import { createServer } from "node:http";
import { Readable } from "node:stream";

import type { Instant } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";
import type { CapabilityOptions, ServerCapabilities } from "./internal/capabilities.js";
import { assertCapable, capabilitiesFor } from "./internal/capabilities.js";
import type { RequestLifecycle } from "./internal/context.js";
import { locateStatic, staticRoot } from "./internal/static.js";
import type { Logger } from "./internal/log.js";
import { elapsedMs, logRequest, processLogger } from "./log.js";

export type { RequestLifecycle } from "./internal/context.js";

// The fourth front door is not in this package: `uf preview` serves files with
// Vite's own middleware, which runs in front of anything uf mounts behind it,
// so the skip in `createStaticHandler` below never sees the request
// (ubugeeei-prod/uf#620). It cannot re-argue the rule and it must not
// re-implement it, so the rule is exported — `@uniflowed/vite`'s
// `internal/serve.js` reaches it through this module, which is already the one
// it loads for `createStaticHandler`.
export { prerenderedMayAnswer } from "./internal/draft.js";

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
 * Which file, and with what headers, is
 * [`locateStatic`](./internal/static.js) — shared with every other host,
 * because none of it is about Node. What is Node's is the last line: the body.
 */
export function createStaticHandler(options: {|
  readonly root: string,
|}): (request: Request) => Promise<Response | null> {
  const state = staticRoot(options.root);

  return async function serveStatic(request: Request): Promise<Response | null> {
    const found = await locateStatic(state, request);
    if (found == null) return null;
    if (found.headOnly) return new Response(null, { headers: found.headers });
    // Streamed rather than read into memory, so serving a large asset costs
    // a buffer rather than the file. This line is the whole of what is
    // host-specific about serving a directory — see `./internal/static.js`,
    // and `./bun.js` for the same function with the other body.
    // $FlowFixMe[incompatible-call] - a Node web stream is a `BodyInit`.
    return new Response(Readable.toWeb(createReadStream(found.path)), { headers: found.headers });
  };
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
 * A handler that throws is answered with a bare 500 and reported to the logger:
 * the body must not carry the stack, because the body goes to whoever asked,
 * and the log is where the operator is already looking. The drain is in a
 * `finally` below the `catch`, so a middleware that logged the request sees its
 * callback run once that 500 is on the wire rather than once the handler gave
 * up — and a request that failed is still a request that happened, which is why
 * it is drained at all.
 *
 * # The access line
 *
 * One line per request, written after the response and never before it, because
 * the two things worth knowing — what it answered and how long it took — are
 * only true then. It carries the route that matched rather than only the path,
 * which is the point of ubugeeei-prod/uf#506 and the reason this reads
 * `lifecycle.context` at the end: the route is not known when the request
 * begins, and by here whichever of the dispatcher and the renderer claimed it
 * has said so. `@uniflowed/server/log`'s `logRequest` decides the level and the
 * field names, so every host says it the same way.
 *
 * A request whose bytes were not a request Node could parse never reaches this
 * function at all; that one is `clientError` in [`serve`], which is the half of
 * ubugeeei-prod/uf#405 that had nowhere to be reported.
 */
export function nodeListener(
  handle: (request: Request) => Promise<Response>,
  options: {|
    readonly beginRequest: (request: Request) => RequestLifecycle,
    readonly secure?: boolean,
    /**
     * Where this listener's lines go. The process logger by default.
     *
     * Passed rather than only installed globally because a host that runs two
     * servers in one process — `uf preview` beside a test harness — has a
     * reason to tell them apart, and because a test that wants silence should
     * not have to reach for a global to get it.
     */
    readonly log?: Logger,
  |},
): (incoming: NodeRequest, outgoing: NodeResponse) => Promise<void> {
  return async function listener(incoming: NodeRequest, outgoing: NodeResponse): Promise<void> {
    // Declared out here because `toRequest` is inside the `try`: a request that
    // could not even be built has no lifecycle to settle.
    let lifecycle: RequestLifecycle | null = null;
    // Resolved once rather than at each of the two places that write a line.
    // One request leaves one account of itself, and a process logger installed
    // halfway through this one would otherwise split it across two sinks.
    const log = options.log ?? processLogger();
    // uf's clock, not the host's: `@uniflowed/server/log`'s `elapsedMs` reads
    // the same one at the other end, so what is measured here is one seam's
    // idea of the time rather than two calls to a global.
    const started = Temporal.Now.instant();
    // The path, before anything can fail. A request that could not be built has
    // no `URL` to take one from, and a line saying nothing about which request
    // it was would be the state ubugeeei-prod/uf#405 describes.
    const target = incoming.originalUrl ?? incoming.url ?? "/";
    try {
      const request = toRequest(incoming, options);
      lifecycle = options.beginRequest(request);
      await lifecycle.run(async () => {
        await send(outgoing, await handle(request));
      });
    } catch (error) {
      // `error` is a field rather than part of the message: an exception's text
      // is the varying half of what happened, and a logger that interpolated it
      // would produce a million distinct messages for one fault.
      log.error("request failed", { error, path: pathOf(target) });
      if (outgoing.headersSent) {
        outgoing.destroy();
      } else {
        outgoing.statusCode = 500;
        outgoing.setHeader("content-type", "text/plain; charset=utf-8");
        outgoing.end("500 Internal Server Error\n");
      }
    } finally {
      logRequest(log, {
        // A request that never got a context still gets a line; it gets an
        // empty id rather than a fabricated one, because inventing an id for a
        // request that had none would put a value in the log that nothing else
        // in the system has ever seen.
        requestId: lifecycle?.context.id ?? "",
        method: (incoming.method ?? "GET").toUpperCase(),
        path: pathOf(target),
        route: lifecycle?.context.route ?? null,
        status: outgoing.statusCode,
        durationMs: elapsedMs(started),
      });
      if (lifecycle != null) await lifecycle.settle();
    }
  };
}

/**
 * The path half of a request target, with the query string dropped.
 *
 * Not `new URL(...).pathname`: this runs on whatever bytes arrived, including
 * the ones that are not a URL at all, and a constructor that throws in the
 * `finally` of a failed request would replace one fault with another. A cut at
 * the first `?` or `#` is all that is needed, and `@uniflowed/server/log` says
 * why the rest must not be logged.
 *
 * Two `indexOf` calls rather than one small regular expression, because
 * `docs/security.md` rule 5 is about the whole class and not about whether this
 * particular pattern could backtrack. A scan that is obviously linear needs no
 * argument.
 */
function pathOf(target: string): string {
  const query = target.indexOf("?");
  const fragment = target.indexOf("#");
  if (query === -1) return fragment === -1 ? target : target.slice(0, fragment);
  return target.slice(0, fragment === -1 ? query : Math.min(query, fragment));
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
 *
 * # The request that never became one
 *
 * [`reportMalformedRequests`] is the other half of ubugeeei-prod/uf#405, and it
 * is a separate function because this one cannot be called without a socket and
 * that one can: a `stream.Duplex` handed to an `http.Server` as a connection
 * drives the real parser with no `listen` at all, which is what makes the
 * behaviour testable on a machine where `bind` is refused. `./standalone.js`
 * calls it too, so a compiled binary is not the one front door that stays
 * silent.
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
  /** Where this server's lines go. The process logger by default. */
  readonly log?: Logger,
|}): Promise<{|
  readonly host: string,
  readonly port: number,
  readonly close: () => Promise<void>,
|}> {
  const log = options.log ?? processLogger();
  const listener = nodeListener(
    createServeHandler({ staticDir: options.staticDir, handle: options.handle }),
    { beginRequest: options.beginRequest, log },
  );
  const server = createServer((request, response) => {
    void listener(request, response);
  });
  reportMalformedRequests(server, log);

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

/** The pieces of a `node:http` server this module attaches to. */
export type ClientErrorServer = {
  on(event: string, listener: (error: mixed, socket: ClientErrorSocket) => mixed): mixed,
  ...
};

/** The pieces of a socket a refused request is answered on. */
export type ClientErrorSocket = {
  readonly writableEnded: boolean,
  end(chunk?: string): mixed,
  ...
};

/**
 * Most `malformed request` lines one server writes per window.
 *
 * Twenty is enough to see the shape of a fault — the same `code` twenty times
 * is one client with a broken library, twenty different ones are a scan — and
 * few enough that a window's worth is a paragraph rather than a page.
 */
const MALFORMED_LOG_BURST = 20;

/** How long that budget lasts, in milliseconds. */
const MALFORMED_LOG_WINDOW_MS = 60_000;

/**
 * Say what the runtime's own parser refused, on a server that says nothing.
 *
 * The whole of ubugeeei-prod/uf#405. Bytes Node's parser rejects never reach a
 * request listener: `http.Server` answers `400 Bad Request`, closes the socket,
 * and — with no `clientError` handler attached — says so to nobody. An operator
 * whose client is sending a header Node will not accept sees a failing request
 * and an empty terminal, which is the worst combination a server can offer, and
 * a day was spent on exactly that (`assert_served` built a request target with
 * a space in it, and the space was invisible from every side).
 *
 * Node's own `400` is still what goes on the wire. This handler writes the same
 * bytes the default one does, because replacing the default means taking over
 * its job as well as adding to it, and answering differently would make the
 * change a behaviour change rather than a diagnostic one.
 *
 * # The log-volume question, answered
 *
 * This is the reason somebody might not want it, so it is decided here rather
 * than left to be discovered on a public address. A malformed request is two
 * dozen bytes to send and, unbounded, one line to write — which makes a line
 * per rejection an amplifier: a client that can saturate a link can fill a disk
 * and a log bill with it, and can push whatever an operator actually needed off
 * the end of the retention window.
 *
 * So the budget is **twenty lines a minute per server, and a count of what that
 * hid**. The first twenty in a window are written; the rest are counted, and
 * the count is written once, when the next window opens, as its own record. A
 * flood therefore costs a fixed number of lines per minute and still says how
 * big it was, which is the number an operator wants from a flood — the
 * individual lines of one are all the same line.
 *
 * Sampling was the alternative and it is worse for this: one in a hundred of a
 * flood is still unbounded, and one in a hundred of *three* malformed requests
 * is nothing at all, which is the case where the line matters most.
 *
 * The budget is per call rather than per process, so two servers in one
 * process — `uf preview` beside a test harness — cannot spend each other's.
 *
 * # And what is in the line
 *
 * `error.code` and nothing else. Node puts the offending bytes on
 * `error.rawPacket`, and those are whatever the client sent: the one thing
 * `@uniflowed/server/log`'s header spends its length explaining does not belong
 * in a log line. The `code` is llhttp's own enumeration — `HPE_INVALID_METHOD`,
 * `HPE_HEADER_OVERFLOW` — which is a closed set uf did not have to invent, and
 * it is the half that says what to fix.
 *
 * `warn` rather than `error`, for the reason `logRequest` picks a level from a
 * status: a malformed request is somebody else's mistake far more often than it
 * is this server's.
 */
export function reportMalformedRequests(server: ClientErrorServer, log: Logger): void {
  let windowStarted: Instant | null = null;
  let written = 0;
  let suppressed = 0;

  server.on("clientError", (error: mixed, socket: ClientErrorSocket) => {
    const code = errorCode(error);
    const now = Temporal.Now.instant();
    if (windowStarted == null || elapsedMs(windowStarted) >= MALFORMED_LOG_WINDOW_MS) {
      if (suppressed > 0) {
        // The count is its own record with its own message, so a collector can
        // alert on the *shape* — "this server is being flooded" — without
        // parsing a number out of a sentence about one request.
        log.warn("malformed requests not logged", { count: suppressed });
      }
      windowStarted = now;
      written = 0;
      suppressed = 0;
    }
    if (written < MALFORMED_LOG_BURST) {
      written += 1;
      log.warn("malformed request", { code });
    } else {
      suppressed += 1;
    }

    // A socket that is already gone, or one whose error was a timeout Node has
    // handled itself, must not be written to; the check is the one Node's own
    // documentation gives for replacing this handler.
    if (code === "ECONNRESET" || socket.writableEnded) {
      return;
    }
    socket.end("HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
  });
}

/** The value of a `--flag value` pair on the command line, if it is there. */
function argument(name: string): string | null {
  const at = process.argv.indexOf(name);
  return at === -1 ? null : (process.argv[at + 1] ?? null);
}

/**
 * The `code` of a Node error, as a string, or `unknown`.
 *
 * A named helper because the alternative at the two call sites above is a cast:
 * `clientError` hands over an `Error`, and the `code` every Node error in fact
 * carries is not on that type.
 */
function errorCode(error: mixed): string {
  if (error != null && typeof error === "object") {
    const code = error.code;
    if (typeof code === "string") return code;
  }
  return "unknown";
}
