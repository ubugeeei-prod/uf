// @flow
//
// `@uniflowed/server/vercel`: the application as a Vercel Node.js function.
//
// `uf build --adapter vercel` writes Vercel's Build Output API (v3): a
// `.vercel/output/config.json` that routes every request to one function, and
// that function's directory, `functions/uf.func`, holding the same
// `handler.js` every other adapter carries, its `static/` copy of the build,
// and an `index.js` whose default export is what this module makes. Vercel's
// Node.js launcher (`"launcherType": "Nodejs"` in `.vc-config.json`) calls
// that export with Node's own `IncomingMessage` and `ServerResponse`, so the
// function is `./node.js`'s listener with the static half in front of it — the
// same order `node server.js` answers in, and none of Vercel's request helpers
// (`shouldAddHelpers: false`), which would be a second parser of the request
// uf already parses.
//
// # Why every request goes to the function
//
// The files could be served from Vercel's CDN instead (`handle: "filesystem"`
// in `config.json`), and would be faster. They are not, yet, because the
// adapter contract (`docs/app/guide/deploy`, "The adapter contract") puts
// `app.router`'s redirects and headers in front of the files, and those rules
// are the application's to apply: translating them into Vercel's route
// grammar is a second implementation of `./internal/routing.js` that would
// have to agree with the first. Serving files from the function is the answer
// that is correct on the first request, and a faster one is an optimisation
// on top of it.
//
// # What a Vercel function is, in `./internal/capabilities.js`'s terms
//
// `stream: true` — the function directory declares
// `supportsResponseStreaming`, and a document's shell goes out before its
// holes. `persistent: false` — an invocation is frozen once its response is
// sent, like a Lambda: work that outlives the response is handed to the
// platform's `waitUntil` where the request context offers one, and awaited
// before the handler's promise resolves where it does not.

import type { CapabilityOptions, ServerCapabilities } from "./internal/capabilities.js";
import { assertCapable, capabilitiesFor } from "./internal/capabilities.js";
import type { RequestLifecycle } from "./internal/context.js";
import type { RoutingRules } from "./internal/routing.js";
import type { NodeRequest, NodeResponse } from "./node.js";
import { createServeHandler, nodeListener } from "./node.js";

export type { RequestLifecycle } from "./internal/context.js";

/**
 * What a Vercel Node.js function can do.
 *
 * Streams, because the generated `.vc-config.json` asks for response
 * streaming; does not persist, because an invocation is frozen after its
 * response. A deployment handing it a queue that lives in the process, or a
 * WebSocket upgrader, is refused here by name — the same refusal the Lambda
 * adapter makes, for the same reason.
 */
export function vercelCapabilities(options?: CapabilityOptions): ServerCapabilities {
  return assertCapable(capabilitiesFor("vercel", { stream: true, persistent: false }, options));
}

/** What `index.js` hands this module: the application and its build. */
export type VercelHandlerOptions = {|
  /** `handler.js`'s `fetch`. */
  readonly handle: (request: Request) => Promise<Response>,
  /** `handler.js`'s own `beginRequest`, never a copy installed beside it. */
  readonly beginRequest: (request: Request) => RequestLifecycle,
  /** The `static/` directory beside `index.js`. */
  readonly staticDir: string,
  /** `handler.js`'s `routing`: `app.router`'s redirects, rewrites and headers. */
  readonly routing?: RoutingRules,
|};

/** The piece of Vercel's request context this module reads. */
type VercelRequestContext = {
  +get?: () => ?{ +waitUntil?: (promise: Promise<mixed>) => mixed, ... },
  ...
};

/**
 * The platform's `waitUntil` for the invocation in flight, or `null`.
 *
 * Read from the request context Vercel's runtime publishes under
 * `Symbol.for("@vercel/request-context")` — the documented seam
 * `@vercel/functions`'s own `waitUntil` reads — rather than by depending on
 * that package: a provider SDK in the portable server package is what
 * `ubugeeei-redundancy.md` keeps out, and one symbol is all it would supply.
 * Absent outside Vercel (a test, `vercel dev`, the deploy matrix's harness),
 * where the handler's promise is what keeps the work alive.
 */
function platformWaitUntil(): ((promise: Promise<mixed>) => mixed) | null {
  const holder: { +[symbol]: ?VercelRequestContext, ... } = (globalThis: $FlowFixMe);
  const context = holder[Symbol.for("@vercel/request-context")];
  const current = context?.get?.();
  return typeof current?.waitUntil === "function" ? current.waitUntil : null;
}

/**
 * The function `index.js` exports as its default.
 *
 * Files first, then the application — `createServeHandler`, which is what
 * `node server.js` answers with — behind `nodeListener`, which begins the
 * request with `handler.js`'s own `beginRequest`, writes the response
 * (every `Set-Cookie` separate, the body streamed), answers a thrown error
 * with a bare `500`, and settles the request afterwards. The promise it
 * returns resolves once the settle has finished, and is also handed to the
 * platform's `waitUntil` when there is one, so deferred work — `after()`, a
 * durable cache write — is not frozen with the invocation.
 */
export function createVercelHandler(
  options: VercelHandlerOptions,
): (incoming: NodeRequest, outgoing: NodeResponse) => Promise<void> {
  const listener = nodeListener(
    createServeHandler({
      staticDir: options.staticDir,
      handle: options.handle,
      routing: options.routing,
    }),
    { beginRequest: options.beginRequest },
  );
  return function vercelFunction(incoming: NodeRequest, outgoing: NodeResponse): Promise<void> {
    const answered = listener(incoming, outgoing);
    platformWaitUntil()?.(answered);
    return answered;
  };
}
