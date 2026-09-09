// @flow
//
// `@uniflowed/server/cache`: the cache, and the two ways to invalidate it.
//
// `uf.config.js` has had `rendering.cache` in it for a long time, and until now
// the four switches under it were read once, copied into
// `dist/uf-build-manifest.json` and read by nothing. Two of them mean something
// from here on — `route` and `fetch` — and two of them do not, which is stated
// rather than implied: `data` and `actions` are refused by the configuration
// loader, by name, because a key that accepts `true` and changes nothing is
// worse than a key that is not there. See ubugeeei-prod/uf#277.
//
// # What is here
//
// * [`createCacheStore`] — the store. Its contract is written out in
//   `./internal/cache-store.js`: what a key is, what an entry is, when an entry
//   is stale, who evicts, and what happens to a request that arrives while an
//   entry is being filled. Read that before this.
// * [`cacheLife`] and [`cacheTag`] — what a render says about the entry it is
//   filling, from inside the render.
// * [`noStore`] — what it says when the answer must not be kept.
// * [`revalidateTag`] and [`revalidatePath`] — what a server action or a route
//   handler says when the data changed.
// * [`createCachedFetch`] — the fetch cache, as a wrapper.
//
// # What is not here, and will not be quietly added
//
// **No entry is stored without a stated lifetime.** There is no "cache forever"
// and no default lifetime. A route that never calls [`cacheLife`] is rendered
// for every request exactly as it is today, whatever `rendering.cache.route`
// says — the switch decides whether a *stated* lifetime is honoured, not
// whether uf starts keeping documents nobody asked it to keep.
//
// **A render that read the request is never stored.** `cookies()`, `headers()`
// and `draftMode()` are how a document comes to be about one person, and a
// document about one person in a cache shared by every person is the worst bug
// a framework can have. `./fetch.js` counts those reads across the whole render
// and refuses to store when the count moved. It is a runtime refusal today;
// `crates/uf_rsc` already answers "is this call reachable from here" for
// server-only imports, and turning the same question on a cached scope is what
// would make it a build error instead. #277 argues that, and it is not done.
//
// # Where it lives
//
// In memory, in one process, unless a host gives the store somewhere else to
// keep things. That was the whole truth once — four processes behind a load
// balancer held four of these and disagreed, a restart emptied one, and
// `revalidateTag` in one did not reach the other three — and the fix this
// header named is now written: **a durable store behind `resolve`**.
//
// `./internal/cache-provider.js` is the seam, five methods over strings.
// `./cache-filesystem.js` is one implementation of it, and a deployment adapter
// with a Redis or a KV namespace writes another without uf naming either. A
// store handed one keeps entries across a restart, shares them between
// processes, and takes an invalidated tag out of the store all of them fill
// from — which, with the `revalidate`/`expire` window and the on-demand
// invalidation that were already here, is incremental static regeneration.
//
// Two things stay exactly as they were, and both are deliberate.
//
// **Nothing caches without a stated lifetime**, durable or not. Persistence is
// a second opt-in on top of the first and never a way around it.
//
// **A durable store needs the identity of the build that filled it.**
// `./internal/cache-key.js` argues why at length: a durable entry outlives the
// build, so the build is the first member of every durable key, and a store
// constructed with a provider and no `build` is refused rather than allowed to
// serve the last deployment's documents.
//
// What is still not here is **prerendering into it** — `uf build` filling the
// store so the first request to a cold URL is a read rather than a render. See
// the guide.

import type {
  CacheLifetime,
  CacheOptions,
  CacheRequest,
  CacheStoreOptions,
} from "./internal/cache-store.js";
import { CacheStore, currentScope } from "./internal/cache-store.js";
import type { CacheKey } from "./internal/cache-key.js";
import { currentContext } from "./internal/context.js";

