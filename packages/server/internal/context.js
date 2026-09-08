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
import type { DraftChange } from "./draft.js";
import { DRAFT_COOKIE, draftKeyIsPerProcess, draftSetCookie, verifyDraftCookie } from "./draft.js";
import { processLogger } from "../log.js";

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
  /**
   * What to call this request in a log line, in a trace, and in an error page.
   *
   * On the context for the same reason everything else here is, and the reason
   * is sharper for an id than for anything above it: an id whose whole purpose
   * is to tell two requests apart, kept in a module-level variable, would name
   * whichever request set it last. Every line the first request wrote after the
   * second one arrived would carry the second one's id, and the log would be
   * wrong in a way that reads as though it were right.
   *
   * uf generates it and never takes it from the request. An `X-Request-Id` a
   * client sent is text that client chose: it can be the same on a million
   * requests, which defeats the one thing an id is for; it can be a megabyte;
   * and it lands in a log line, which is a place `./log.js` spends its header
   * explaining that attacker-chosen text does not belong. A deployment behind
   * a proxy that already assigns ids has a real need here and it is not this
   * field — it is a second, clearly-named one that says whose id it is, and it
   * is not in this change.
   */
  readonly id: string,
  /**
   * The route pattern that claimed this request, or `null` if none has.
   *
   * `/orders/:id` rather than `/orders/8813`. Written by whichever part of
   * `@uniflowed/router` matched — the handler dispatcher or the renderer —
   * through [`noteRoute`], and read by the host when it writes the request's
   * log line. Mutable because it is not known when the request begins: a host
   * establishes the request before anything has looked at the path.
   */
  route: string | null,
  /**
   * Whether this request is rendering draft content.
   *
   * Read from the request's signed `__Host-uf.draft` cookie by [`beginRequest`]
   * before anything the host asked for runs, and settable afterwards by
   * `../index.js`'s `draftMode()` — so a guard, a route handler and the page
   * underneath them all agree, and a request that arrived with the cookie is in
   * draft mode from its first line rather than from whenever something happened
   * to call `enable()`. It was initialised `false` and never read from the
   * request at all, which made draft mode a feature that could not be turned
   * on: ubugeeei-prod/uf#282.
   */
  draft: boolean,
  /**
   * What this request decided about draft mode, for a responder to write.
   *
   * `null` until `draftMode().enable()` or `.disable()` is called. It is
   * separate from `draft` because the two are different facts: `draft` is what
   * *this* request renders, and this is the instruction to change what the
   * *next* one does — a `Set-Cookie` that only the thing producing the response
   * can write. [`asResponder`] is what turns it into one, and clears it.
   */
  draftChange: DraftChange | null,
  /**
   * What is producing this request's response, or `null`.
   *
   * A name, e.g. `a route handler`, set by [`asResponder`] around the call into
   * whatever owns the response. `draftMode().enable()` refuses when it is
   * `null`, and that refusal is the whole reason this field exists: a component
   * that set a cookie would be setting it at a moment with no defined meaning,
   * because the headers may already be on the wire by the time a component six
   * levels down renders. `../index.js` gives the same argument for `headers()`
   * being read-only, and draft mode is the exception that needs somewhere to
   * put the response half.
   */
  responder: string | null,
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
    id: newRequestId(),
    route: null,
    // `false` here and resolved in `beginRequest`, because deciding it means
    // verifying a signature and `crypto.subtle` is asynchronous while this
    // function is not. A caller that builds a context by hand and never runs it
    // gets a request that is not in draft mode, which is the safe direction of
    // being wrong.
    draft: false,
    draftChange: null,
    responder: null,
    deferred: [],
    requestStateReads: 0,
    cache: null,
    capabilities: null,
  };
}

/**
 * A name for one request.
 *
 * Generated eagerly rather than on the first read, which is the opposite of the
 * decision two paragraphs above about the cookie header, and for a reason that
 * survives being stated: nothing reads the cookies of a request for a
 * stylesheet, and *everything* reads the id — the host writes it into the
 * request's log line whether or not the application ever asked. A lazy value
 * that is always used is a branch and a nullable field bought for nothing.
 *
 * `crypto.randomUUID` because every runtime this package runs on has Web Crypto
 * as a global: Node since 19, Deno, Bun, and every worker runtime. Reaching for
 * `node:crypto` instead would put an import in a module `../edge.js` bundles
 * for a platform that has no such thing.
 *
 * It is random rather than a counter, and that is a decision rather than
 * laziness. A counter is smaller and sorts, and it also says how many requests
 * a process has served and lets one id be guessed from another — and ids end up
 * in error pages shown to whoever hit the error, which is the wrong place to
 * publish a traffic figure.
 */
function newRequestId(): string {
  return crypto.randomUUID();
}

