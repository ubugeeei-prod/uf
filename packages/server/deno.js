// @flow
//
// `@uniflowed/server/deno`: the Deno half of serving a build.
//
// The sibling of `./bun.js`: a host whose server already speaks
// `Request` -> `Response`, with a native file body for the static half. The
// application contract is still the same one `./node.js` and `./bun.js` expose
// to generated `server.js`: hand it a static directory, a fetch handler and the
// request lifecycle belonging to the bundled application.

import type { CapabilityOptions, ServerCapabilities } from "./internal/capabilities.js";
import { assertCapable, capabilitiesFor } from "./internal/capabilities.js";
import type { RequestLifecycle } from "./internal/context.js";
import type { RoutingRules } from "./internal/routing.js";
import { admit, headersFor, withHeaders } from "./internal/routing.js";
import { locateStatic, offerBuildFiles, staticRoot } from "./internal/static.js";
import type { Schedule } from "./schedule.js";
import { startSchedules } from "./schedule.js";
import type { Logger } from "./internal/log.js";
import { processLogger } from "./log.js";
import { answerReturned } from "./internal/returned-response.js";

/**
 * What a Deno host can do, plus whatever the deployment supplied.
 *
 * Deno keeps a process and its `Deno.serve` response can stream a body, so it
 * gets the same two base capabilities as Node and Bun.
 */
export function denoCapabilities(options?: CapabilityOptions): ServerCapabilities {
  return assertCapable(capabilitiesFor("deno", { stream: true, persistent: true }, options));
}

/**
 * The static half: a file under `root`, or `null` for the caller to carry on.
 */
export function createStaticHandler(options: {|
  readonly root: string,
|}): (request: Request) => Promise<Response | null> {
  const state = staticRoot(options.root);

  return async function serveStatic(request: Request): Promise<Response | null> {
    const found = await locateStatic(state, request);
    if (found == null) return null;
    if (found.headOnly) return new Response(null, { headers: found.headers });
    const file = await Deno.open(found.path, { read: true });
    return new Response(file.readable, { headers: found.headers });
  };
}

/**
 * The static half first, then the application — behind `routing`'s redirects
 * and under its headers, as in `./node.js`.
 */
export function createServeHandler(options: {|
  readonly staticDir: string,
  readonly handle: (request: Request) => Promise<Response>,
  readonly routing?: RoutingRules,
|}): (request: Request) => Promise<Response> {
  const serveStatic = createStaticHandler({ root: options.staticDir });
  return async function handle(request: Request): Promise<Response> {
    const headers = headersFor(options.routing, request);
    const admitted = admit(options.routing, request);
    if (admitted.kind === "answer") return withHeaders(admitted.response, headers);
    const addressed = admitted.request;
    const file = await serveStatic(addressed);
    if (file != null) return withHeaders(file, headers);
    offerBuildFiles(addressed, serveStatic);
    return withHeaders(await options.handle(addressed), headers);
  };
}

type DenoFile = {
  readonly readable: ReadableStream,
  close(): void,
  ...
};

type DenoServer = {
  readonly addr: {|
    readonly hostname: string,
    readonly port: number,
    readonly transport: string,
  |},
  shutdown(): Promise<void>,
  ...
};

declare var Deno: {
  readonly args: $ReadOnlyArray<string>,
  readonly env: {
    get(name: string): string | void,
    ...
  },
  open(path: string, options: {| read: true |}): Promise<DenoFile>,
  serve(
    options: {
      hostname: string,
      port: number,
      onListen?: () => mixed,
      ...
    },
    handler: (request: Request) => Promise<Response>,
  ): DenoServer,
  exit(code?: number): void,
  ...
};

/**
 * Serve the application until the process is stopped.
 */
export async function serve(options: {|
  readonly staticDir: string,
  readonly handle: (request: Request) => Promise<Response>,
  readonly beginRequest: (request: Request) => RequestLifecycle,
  readonly host?: string,
  readonly port?: number,
  readonly log?: Logger,
  readonly schedules?: $ReadOnlyArray<Schedule>,
  readonly routing?: RoutingRules,
|}): Promise<{|
  readonly host: string,
  readonly port: number,
  readonly close: () => Promise<void>,
|}> {
  const log = options.log ?? processLogger();
  const handle = createServeHandler({
    staticDir: options.staticDir,
    handle: options.handle,
    routing: options.routing,
  });

  const host = options.host ?? argument("--host") ?? Deno.env.get("HOST") ?? "0.0.0.0";
  const port = options.port ?? Number(argument("--port") ?? Deno.env.get("PORT") ?? 3000);

  const server = Deno.serve(
    {
      hostname: host,
      port,
      // Generated entries print the same line Node and Bun print; Deno's
      // default listener line would be a second, differently-shaped one.
      onListen: () => {},
    },
    (request: Request): Promise<Response> =>
      answerReturned(request, handle, options.beginRequest, log),
  );

  const shown = host === "0.0.0.0" || host === "::" ? "localhost" : host;
  console.log(`uf: listening on http://${shown}:${String(server.addr.port)}`);

  const stopSchedules = startSchedules(options.schedules, log);

  return {
    host,
    port: server.addr.port,
    close: async () => {
      stopSchedules();
      await server.shutdown();
    },
  };
}

function argument(name: string): string | null {
  const at = Deno.args.indexOf(name);
  return at === -1 ? null : (Deno.args[at + 1] ?? null);
}
