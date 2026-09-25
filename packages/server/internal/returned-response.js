// @flow
//
// One request on a host that hands the runtime a `Response` rather than
// writing a socket: `Bun.serve` (`../bun.js`) and `Deno.serve` (`../deno.js`).
//
// The counterpart of `../node.js`'s `nodeListener`, and it keeps the promise
// that module's header makes: `after()` runs once the response has gone. On
// Node that is a line uf writes, after the last byte. Here the runtime writes
// the bytes, so the line is where uf last sees them, the end of the body
// stream. The response is returned first, and the request is settled once
// that stream has been read to the end or cancelled.
//
// # Why not settle before returning
//
// That is what both hosts did until ubugeeei-prod/uf#1552, and it held every
// streamed response back until its deferred work was done. For most requests
// nothing is deferred, so nobody noticed. But the request that fills the route
// cache defers the entry being kept, and keeping it means draining the whole
// document (`../fetch.js`, `cachedDocument`). So under `bun server.js` and
// `deno run server.js` the filling request reached the runtime with its
// document already finished: a `$loading.js` fallback arrived together with
// the page it stood in for, while the same build streamed under
// `node server.js`. `after()` itself says "once the response has been sent",
// and awaiting it before sending was the opposite.

import type { RequestLifecycle } from "./context.js";
import type { Logger } from "./log.js";
import { elapsedMs, logRequest } from "../log.js";
import { Temporal } from "@uniflowed/core/temporal";

/**
 * One request: begun, answered inside its context, logged, and settled once
 * its body has gone.
 */
export async function answerReturned(
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
  }
  logRequest(log, {
    // A request that never got a context still gets a line; it gets an empty
    // id rather than a fabricated one, because inventing an id for a request
    // that had none would put a value in the log that nothing else in the
    // system has ever seen.
    requestId: lifecycle?.context.id ?? "",
    method: request.method.toUpperCase(),
    path: target,
    route: lifecycle?.context.route ?? null,
    status: response.status,
    durationMs: elapsedMs(started),
  });
  if (lifecycle == null) return response;
  // Nothing deferred yet: settled now, and the response goes back untouched.
  // That keeps a file answer the runtime's own: `new Response(Bun.file(…))`
  // is sent from the descriptor, and reading its `body` would turn it into a
  // stream through the heap (see `../bun.js`). A static file never defers
  // anything, because it is answered before the application runs.
  if (lifecycle.context.deferred.length === 0) {
    await lifecycle.settle();
    return response;
  }
  return settledAfterBody(response, lifecycle.settle);
}

/**
 * `response`, with `settle` started once its body has been read to the end,
 * has failed, or has been cancelled. With no body, it starts now.
 *
 * Started rather than awaited, which is the reason for doing this at all. The
 * body's end reaches the runtime without waiting for deferred work, and a
 * settle never rejects: `drainDeferred` reports each failing task and goes on
 * to the next.
 */
export function settledAfterBody(response: Response, settle: () => Promise<void>): Response {
  const body = response.body;
  if (body == null) {
    void settle();
    return response;
  }
  const reader = body.getReader();
  const read = new ReadableStream({
    async pull(controller: ReadableStreamDefaultController<Uint8Array>) {
      let next;
      try {
        next = await reader.read();
      } catch (error) {
        controller.error(error);
        void settle();
        return;
      }
      if (next.done) {
        controller.close();
        void settle();
        return;
      }
      controller.enqueue(next.value);
    },
    cancel(reason: mixed) {
      void settle();
      return reader.cancel(reason);
    },
  });
  return new Response(read, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
  });
}
