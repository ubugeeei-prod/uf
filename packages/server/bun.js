// @flow
//
// `@uniflowed/server/bun`: the Bun half of serving a build.
//
// The sibling of `./node.js`, and deliberately much shorter. Bun's server
// already speaks `Request` → `Response`, which is the shape `../fetch.js`
// hands it, so none of Node's translation — `toRequest`, `send`, the
// backpressure dance in `writable` — has anything to do here. What is left is
// a body, a socket, and the request lifecycle.
//
// # Why this exists at all, given `node` already runs on Bun
//
// `uf build --adapter node` produces a directory Bun executes unchanged, so an
// adapter that were not measurably faster would be a directory with a
// different name on it. That was the condition written into
// `DeployAdapter::unimplemented_because`, and it is met on both halves of a
// request (ubugeeei-prod/uf#391):
//
// | path | node | bun | |
// | --- | ---: | ---: | --- |
// | dynamic, `/api/health`, c=32 | 14,006/s | 18,480/s | +32% |
// | static, 215 KB asset, c=32 | 5,742/s | 7,251/s | +26% |
// | static, 508 B asset, c=64 | 18,125/s | 27,862/s | +54% |
//
// # The body is the whole of what differs
//
// The static win above is **not** the runtime. `Bun.serve` handed a
// `Readable.toWeb(createReadStream(…))` — which is what reusing `./node.js`'s
// handler verbatim would give it — measures 5,670/s on that 215 KB asset at
// c=64 against `node:http`'s 5,732: level, inside the noise. The +17% is
// `Bun.file`, and nothing else.
//
// So this module is `./internal/static.js` for every decision and one line of
// its own for the bytes. It re-implements no part of the path policy: the
// containment check, the symlink re-check (#550), the draft rule (#620) and
// the candidate order live once, where both hosts read them.

import type { CapabilityOptions, ServerCapabilities } from "./internal/capabilities.js";
import { assertCapable, capabilitiesFor } from "./internal/capabilities.js";
import type { RequestLifecycle } from "./internal/context.js";
import { locateStatic, staticRoot } from "./internal/static.js";
import type { Schedule } from "./schedule.js";
import { startSchedules } from "./schedule.js";
import type { Logger } from "./internal/log.js";
import { elapsedMs, logRequest, processLogger } from "./log.js";
import { Temporal } from "@uniflowed/core/temporal";

/**
 * What a Bun host can do, plus whatever the deployment supplied.
 *
 * The same two as Node's and for the same reasons: a `Bun.serve` response is a
 * socket, so a body reaches the client as it is written, and the process is
 * still there once the response has gone.
 *
 * `websocket` is still the deployment's to pass even though `Bun.serve` has
 * one built in. Taking it means choosing where a socket's messages go, and
 * `docs/red-lines.md` rule 3 says that is not uf's choice to make on a
 * project's behalf — the same answer `./node.js` gives for the same question.
 */
export function bunCapabilities(options?: CapabilityOptions): ServerCapabilities {
  return assertCapable(capabilitiesFor("bun", { stream: true, persistent: true }, options));
}

/**
 * The static half: a file under `root`, or `null` for the caller to carry on.
 *
 * Which file, and with what headers, is
 * [`locateStatic`](./internal/static.js) — the same function `./node.js`
 * asks. What is Bun's is the last line.
 */
export function createStaticHandler(options: {|
  readonly root: string,
|}): (request: Request) => Promise<Response | null> {
  const state = staticRoot(options.root);

  return async function serveStatic(request: Request): Promise<Response | null> {
    const found = await locateStatic(state, request);
    if (found == null) return null;
    if (found.headOnly) return new Response(null, { headers: found.headers });
    // `Bun.file` is a lazy handle, not a read: the runtime sends the file from
    // the descriptor and the bytes never enter the heap. This is the line the
    // benchmark in the header is about.
    return new Response(Bun.file(found.path), { headers: found.headers });
  };
}

/** The static half first, then the application. */
export function createServeHandler(options: {|
  readonly staticDir: string,
  readonly handle: (request: Request) => Promise<Response>,
|}): (request: Request) => Promise<Response> {
  const serveStatic = createStaticHandler({ root: options.staticDir });
  return async function handle(request: Request): Promise<Response> {
    return (await serveStatic(request)) ?? (await options.handle(request));
  };
}

/** The pieces of `Bun.serve`'s return value this module uses. */
type BunServer = {
  readonly port: number,
  readonly hostname: string,
  stop(closeActiveConnections?: boolean): mixed,
  ...
};

/**
 * The Bun global, narrowed to the two calls this module makes.
 *
 * Declared here rather than pulled in as a libdef, which is the move
 * `@uniflowed/core/clock` makes for `Intl` and `@uniflowed/hooks/timing` for
 * `Intl.RelativeTimeFormat`: this checkout carries no `[libs]`, and a full
 * `bun-types` would put a second, larger set of DOM and Node globals in scope
 * for every file that resolves it.
 *
 * `file` is typed as returning a `Blob` because that is the part of `BunFile`
 * used — `new Response(blob)` — and the part whose contract is standard.
 */
