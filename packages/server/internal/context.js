// @flow
//
// Internal to `@uniflowed/server`: the request a server function is inside.
//
// `headers()` and `cookies()` take no arguments, which is the whole point —
// a component nested six levels down should not have to be handed a request
// that every layer between it and the server has to thread through. That
// convenience needs somewhere to keep the request, and "somewhere" has exactly
// one safe answer on a server: storage scoped to the asynchronous call tree of
// the request being handled.
//
// A module-level variable would be wrong in a way that only shows up under
// load. `renderToString` is synchronous, so a variable set around it reads
// correctly — right up until a route awaits something, another request arrives
// while it is suspended, and the second request's headers are what the first
// one sees. `AsyncLocalStorage` is the primitive that does not have that bug,
// and Node, Deno and Bun all provide it under the `node:` specifier.
//
// This module is server-only by construction: nothing in `@uniflowed/server`
// is reachable from a client component, and `uf:rsc` classifies it that way.

import { AsyncLocalStorage } from "node:async_hooks";

import type { CacheOptions } from "./cache-store.js";
import type { ServerCapabilities } from "./capabilities.js";

/** A read-only view of one request's headers. */
export type HeaderStore = {
  readonly get: (name: string) => string | null,
  readonly has: (name: string) => boolean,
};

/** A read-only view of one request's cookies. */
export type CookieStore = {
  readonly get: (name: string) => string | null,
  readonly has: (name: string) => boolean,
};

/** Whether this request is rendering draft content, and how to change that. */
export type DraftMode = {
  readonly isEnabled: boolean,
  readonly enable: () => void,
  readonly disable: () => void,
};

/**
 * Everything a server function may ask about the request it is inside.
 *
 * Deliberately not the `Request` itself. A server function that could reach the
 * whole request could read the body, which is already being consumed by the
 * thing that called it, and could hold it past the response.
 */
export type RequestContext = {
  readonly headers: HeaderStore,
  readonly cookies: CookieStore,
  draft: boolean,
  /** Work deferred until the response has been sent. */
  readonly deferred: Array<() => mixed | Promise<mixed>>,
  /**
   * How many times this request has read state that varies per request.
   *
   * A counter rather than a flag, and the difference is what makes a route
   * cache possible at all. A middleware that reads a cookie to decide whether
   * to let the request through has not made the *page* vary — it either
   * answered or it did not — so a flag set by that read would refuse to cache
   * every page in every application that has an auth guard. A counter can be
   * read before the render and again after the last byte, and what moved in
   * between is exactly what the document depended on.
   *
   * Incremented by `../index.js`'s three bindings and by nothing else.
   * `after()` reads the context too and does not touch this: registering
   * deferred work says nothing about what the response contains.
   */
  requestStateReads: number,
  /**
   * The cache the host installed for this request, or `null`.
   *
   * On the context rather than in a module-level variable, which is the same
   * decision `storage` above is and rests on the same fact: two requests are
   * answered at once, and anything a server function reaches for by name has
   * to be scoped to the request or it is scoped to whichever request set it
   * last. It also means `revalidateTag()` in a server action reaches the store
   * that answered the request the action is part of, rather than a copy some
   * other module instance is holding — the hazard ubugeeei-prod/uf#389 is
   * about, pointed at a cache.
   */
  cache: CacheOptions | null,
  /**
   * What the host answering this request can do, or `null`.
   *
   * Beside the cache, and set the same way and at the same moment, because it
   * is the same kind of fact: something the *host* knows that a route handler
   * has no other way to ask about. `eventStream`, `upgradeWebSocket` and
   * `enqueue` all read it, and each refuses by naming the target rather than
   * failing as a dropped connection somewhere downstream.
   *
   * `null` means no host said, which is what every request looked like before
   * this field existed. The three readers treat it as "cannot", because the
   * alternative — assuming a host that says nothing can hold a socket open —
   * is the failure this is here to prevent.
   */
  capabilities: ServerCapabilities | null,
};

/**
 * One request, from the moment a host has one to the moment its bytes are gone.
 *
 * Two functions rather than one, because they are called from two places and
 * that is the whole point rather than an inconvenience. `run` wraps everything
 * that *decides* the response — the guard, the dispatcher, the render — and
 * `settle` happens after the response has been *written*, which in every host
 * uf has is a different line in a different module. A single
 * `handle(request, body)` that drained when `body` returned would be the bug
 * this exists to fix, spelled once instead of twice.
 */
export type RequestLifecycle = {|
  /** The context `run` establishes, for a host that needs to read it. */
  readonly context: RequestContext,
  /** Run the whole request inside it. */
  readonly run: <T>(body: () => Promise<T>) => Promise<T>,
  /** The response has gone: run what `after()` deferred. */
  readonly settle: () => Promise<void>,
|};

const storage: AsyncLocalStorage<RequestContext> = new AsyncLocalStorage();

/**
 * The context of the request being handled, or `null` outside one.
 *
 * `null` rather than throwing, so each caller can say what *it* needed the
 * request for — "cookies() was called outside a request" is a better error than
 * one generic message from here.
 */
export function currentContext(): RequestContext | null {
  return storage.getStore() ?? null;
}