export type { CacheKey } from "./internal/cache-key.js";
export type {
  CacheEntry,
  CacheLifetime,
  CacheOptions,
  CacheOutcome,
  CacheRequest,
  CacheResult,
  CacheStats,
  CacheStoreOptions,
} from "./internal/cache-store.js";
export { CacheStore } from "./internal/cache-store.js";

// The durable seam, re-exported from the package's front door for the cache so
// that an adapter writing a provider imports one module and not two — and, more
// to the point, so that "what a provider is" is part of the public surface
// rather than something reached by path into `internal/`. `encodeCacheValue`
// comes with them because a provider that wants to store something other than
// a string — a Redis hash, a KV entry with metadata — still has to agree with
// uf about what an entry *is*.
export type { CacheProvider, DurableCacheEntry } from "./internal/cache-provider.js";
export {
  UnserialisableCacheValueError,
  decodeCacheValue,
  encodeCacheValue,
} from "./internal/cache-provider.js";

/**
 * Raised when something that only means anything inside a cached fill is called
 * outside one.
 *
 * Names the binding, for the same reason `OutsideRequestError` does: "no cache
 * scope" leaves a reader hunting for which call was the one out of place.
 */
export class OutsideCacheScopeError extends Error {
  /** The binding that was called, e.g. `cacheTag`. */
  binding: string;

  constructor(binding: string) {
    super(
      `@uniflowed/server: ${binding}() was called outside a cached scope. ` +
        "It states something about the entry being filled, and nothing is being filled here — " +
        "a module's top level, a client component, or a request answered by a host that " +
        "established no render scope.",
    );
    this.name = "OutsideCacheScopeError";
    this.binding = binding;
  }
}

/** Raised when the cache is asked about from outside a request that has one. */
export class OutsideCachedRequestError extends Error {
  /** The binding that was called, e.g. `revalidateTag`. */
  binding: string;

  constructor(binding: string) {
    super(
      `@uniflowed/server: ${binding}() needs the cache the host installed for this request, ` +
        "and there is not one here. Either this is outside a request, or the host answered it " +
        "without passing `cache` to createFetchHandler — which is what `rendering.cache` in " +
        "uf.config.js turns on.",
    );
    this.name = "OutsideCachedRequestError";
    this.binding = binding;
  }
}

/**
 * A cache.
 *
 * A function rather than the constructor as the front door, so the store can
 * grow a second implementation — a durable one, from an adapter — without every
 * caller having named a class. That second implementation turned out to be an
 * option rather than a class: pass `provider` and `build` and the same store
 * keeps its entries where the provider puts them.
 */
export function createCacheStore(options?: CacheStoreOptions): CacheStore {
  return new CacheStore(options);
}

/**
 * How long the entry this render is filling stays usable.
 *
 * Called from inside the render, by whatever knows the answer — a loader knows
 * how often its data changes and the handler above it does not. Called twice,
 * the *shorter* lifetime wins: a page composed of a thing that changes hourly
 * and a thing that changes by the minute is a page that changes by the minute,
 * and taking the longer one would serve the fast half stale for an hour.
 */
export function cacheLife(lifetime: CacheLifetime): void {
  const scope = currentScope();
  if (scope == null) {
    throw new OutsideCacheScopeError("cacheLife");
  }
  const held = scope.lifetime;
  if (held == null || lifetime.revalidate < held.revalidate) {
    scope.lifetime = lifetime;
  }
}

/**
 * Label the entry this render is filling, so `revalidateTag` can reach it.
 *
 * Tags are what make invalidation a statement about meaning rather than about
 * spelling: a mutation says "posts changed" and every entry that read a post
 * goes, without the mutation knowing which URLs those were.
 */
export function cacheTag(...tags: $ReadOnlyArray<string>): void {
  const scope = currentScope();
  if (scope == null) {
    throw new OutsideCacheScopeError("cacheTag");
  }
  for (const tag of tags) {
    if (typeof tag !== "string" || tag === "") {
      throw new TypeError(
        "@uniflowed/server: a cache tag is a non-empty string; " +
          `cacheTag received ${JSON.stringify(tag)}.`,
      );
    }
    scope.tags.push(tag);
  }
}