declare var Bun: {
  file(path: string): Blob,
  serve(options: {
    hostname: string,
    port: number,
    fetch: (request: Request) => Promise<Response>,
    ...
  }): BunServer,
  ...
};

/**
 * Serve the application until the process is stopped.
 *
 * What `uf build --adapter bun` writes calls this and nothing else, and the
 * signature is `./node.js`'s exactly — `uf`'s Rust side chooses which entry to
 * emit and must not have to know that the two differ, and a project moving
 * between the two adapters should not find a different contract.
 *
 * `PORT` and `HOST` are read from the environment for the reason they are
 * there: that is how every process manager and container platform says which
 * socket to take. The command line wins over both.
 */
export async function serve(options: {|
  readonly staticDir: string,
  readonly handle: (request: Request) => Promise<Response>,
  /**
   * The application bundle's own `beginRequest`.
   *
   * From the bundle rather than imported here, for the reason `./node.js`
   * spells out under "Who owns the request": the request lives in an
   * `AsyncLocalStorage` belonging to a module *instance*, and the instance the
   * application reads is the one linked into `handler.js`.
   */
  readonly beginRequest: (request: Request) => RequestLifecycle,
  readonly host?: string,
  readonly port?: number,
  /** Where this server's lines go. The process logger by default. */
  readonly log?: Logger,
  /**
   * Schedules to run in this process, from `./schedule.js`.
   *
   * Bun keeps a process, so it is one of the targets that can hold a
   * scheduler rather than needing its platform to call one — the same as
   * `./node.js`, through the same helper, so the two cannot drift about how
   * often a tick happens. See ubugeeei-prod/uf#531.
   */
  readonly schedules?: $ReadOnlyArray<Schedule>,
|}): Promise<{|
  readonly host: string,
  readonly port: number,
  readonly close: () => Promise<void>,
|}> {
  const log = options.log ?? processLogger();
  const handle = createServeHandler({ staticDir: options.staticDir, handle: options.handle });

  const host = options.host ?? argument("--host") ?? process.env.HOST ?? "0.0.0.0";
  const port = options.port ?? Number(argument("--port") ?? process.env.PORT ?? 3000);

  const server: BunServer = Bun.serve({
    hostname: host,
    port,
    fetch: (request: Request): Promise<Response> =>
      answer(request, handle, options.beginRequest, log),
  });

  // `0.0.0.0` is not a URL anybody can open, so the loopback spelling is what
  // is printed — the same split `uf dev` and `uf start` print, and for the
  // same reason: one of the two is a link and the other is a fact about the
  // socket.
  const shown = host === "0.0.0.0" || host === "::" ? "localhost" : host;
  process.stdout.write(`uf: listening on http://${shown}:${String(server.port)}\n`);

  const stopSchedules = startSchedules(options.schedules, log);

  return {
    host,
    port: server.port,
    close: async () => {
      stopSchedules();
      await server.stop(true);
    },
  };
}

/**
 * One request: begun, answered inside its context, logged, and settled.
 *
 * The counterpart of `./node.js`'s `nodeListener`, and it makes the same
 * promise that module's header makes — `after()` runs once the response has
 * gone. It is shorter for one reason: this host hands back a `Response` rather
 * than writing to a socket, so "the response has gone" is a thing the runtime
 * decides. `settle` therefore runs after `run` resolves rather than after the
 * last byte, which is the honest place for it here and is why streaming a body
 * and deferring work with `after()` are documented as ordered against each
 * other only on Node.
 */
async function answer(
  request: Request,
  handle: (request: Request) => Promise<Response>,
  beginRequest: (request: Request) => RequestLifecycle,
  log: Logger,
): Promise<Response> {
  // uf's clock, not the host's: `@uniflowed/server/log`'s `elapsedMs` reads the
  // same one at the other end.
  const started = Temporal.Now.instant();
  const target = new URL(request.url).pathname;
  let lifecycle: RequestLifecycle | null = null;
  let response: Response;
  try {
    lifecycle = beginRequest(request);
    response = await lifecycle.run(() => handle(request));
  } catch (error) {
    // `error` is a field rather than part of the message: an exception's text
    // is the varying half of what happened, and a logger that interpolated it
    // would produce a million distinct messages for one fault.
    log.error("request failed", { error, path: target });
    response = new Response("500 Internal Server Error\n", {
      status: 500,
      headers: { "content-type": "text/plain; charset=utf-8" },
    });
  } finally {
    logRequest(log, {
      // A request that never got a context still gets a line; it gets an empty
      // id rather than a fabricated one, because inventing an id for a request
      // that had none would put a value in the log that nothing else in the
      // system has ever seen.
      requestId: lifecycle?.context.id ?? "",
      method: request.method.toUpperCase(),
      path: target,
      route: lifecycle?.context.route ?? null,
      status: response?.status ?? 500,
      durationMs: elapsedMs(started),
    });
    if (lifecycle != null) await lifecycle.settle();
  }
  return response;
}

function argument(name: string): string | null {
  const at = process.argv.indexOf(name);
  return at === -1 ? null : (process.argv[at + 1] ?? null);
}
