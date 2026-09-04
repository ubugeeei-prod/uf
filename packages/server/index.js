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
 * The request's headers, read-only.
 *
 * Read-only because a response header set from inside a render has no defined
 * moment to take effect: the headers may already be on the wire by the time a
 * component deep in the tree renders.
 */
export function headers(): HeaderStore {
  return require$Context("headers").headers;
}

/**
 * The request's cookies, read-only.
 *
 * Setting a cookie belongs to a route handler or a server action, which run
 * before a response exists and can say so in it.
 */
export function cookies(): CookieStore {
  return require$Context("cookies").cookies;
}

/**
 * Whether this request is rendering draft content.
 *
 * The flag lives on the request rather than in a module, so two requests being
 * handled at once cannot see each other's answer.
 */
export function draftMode(): DraftMode {
  const context = require$Context("draftMode");
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
 */
export function after(callback: () => mixed | Promise<mixed>): void {
  require$Context("after").deferred.push(callback);
}
