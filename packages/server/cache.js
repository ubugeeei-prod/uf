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
// In memory, in one process. Four processes behind a load balancer hold four of
// these and disagree; a restart empties it; `revalidateTag` in one of them does
// not reach the other three. That is the whole truth about it today and there
// is no configuration that changes it. What would is a durable store behind
// `resolve`, which is an adapter's to provide.

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
 * caller having named a class.
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
 * Expire every entry filled under `tag`. Answers how many went.
 *
 * Expired, not marked stale: a tag is invalidated because somebody changed the
 * thing it names, so the entry is known wrong rather than possibly old. See
 * `./internal/cache-store.js`, which argues the difference.
 *
 * In this process. A deployment with four of them has four caches and this
 * empties one — which is why the count comes back rather than nothing, so a
 * caller can log what it actually did rather than what it meant to.
 */
export function revalidateTag(tag: string): number {
  return require$Cache("revalidateTag").store.revalidateTag(tag);
}

/** Expire every entry filled for `path`. Answers how many went. */
export function revalidatePath(path: string): number {
  return require$Cache("revalidatePath").store.revalidatePath(path);
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