/**
 * Record that `pattern` is the route this request turned out to be.
 *
 * Silent outside a request, and that is what makes it callable from the
 * router's own matching code: `prerender` resolves routes at build time, where
 * there is no request and nothing to record, and a function that threw there
 * would push the check into every caller.
 *
 * Last writer wins, and the writers are chosen so that this means "the most
 * specific thing that claimed the request". `@uniflowed/router`'s dispatcher
 * calls it for a route handler and its renderer calls it for a page; the
 * middleware runner deliberately does not. A guard covers a directory —
 * `/dashboard` covers `/dashboard/typo`, which is a 404 — so recording the
 * guard's path would label a request with a route it never reached.
 */
export function noteRoute(pattern: string): void {
  const context = storage.getStore();
  if (context != null) {
    context.route = pattern;
  }
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

  /**
   * Everything the host asked for, with `draft` already decided.
   *
   * The draft cookie is verified here rather than in `contextFor` because
   * verifying it is an HMAC and `crypto.subtle` is asynchronous — and here
   * rather than in each of the three router entry points because a request has
   * one answer to "is this a draft request", not one per module that asks.
   *
   * The cookie is looked up before anything is awaited, so a request that does
   * not carry one — which is every request on an ordinary site — runs `body`
   * synchronously exactly as it did before this existed, with no extra
   * microtask between the host and the guard.
   */
  function run<T>(body: () => Promise<T>): Promise<T> {
    return runWithContext(context, () => {
      const carried = context.cookies.get(DRAFT_COOKIE);
      if (carried == null) {
        return body();
      }
      return verifyDraftCookie(carried).then((valid) => {
        context.draft = valid;
        return body();
      });
    });
  }

  function settle(): Promise<void> {
    settling ??= drainDeferred(context);
    return settling;
  }

  return { context, run, settle };
}

/**
 * Run `body` as the thing that owns this request's response.
 *
 * Two things at once, and they are the same thing seen from both ends.
 * `draftMode().enable()` refuses outside this scope — a render has no defined
 * moment at which a response header takes effect — and inside it, whatever
 * `body` decided about draft mode is written onto the `Response` it returned.
 *
 * `@uniflowed/router` calls it in exactly two places: around a route handler
 * and around a server action. Those are the two things Next allows `enable()`
 * in and the two things uf can honestly allow it in, because they are the two
 * that return a response of their own. A middleware is deliberately not one:
 * it may decline, and a guard that turned draft mode on and then let the
 * request through would have made a decision with nowhere to be written —
 * silently, which is the failure ubugeeei-prod/uf#282 is about wearing a
 * different hat.
 *
 * Silent outside a request, and that matters for the same reason `noteRoute`
 * is: `dispatch` refuses outside one already, and a second refusal here would
 * only replace a good message with a worse one.
 */
export async function asResponder(kind: string, body: () => Promise<Response>): Promise<Response> {
  const context = storage.getStore();
  if (context == null) {
    return body();
  }
  const outer = context.responder;
  context.responder = kind;
  let response: Response;
  try {
    response = await body();
  } finally {
    context.responder = outer;
  }
  const change = context.draftChange;
  if (change == null) {
    return response;
  }
  // Cleared before the cookie is built rather than after it: two responders on
  // one request — an action that answered, and nothing else — must not both
  // write the header, and a `Set-Cookie` written twice is a browser told two
  // things about one name.
  context.draftChange = null;
  if (change === "enable" && draftKeyIsPerProcess()) {
    warnAboutGeneratedKey();
  }
  return withSetCookie(response, await draftSetCookie(change));
}

/** Whether the per-process-key warning has already been written. */
let warnedAboutGeneratedKey = false;

/**
 * Say once that draft mode is keyed with a secret this process invented.
 *
 * Once per process, and only when a cookie is actually issued: a deployment
 * that never uses draft mode has nothing to be told, and a line per preview
 * link would be a line per request in a CMS's hands.
 *
 * `warn` rather than `error`, because it is exactly right for `uf dev` and
 * exactly wrong for two containers behind a load balancer, and this module
 * cannot tell which it is in.
 */
function warnAboutGeneratedKey(): void {
  if (warnedAboutGeneratedKey) {
    return;
  }
  warnedAboutGeneratedKey = true;
  processLogger().warn(
    "draft mode is signed with a secret generated for this process: the cookie stops " +
      "working when it restarts, and another instance will not accept it. Set " +
      "UF_DRAFT_SECRET to a shared value of at least 32 bytes.",
  );
}

/**
 * `response` with one more `Set-Cookie` on it.
 *
 * Tried in place first because that keeps everything a `Response` can be that a
 * reconstruction cannot: a `101` from an upgrade, which the `Response`
 * constructor refuses outright, and a body that is already being consumed. A
 * `Response.redirect` — which is what the preview flow returns — has an
 * immutable header guard and throws instead, and the copy is for that one. The
 * guard is not observable except by trying, which is why this is a `catch`
 * rather than a test.
 */
function withSetCookie(response: Response, cookie: string): Response {
  try {
    response.headers.append("set-cookie", cookie);
    return response;
  } catch {
    const copy = new Response(response.body, {
      status: response.status,
      statusText: response.statusText,
      headers: response.headers,
    });
    copy.headers.append("set-cookie", cookie);
    return copy;
  }
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