/**
 * Refuse to store the entry this render is filling.
 *
 * `reason` is kept because the interesting question about an uncached page is
 * never "is it cached" but "why is it not", and the answer is usually six
 * frames down in somebody else's module. The first reason wins: what stopped an
 * answer being stored is the first thing that did.
 */
export function noStore(reason: string = "noStore() was called"): void {
  const scope = currentScope();
  if (scope == null) {
    throw new OutsideCacheScopeError("noStore");
  }
  scope.denied ??= reason;
}

/** The cache the host installed for this request, or a named failure. */
function require$Cache(binding: string): CacheOptions {
  const cache = requestCache();
  if (cache == null) {
    throw new OutsideCachedRequestError(binding);
  }
  return cache;
}

/**
 * Expire every entry filled under `tag`. Answers how many went **here**.
 *
 * Expired, not marked stale: a tag is invalidated because somebody changed the
 * thing it names, so the entry is known wrong rather than possibly old. See
 * `./internal/cache-store.js`, which argues the difference.
 *
 * The number is this process's, and with a durable store that is a smaller
 * number than the invalidation. The store this process shares with the other
 * three loses every entry carrying the tag; what this counts is the copies in
 * *this* process's memory, because that is the only quantity available without
 * making a mutation handler wait on a disk to find out an integer it is going
 * to put in a log. Nothing is lost by that: the durable half is finished before
 * the request is, which is what [`carryDurableWork`] arranges.
 *
 * With no durable store this is exactly what it always was — a count of what
 * one process forgot, in a deployment where that is all there is to forget.
 */
export function revalidateTag(tag: string): number {
  const store = require$Cache("revalidateTag").store;
  const dropped = store.revalidateTag(tag);
  carryDurableWork(store);
  return dropped;
}

/** Expire every entry filled for `path`. Answers how many went here. */
export function revalidatePath(path: string): number {
  const store = require$Cache("revalidatePath").store;
  const dropped = store.revalidatePath(path);
  carryDurableWork(store);
  return dropped;
}

/**
 * Make the request wait for the durable half of what just happened.
 *
 * A durable invalidation is started and not awaited — see the store — which is
 * right for a long-lived server and is a dropped write on a host that stops the
 * process the moment a response is written. Every serverless target is one of
 * those, and it is the target where a shared cache matters most, so the work is
 * handed to the request: `after()` runs at `settle()`, which is where a worker
 * hands `ctx.waitUntil` its promise and where `./node.js` waits for the bytes.
 *
 * Registered here rather than inside the store because the store deliberately
 * does not know what a request is — `./internal/cache-store.js`'s note on
 * `refresh` gives that reason and this respects it. Outside a request there is
 * nothing to register with and nothing to do: a caller holding a store directly
 * has `store.settled()`.
 *
 * `./fetch.js` registers the same thing once per request, because a route fill
 * writes durably without anybody calling this. Both registrations landing is
 * not a problem worth code to avoid: `settled()` is idempotent and resolves
 * without yielding when there is nothing outstanding.
 */
function carryDurableWork(store: CacheStore): void {
  const context = currentContext();
  if (context == null) {
    return;
  }
  context.deferred.push(() => store.settled());
}

/** The cache answering this request, or `null` where the host installed none. */
export function requestCache(): CacheOptions | null {
  return currentContext()?.cache ?? null;
}

/** A request, plus what it says about caching itself. */
export type CachedRequestOptions = {
  readonly method?: string,
  readonly searchParams?: { readonly [string]: string | number | boolean },
  /** Absent means "do not cache this", which is the default and stays it. */
  readonly cache?: FetchCacheOptions,
  ...
};

