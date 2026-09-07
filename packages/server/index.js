// @flow
//
// `@uniflowed/server`: what a server function may ask about its request.
//
// Every binding here takes no arguments and answers about the request being
// handled, which is only possible because the renderer establishes a context
// around each one ([`./internal/context.js`]). Outside a request they throw,
// and each says what it was that had nowhere to look — a component that calls
// `cookies()` during a static prerender has made a mistake worth naming, not a
// mistake worth returning `null` for.
//
// This module is server-only. Nothing in it is reachable from a client
// component, `uf:rsc` classifies it that way, and importing it from one is the
// error that classification exists to produce.

import type { CookieStore, DraftMode, HeaderStore } from "./internal/context.js";
import { currentContext } from "./internal/context.js";

export type { CookieStore, DraftMode, HeaderStore } from "./internal/context.js";

/**
 * Raised when a server function is called with no request to answer about.
 *
 * Names the binding, because "no request context" on its own leaves a reader
 * hunting for which of the six things they called was the one out of place.
 */
export class OutsideRequestError extends Error {
  /** The binding that was called, e.g. `cookies`. */
  binding: string;

  constructor(binding: string) {
    super(
      `@uniflowed/server: ${binding}() was called outside a request. ` +
        "It answers about the request being handled, and there is not one here — " +
        "a static prerender, a module's top level, or a client component.",
    );
    this.name = "OutsideRequestError";
    this.binding = binding;
  }
}

/** The current request's context, or a named failure. */
function require$Context(binding: string) {
  const context = currentContext();
  if (context == null) {
    throw new OutsideRequestError(binding);
  }
  return context;
}

/**
 * The current request's context, counting this as a read of request state.
 *
 * The three bindings below that answer about *this* request go through this
 * one, and `after()` deliberately does not: registering deferred work says
 * nothing about what the response contains. What the count is for is the route
 * cache — `./fetch.js` reads it before the render and again after the last
 * byte, and stores the document only if nothing in between asked who was
 * asking. A page that read a cookie is a page about one person, and a page
 * about one person must not be served to the next person from a cache.
 *
 * Counted at the call rather than at the value, which is conservative in the
 * one direction that is safe: `const store = cookies()` followed by no `get`
 * counts, so such a render is re-rendered rather than cached. Slower, never
 * wrong — and the alternative, instrumenting the getters, would have to decide
 * what `has()` on a name that is absent means, which is a question with no
 * answer that is safe in both directions.
 */
function require$VaryingContext(binding: string) {
  const context = require$Context(binding);
  context.requestStateReads += 1;
  return context;
}

/**
 * The request's headers, read-only.
 *
 * Read-only because a response header set from inside a render has no defined
 * moment to take effect: the headers may already be on the wire by the time a
 * component deep in the tree renders.
 */
export function headers(): HeaderStore {
  return require$VaryingContext("headers").headers;
}

/**
 * The request's cookies, read-only.
 *
 * Setting a cookie belongs to a route handler or a server action, which run
 * before a response exists and can say so in it.
 */
export function cookies(): CookieStore {
  return require$VaryingContext("cookies").cookies;
}

/**
 * Whether this request is rendering draft content.
 *
 * The flag lives on the request rather than in a module, so two requests being
 * handled at once cannot see each other's answer.
 */
export function draftMode(): DraftMode {
  const context = require$VaryingContext("draftMode");
  return {
    isEnabled: context.draft,
    enable: () => {
      context.draft = true;
    },
    disable: () => {
      context.draft = false;
    },
  };
}

/**
 * Run `callback` once the response has been sent.
 *
 * For the work a request causes but a response does not wait on: recording a
 * view, flushing a metric, warming a cache. Registered work runs in the order
 * it was registered, and one task failing does not stop the others — deferred
 * work is by definition not what the response depended on.
 *
 * It is the small version of a queue and the line between them is durability,
 * not size. Deferred work lives in this process, is not written down anywhere,
 * and is gone when the process is — which is right for a metric and wrong for
 * anything a user would notice missing. `@uniflowed/server/queue` is the other
 * side of that line, and says what a deployment has to bring to it.
 *
 * "Sent" is the host's word to keep, and it keeps it: the request is drained
 * after `uf dev` has written the document, after `uf preview` and `uf start`
 * have returned from `send`, and after a compiled binary's `pipe` has resolved
 * on the last byte. One list per request, whether the callback was registered
 * by a middleware, a route handler or a page. It was not always so — see
 * ubugeeei-prod/uf#389 for what it meant before, and
 * `@uniflowed/server/host`'s `beginRequest` for the half a host supplies.
 */
export function after(callback: () => mixed | Promise<mixed>): void {
  require$Context("after").deferred.push(callback);
}