/**
 * Whether a request has been established around this call.
 *
 * For a caller that is not a server function and has nothing to answer about
 * the request — the router's dispatcher and its middleware runner, which need
 * to know that a host established one *before* anything they call asks for
 * cookies. They must not be handed the context itself: a module that can reach
 * it can drain it, which is how the drain came to be in the wrong place.
 */
export function insideRequest(): boolean {
  return storage.getStore() != null;
}

/**
 * Run `body` with `context` as the current request.
 *
 * Everything `body` awaits sees the same context, and nothing outside it does.
 */
export function runWithContext<T>(context: RequestContext, body: () => T): T {
  return storage.run(context, body);
}

/**
 * Build a context from a `Request`.
 *
 * The header and cookie views are built once and read many times: a render
 * touches `cookies().get(…)` as often as it has components that care, and
 * re-parsing the cookie header each time would be the kind of cost nobody
 * looks for.
 *
 * Parsed on the first read rather than here, and that changed when the host
 * became the thing that begins a request: a host begins one before it knows
 * whether the path is an embedded chunk or a page, so every asset a compiled
 * binary serves now builds a context. Splitting a `Cookie` header for a
 * request that never asks about cookies is exactly the cost the paragraph
 * above refuses to pay per read, and there is no reason to pay it per request
 * either.
 */
export function contextFor(request: Request): RequestContext {
  const headers = request.headers;
  let cookies: { [string]: string } | null = null;
  const parsed = () => (cookies ??= parseCookies(headers.get("cookie")));

  return {
    headers: {
      get: (name) => headers.get(name),
      has: (name) => headers.has(name),
    },
    cookies: {
      get: (name) => (Object.hasOwn(parsed(), name) ? parsed()[name] : null),
      has: (name) => Object.hasOwn(parsed(), name),
    },
    draft: false,
    deferred: [],
    requestStateReads: 0,
    cache: null,
    capabilities: null,
  };
}

/**
 * Begin a request, and hand back the two halves of owning it.
 *
 * The one function a host calls. `contextFor`, `runWithContext` and
 * `drainDeferred` are still here because they are what this is made of and
 * because the suite drives them one at a time, but a *host* reaching for them
 * separately is how uf got two contexts on one request and a drain that ran
 * before the response: the middleware runner built one and drained it, and the
 * dispatcher underneath it built another. See ubugeeei-prod/uf#389.
 *
 * `settle` runs once. A host learns that a response is finished more than once
 * — the body stream closed, and then the socket did — and draining twice would
 * run whatever the first drain's callbacks registered, at a moment nothing
 * asked for.
 */
export function beginRequest(request: Request): RequestLifecycle {
  const context = contextFor(request);
  let settling: Promise<void> | null = null;

  function run<T>(body: () => Promise<T>): Promise<T> {
    return runWithContext(context, body);
  }

  function settle(): Promise<void> {
    settling ??= drainDeferred(context);
    return settling;
  }

  return { context, run, settle };
}

/**
 * Parse a `Cookie` header into a plain object.
 *
 * `Object.create(null)` rather than `{}`: a cookie called `__proto__` is a
 * thing an attacker can set, and on an ordinary object it would not be a key
 * at all — it would be the prototype.
 *
 * A duplicated name keeps the first value, which is what every server-side
 * cookie parser does and what browsers send for a name set at two paths.
 */
export function parseCookies(header: string | null): { [string]: string } {
  const out: { [string]: string } = Object.create(null);
  if (header == null || header === "") {
    return out;
  }

  for (const pair of header.split(";")) {
    const at = pair.indexOf("=");
    if (at < 0) {
      continue;
    }
    const name = pair.slice(0, at).trim();
    if (name === "" || Object.hasOwn(out, name)) {
      continue;
    }
    out[name] = decodeValue(pair.slice(at + 1).trim());
  }
  return out;
}

/**
 * Decode one cookie value, leaving it alone if it is not valid encoding.
 *
 * `decodeURIComponent` throws on a stray `%`, and a malformed cookie is not a
 * reason to fail a request — the value is simply not what the sender meant.
 */
function decodeValue(value: string): string {
  const unquoted =
    value.length >= 2 && value.startsWith('"') && value.endsWith('"') ? value.slice(1, -1) : value;
  try {
    return decodeURIComponent(unquoted);
  } catch {
    return unquoted;
  }
}

/**
 * Run everything `after()` deferred, in the order it was registered.
 *
 * A failure is reported and does not stop the rest: deferred work is by
 * definition not what the response depended on, and one broken analytics call
 * should not take the others with it.
 */
export async function drainDeferred(context: RequestContext): Promise<void> {
  const pending = context.deferred.splice(0, context.deferred.length);
  for (const task of pending) {
    try {
      await task();
    } catch (error) {
      reportDeferredFailure(error);
    }
  }
}

/**
 * Report a deferred task that threw.
 *
 * Isolated so a host can be given somewhere to put this; today it is the
 * console, which is where an unhandled rejection would have gone anyway.
 */
function reportDeferredFailure(error: mixed): void {
  // eslint-disable-next-line no-console
  console.error("uf: a task registered with after() failed", error);
}
