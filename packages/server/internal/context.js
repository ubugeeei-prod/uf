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
};

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
 */
export function contextFor(request: Request): RequestContext {
  const headers = request.headers;
  const cookies = parseCookies(headers.get("cookie"));

  return {
    headers: {
      get: (name) => headers.get(name),
      has: (name) => headers.has(name),
    },
    cookies: {
      get: (name) => (Object.hasOwn(cookies, name) ? cookies[name] : null),
      has: (name) => Object.hasOwn(cookies, name),
    },
    draft: false,
    deferred: [],
  };
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
