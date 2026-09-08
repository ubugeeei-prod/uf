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

import type { CookieStore, DraftMode, HeaderStore, RequestContext } from "./internal/context.js";
import { currentContext } from "./internal/context.js";
import { DraftModeError } from "./internal/draft.js";
import type { LogFields, Logger } from "./internal/log.js";
import { processLogger } from "./log.js";

export type { CookieStore, DraftMode, HeaderStore } from "./internal/context.js";
export type { LogFields, LogLevel, Logger } from "./internal/log.js";
export { DraftModeError } from "./internal/draft.js";

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
 * Whether this request is rendering draft content, and how to change that.
 *
 * The flag lives on the request rather than in a module, so two requests being
 * handled at once cannot see each other's answer. It is read from a signed
 * cookie when the request begins, so a guard, a route handler and the page
 * underneath them agree about it — and so that it is `true` at all, which it
 * never was before ubugeeei-prod/uf#282: nothing wrote the flag to a response
 * and nothing read it from one, so `enable()` mutated an object that was
 * discarded when the response was sent.
 *
 * # `enable()` is a route handler's to call, or an action's
 *
 * It writes a cookie, and a cookie is part of a response. `headers()` and
 * `cookies()` above are read-only for the reason in their own paragraphs — a
 * response header set from inside a render has no defined moment to take
 * effect — and draft mode is the one case that needs the exception, so the
 * exception is given exactly where a response is being produced and refused
 * everywhere else with [`DraftModeError`]. That is Next's rule and it is Next's
 * reason; what differs is that uf can name the two places in the error.
 *
 * The flow it exists for is one route handler:
 *
 *     // app/api/preview/_uf.route.js
 *     export function GET(request: Request): Response {
 *       const url = new URL(request.url);
 *       if (url.searchParams.get("token") !== process.env.CMS_PREVIEW_TOKEN) {
 *         return new Response("no", { status: 401 });
 *       }
 *       draftMode().enable();
 *       return Response.redirect(new URL(url.searchParams.get("to") ?? "/", request.url), 307);
 *     }
 *
 * uf checks that the cookie it later receives is one it issued and has not
 * expired. It does **not** check who asked for it: the handler above is the
 * authorization, and the token comparison in it is the application's to write,
 * because only the application knows what a CMS editor is. A handler that
 * enables draft mode with no check is an open door, and this documentation is
 * the only place that can say so.
 *
 * # What it changes
 *
 * A draft request is never answered from the route cache and never from a
 * prerendered document on disk, because both are answers about a moment before
 * the draft existed. `packages/server/fetch.js` has the first half and
 * `./node.js`'s static handler the second.
 */
export function draftMode(): DraftMode {
  const context = require$VaryingContext("draftMode");
  return {
    isEnabled: context.draft,
    enable: () => {
      requireResponder(context, "enable");
      context.draft = true;
      context.draftChange = "enable";
    },
    disable: () => {
      requireResponder(context, "disable");
      context.draft = false;
      context.draftChange = "disable";
    },
  };
}

/**
 * Refuse a draft-mode change made where no response is being produced.
 *
 * The message distinguishes the two ways of being in the wrong place, because
 * they have different fixes: a render has to move the call into a handler, and
 * a middleware has to answer with a redirect to one.
 */
function requireResponder(context: RequestContext, operation: string): void {
  if (context.responder != null) {
    return;
  }
  throw new DraftModeError(operation, "was called where nothing owns the response");
}

/**
 * What this request is called, everywhere it is mentioned.
 *
 * The same string the host puts in the request's log line, so a page that
 * renders it into an error message gives whoever hit the error something they
 * can quote and an operator something they can search for. It is created when
 * the request arrives and is readable from a loader, from a route handler and
 * from a server component's render, without any of them being handed a request
 * — which is only possible because it lives on the request context rather than
 * in a module, and is the whole of ubugeeei-prod/uf#506's harder half.
 *
 * # It counts as reading request state, and `logger()` does not
 *
 * This one goes through `require$VaryingContext`, which is the difference
 * between the two bindings and is not a detail. A component that renders the
 * request id has rendered a document that is true of exactly one request; a
 * route cache that stored it would answer every later visitor with the first
 * one's id, which is the same failure as a cached `Set-Cookie` in a smaller
 * hat. So reading it makes the render uncacheable, exactly as `cookies()` does.
 *
 * `logger()` below does not count, because the id it binds goes into a log line
 * rather than into the document, and a page that logs must not thereby become a
 * page uf refuses to cache.
 */
export function requestId(): string {
  return require$VaryingContext("requestId").id;
}

/**
 * Somewhere to say something about this request.
 *
 * The process logger — whatever `@uniflowed/server/log`'s `installLogger` was
 * last given — with this request's id and matched route already bound, so a
 * line written from six levels down inside a render can be joined to the
 * request that caused it without anybody threading anything through.
 *
 * # It does not throw outside a request
 *
 * Every other binding in this module does, and the argument for that is in the
 * header: they answer *about* a request, and outside one there is no honest
 * answer to give. A logger is the exception because outside a request there is
 * an honest answer — the same logger, writing the same line, without a request
 * id on it. The alternative is a package whose logging call is the one call you
 * cannot make from the code that handles a failure, which is where logging is
 * worth the most.
 *
 * The route is read at the moment a line is written rather than bound once, and
 * so is the process logger. Both change under a caller that is holding one of
 * these: a loader logs before the render has begun and a component logs after
 * the router has matched, so a route captured at construction would be `null`
 * on lines of a request whose route is perfectly well known — and a host that
 * calls `installLogger` after something has already taken a logger would
 * otherwise be sending part of its output to the sink it replaced.
 */
export function logger(): Logger {
  const context = currentContext();
  return context == null ? processLogger() : boundLogger(context, {});
}

/**
 * The process logger with this request's fields on it, resolved per line.
 *
 * Recursive, so a `child` of it is still one of these rather than a plain child
 * of the process logger with the route frozen into it at the moment `child` was
 * called.
 */
function boundLogger(context: RequestContext, extra: LogFields): Logger {
  const fields = () => ({
    requestId: context.id,
    ...(context.route == null ? {} : { route: context.route }),
    ...extra,
  });
  return {
    // Read now rather than per line: a threshold is what a caller checks to
    // decide whether building a field is worth it, and one that changed under
    // them would make that check meaningless.
    level: processLogger().level,
    debug: (message, more) => processLogger().debug(message, { ...fields(), ...more }),
    info: (message, more) => processLogger().info(message, { ...fields(), ...more }),
    warn: (message, more) => processLogger().warn(message, { ...fields(), ...more }),
    error: (message, more) => processLogger().error(message, { ...fields(), ...more }),
    child: (more) => boundLogger(context, { ...extra, ...more }),
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