/**
 * The half of a fetch client this module calls.
 *
 * Declared structurally rather than imported from `@uniflowed/fetch`, and that
 * is a decision rather than an oversight. `@uniflowed/fetch`'s own header says
 * what it is — a failed response that is a failed promise, a timeout, and a
 * retry policy — and a cache is none of those three; putting one inside it
 * would make the thin wrapper thick, and would put a server cache in a package
 * a client bundle imports. So the caching lives on the server side and reaches
 * the client through the one method it calls, exactly as
 * `./internal/application.js` names the two methods of a Node stream rather
 * than importing `node:stream` into a module a worker has to bundle.
 */
export type CacheableClient = {
  readonly request: <T>(path: string, options?: CachedRequestOptions) => Promise<T>,
  ...
};

/** What one cached request states about its entry. */
export type FetchCacheOptions = {|
  readonly lifetime: CacheLifetime,
  readonly tags?: $ReadOnlyArray<string>,
  /** Overrides the key built from the client's name, the method and the URL. */
  readonly key?: CacheKey,
|};

/** A client that caches the requests which ask to be cached. */
export type CachedFetchClient = {|
  readonly request: <T>(path: string, options?: CachedRequestOptions) => Promise<T>,
|};

/** Everything the fetch cache needs. */
export type CachedFetchOptions = {|
  readonly client: CacheableClient,
  /**
   * What distinguishes this client from another one in the same store.
   *
   * Required, and it is the one piece of ceremony here worth defending: two
   * clients with different `baseURL`s both request `/users`, and a key built
   * from the path alone would file one client's answer under the other's name.
   * A client does not publish its `baseURL`, so the caller names it.
   */
  readonly name: string,
  /** Defaults to the store the host installed for this request. */
  readonly store?: CacheStore,
|};

/**
 * A fetch client whose cacheable requests are cached.
 *
 * Opt-in per call, and that is the whole safety argument: a request with no
 * `cache` option behaves exactly as the underlying client's does, so wrapping a
 * client changes nothing until somebody states a lifetime for one request.
 * There is no "cache every GET" mode, because every GET is not cacheable and a
 * framework guessing which ones are is how a cache serves one person's account
 * page to another.
 *
 * With no store — `rendering.cache.fetch` off, or outside a request — every
 * call passes straight through. Slower, never wrong.
 */
export function createCachedFetch(options: CachedFetchOptions): CachedFetchClient {
  const { client, name } = options;

  function storeFor(): CacheStore | null {
    if (options.store != null) {
      return options.store;
    }
    const installed = requestCache();
    return installed != null && installed.fetch === true ? installed.store : null;
  }

  return {
    request<T>(path: string, requestOptions?: CachedRequestOptions): Promise<T> {
      const caching = requestOptions?.cache;
      const store = caching == null ? null : storeFor();
      if (caching == null || store == null) {
        return client.request(path, requestOptions);
      }
      const request: CacheRequest = {
        key: caching.key ?? [
          "fetch",
          name,
          (requestOptions?.method ?? "GET").toUpperCase(),
          path,
          searchOf(requestOptions?.searchParams),
        ],
        lifetime: caching.lifetime,
        tags: caching.tags,
      };
      return store
        .resolve(request, () => client.request(path, requestOptions))
        .then((result) => result.value);
    },
  };
}

/**
 * The search parameters as one string, in the order the caller wrote them.
 *
 * Not sorted, deliberately, and it is the one place this key is looser than it
 * could be: `?a=1&b=2` and `?b=2&a=1` are two entries. Sorting would merge them
 * — and would also merge two requests to a server that treats repeated or
 * ordered parameters as meaning something, which is a wrong answer where two
 * entries are only a wasted one. A caller that minds passes `key`.
 */
function searchOf(params?: { readonly [string]: string | number | boolean }): string {
  if (params == null) {
    return "";
  }
  const search = new URLSearchParams();
  for (const key of Object.keys(params)) {
    search.set(key, String(params[key]));
  }
  return search.toString();
}
