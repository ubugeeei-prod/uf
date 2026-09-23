// @flow
//
// `@uniflowed/server/adapter`: the contract a deploy target implements, as
// types a platform outside this repository can build against.
//
// uf ships six server adapters, and every one of them is the same two things:
// the application as a `Request` → `Response` function (`handler.js`), and a
// handful of lines that put that function on a platform (`server.js`,
// `worker.js`, `lambda.js`). The first is written by `uf build` and is
// byte-for-byte the same for every target; the second is all an adapter is.
// A platform that wants to run uf applications writes the second half and
// nothing else — and this module is what that half is written against.
//
// The whole contract, with the order a host has to keep and the reasons, is
// `docs/app/guide/deploy/$page.mdx` under "The adapter contract". What is here
// is the part a type checker can hold a host to, and the one check that is
// worth making at run time: that the module a host loaded is a uf handler at
// all, named field by field, rather than a `TypeError` on the first request.
//
// # What a host receives
//
// The directory `uf build --adapter node` writes (or `edge`, for a runtime
// with Web-standard APIs and no Node built-ins):
//
//   handler.js      the application; see [`HandlerModule`]
//   static/         the output directory: hashed assets, prerendered
//                   documents, the previous build's hashed assets for one
//                   build (see version skew in the deploy guide)
//   chunks/         what `handler.js` imports, if the bundle split
//   package.json    `{"type": "module"}`, so `.js` is an ES module
//
// # What a host owes it
//
// Six obligations; the guide argues each. In order, for every request:
//
// 1. **The files first.** A `GET` or `HEAD` for a file under `static/` is
//    answered with it, before any application code runs. Never for another
//    method. `@uniflowed/server/node`'s `createStaticHandler` is the reference
//    decision, and `@uniflowed/server/edge`'s `createWorkerFetch` is the same
//    order for a host whose files are a binding.
// 2. **`routing`'s redirects and headers in front of the files**, when the
//    host has a static half of its own; `fetch` applies them itself for a
//    request that reaches it.
// 3. **One request, begun by the handler's own `beginRequest`.** Never a copy
//    from another installation of this package; the reason is on
//    [`HandlerModule`].
// 4. **`fetch` inside `run`**, and **`settle` after the body has been sent** —
//    or handed to the platform's `waitUntil`, where the platform has one.
// 5. **Every `Set-Cookie` kept separate**, and a body streamed as it is
//    produced where the platform can.
// 6. **A bare `500` for a thrown error**, with nothing of the error in the
//    body.
//
// And then the conformance check: deploy the served-app fixture with the new
// adapter, run `uf start` over the same build, and point
// `tools/ci/deployed-parity.mjs` at both. An adapter that passes it answers
// every question the way the six in this repository do.

import type { RequestLifecycle } from "./internal/context.js";
import type { RoutingRules } from "./internal/routing.js";
import { processLogger } from "./log.js";

export type { RequestLifecycle } from "./internal/context.js";
export type { RoutingRules } from "./internal/routing.js";

/** The file in an artefact that is the application. */
export const HANDLER_FILE = "handler.js";

/** The directory in an artefact that holds the files a host serves first. */
export const STATIC_DIRECTORY = "static";

/**
 * What `handler.js` exports, as its default export and as named exports.
 *
 * `fetch` is the application: middleware, server actions, payloads, route
 * handlers and rendering, in that order, over a `Request`. It touches no
 * filesystem and holds no socket, so it runs on any runtime that has `Request`,
 * `Response` and `ReadableStream`. It refuses a request that names another
 * build with a `409` on its own — a host has nothing to do for version skew
 * beyond serving its files first.
 *
 * `beginRequest` is the handler's own, and a host must use this one: the
 * request store belongs to the copy of `@uniflowed/server` bundled into the
 * application, and a request begun through any other copy is one the
 * application cannot see — `cookies()` would throw inside every page.
 *
 * `routing` is `app.router`'s redirects, rewrites, headers, base path and
 * trailing-slash policy, as the build read them, for a host that puts them in
 * front of its own files. Absent means none.
 */
export type HandlerModule = {|
  +fetch: (request: Request) => Promise<Response>,
  +beginRequest: (request: Request) => RequestLifecycle,
  +routing?: RoutingRules,
|};

/**
 * One request through the application, the way every host in this repository
 * answers it: begun, run, and settled once the caller says the body is out.
 *
 * `sent` is the host's line for "the response has been written" — the end of
 * a Node response, or the platform's `waitUntil`. It is handed the promise of
 * the settle rather than awaited here, because only the host knows where that
 * line is.
 */
export type HostAnswer = (
  request: Request,
  sent: (settled: () => Promise<void>) => void,
) => Promise<Response>;

/**
 * The module a host loaded, checked to be a uf handler.
 *
 * Throws naming every field that is missing or of the wrong kind, so a host
 * that loaded the wrong file — the wrapper instead of the handler, or a build
 * from before `beginRequest` was exported — fails when it starts rather than on
 * its first request.
 */
export function handlerModule(loaded: mixed): HandlerModule {
  const candidate: mixed =
    loaded != null && typeof loaded === "object" && loaded.default != null
      ? loaded.default
      : loaded;
  const problems: Array<string> = [];
  if (candidate == null || typeof candidate !== "object") {
    throw new TypeError(
      `@uniflowed/server/adapter: ${HANDLER_FILE} exported ${String(candidate)}, not a uf ` +
        "handler. Load the handler.js `uf build --adapter` wrote, not the entry beside it.",
    );
  }
  const { fetch, beginRequest, routing } = candidate;
  if (typeof fetch !== "function") problems.push("`fetch` is not a function");
  if (typeof beginRequest !== "function") problems.push("`beginRequest` is not a function");
  if (routing != null && typeof routing !== "object") problems.push("`routing` is not an object");
  if (problems.length > 0 || typeof fetch !== "function" || typeof beginRequest !== "function") {
    throw new TypeError(
      `@uniflowed/server/adapter: ${HANDLER_FILE} is not a uf handler: ${problems.join("; ")}. ` +
        "Rebuild with `uf build --adapter`.",
    );
  }
  // $FlowFixMe[incompatible-type] each field was checked above; `mixed` cannot say so.
  const checked: HandlerModule = candidate;
  return checked;
}

/**
 * Obligations 3 and 4 over a handler: begin the request with the handler's own
 * `beginRequest`, run `fetch` inside it, and hand the settle to the host.
 *
 * The files (obligations 1 and 2) are the host's, before this; the cookies and
 * the streaming (5) are how the host writes what this returns; and a thrown
 * error (6) is answered here with the bare `500` every host in this repository
 * writes, so the host has one shape to handle rather than two.
 */
export function answerWith(handler: HandlerModule): HostAnswer {
  return async function answer(request, sent) {
    const { run, settle } = handler.beginRequest(request);
    let response: Response;
    try {
      response = await run(() => handler.fetch(request));
    } catch (error) {
      processLogger().error("request failed", { error });
      response = new Response("500 Internal Server Error\n", {
        status: 500,
        headers: { "content-type": "text/plain; charset=utf-8" },
      });
    }
    sent(settle);
    return response;
  };
}
