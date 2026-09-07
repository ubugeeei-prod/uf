// @flow
//
// Internal to `@uniflowed/server`: the cache, with its contract written down.
//
// Every cache bug is one of five questions being unstated, so all five are
// answered here before any code, and the code below is only these paragraphs
// spelled in Flow.
//
// # 1. What is a key
//
// A list of strings, hashed by [`./cache-key.js`], which argues at length why
// it is narrower than `@uniflowed/query`'s and why it carries no build
// identity where the two disk caches do. Read that file first; the short
// version is that this store cannot outlive the process, so the identity of
// the code that filled an entry is a constant rather than an input.
//
// # 2. What is an entry
//
// A value, the instant it was stored, the instant it goes stale, the instant
// it may no longer be served at all, the tags it was filled under, and the
// path it belongs to. Nothing else — in particular no promise and no error. A
// failed fill stores nothing, because a cached failure is a failure served to
// people who would have succeeded.
//
// # 3. When is an entry stale
//
// Two different things, and conflating them is how a cache serves an answer it
// knows to be wrong:
//
// * **Time.** `revalidate` seconds after it was stored an entry is *stale*: it
//   may be old. `expire` seconds after it was stored it is *expired*: it may
//   not be served. `expire` defaults to `revalidate`, which means no
//   stale-while-revalidate unless a caller asks for one — serving a stale
//   answer is still serving a stale answer, and the caller is the only one who
//   knows whether that is acceptable for this data. `expire > revalidate`
//   opens the window, and inside it a reader gets the old value now and a
//   refresh happens behind them.
// * **A statement.** `revalidateTag` and `revalidatePath` do not make an entry
//   stale, they **expire** it. An entry a tag invalidated is not "possibly
//   old", it is known wrong — somebody just changed the thing it describes —
//   and stale-while-revalidate over a known-wrong answer is the failure mode
//   this whole store exists not to have.
//
// An entry with no lifetime at all is not stored. There is no "cache forever"
// default and there will not be one: an in-memory entry with no expiry is an
// answer that is served until the process restarts, which is the stale answer
// nobody asked for wearing a config flag.
//
// # 4. Who evicts
//
// Three things, all of them synchronous and none of them a timer:
//
// * a read that finds an expired entry drops it;
// * an insertion that would exceed `maxEntries` drops the least recently used
//   entry first;
// * `clear()`.
//
// No background sweep, deliberately. A cache with a timer in it is a process
// that will not exit, and the two costs it saves — memory held by entries
// nobody reads — are bounded by `maxEntries` anyway.
//
// # 5. What happens to a request that arrives while an entry is being filled
//
// It joins. One fill per key: the second caller awaits the first caller's
// promise rather than starting a second one. That is the property a cache in
// front of a slow loader is mostly *for* — ten simultaneous requests for a
// cold page are one render, not ten — and it is also the only one of the five
// that cannot be added later without changing every caller.
//
// A caller that finds a *stale but servable* entry does not join: it gets the
// old value immediately and a refresh runs behind it, alone. A refresh that
// fails leaves the stale entry where it is and reports through `onError`; the
// next reader tries again. A blocking fill that fails is the caller's failure
// and is not stored.
//
// # Where it lives, said plainly
//
// In memory, in one process. Four server processes behind a load balancer have
// four of these and they disagree; a restart empties it; `revalidateTag` in
// one process does not reach the other three. That is the honest state of it
// today and it is written here, in `docs/architecture.md`, and in the
// package's own documentation rather than implied away. What would fix it is a
// durable store behind this same seam, which is an adapter's job — and per the
// deployment rules a target that cannot provide one has to say so rather than
// quietly degrade. `resolve` is the whole seam: anything that can answer it
// can be the store.

import { AsyncLocalStorage } from "node:async_hooks";

import type { CacheKey } from "./cache-key.js";
import { hashCacheKey } from "./cache-key.js";

/** How long an entry stays fresh, and how long it may be served at all. */
export type CacheLifetime = {|
  /** Seconds after which the entry is stale. */
  readonly revalidate: number,
  /**
   * Seconds after which it may not be served.
   *
   * Defaults to `revalidate` — no stale-while-revalidate unless asked for.
   */
  readonly expire?: number,
|};

/** One stored answer. */
export type CacheEntry<T> = {|
  readonly value: T,
  readonly storedAt: number,
  readonly revalidateAt: number,
  readonly expiresAt: number,
  readonly tags: $ReadOnlyArray<string>,
  readonly path: string | null,
|};

/** What a caller says about the entry before it is filled. */
export type CacheRequest = {|
  readonly key: CacheKey,
  readonly lifetime?: CacheLifetime,
  readonly tags?: $ReadOnlyArray<string>,
  readonly path?: string,
|};

/** How a value was arrived at, for a caller that wants to report it. */
export type CacheOutcome = "hit" | "stale" | "miss" | "coalesced" | "uncacheable";

/** A value, and how it was arrived at. */
export type CacheResult<T> = {|
  readonly value: T,
  readonly outcome: CacheOutcome,
  /** Whether this call left an entry behind. */
  readonly stored: boolean,
|};

/** Running totals, for a benchmark or a report. */
export type CacheStats = {|
  readonly hits: number,
  readonly stale: number,
  readonly misses: number,
  readonly coalesced: number,
  readonly fills: number,
  readonly evictions: number,
  readonly invalidations: number,
|};

/**
 * What a host installed for one request, from `rendering.cache`.
 *
 * Declared here rather than in `../cache.js` so that `./context.js` can name
 * it without importing the module that imports `./context.js`. A type-only
 * cycle is harmless and an import cycle between two modules a request goes
 * through is not worth finding out about later.
 */
export type CacheOptions = {|
  readonly store: CacheStore,
  /** `rendering.cache.route`: whether a rendered document may be stored. */
  readonly route?: boolean,
  /** `rendering.cache.fetch`: whether a cached fetch client may use the store. */
  readonly fetch?: boolean,
|};

/** How a store behaves. */
export type CacheStoreOptions = {|
  /**
   * The clock, in milliseconds.
   *
   * Injectable because staleness is the only thing in here that is a fact
   * about time, and a test that drives it with a real clock is a test that
   * sleeps — which is a test that flakes on a loaded machine. Defaults to
   * `Date.now`.
   */
  readonly now?: () => number,
  /** Most entries held at once. Defaults to 1024. */
  readonly maxEntries?: number,
  /**
   * Where a background refresh's failure goes.
   *
   * It has nowhere else to go: nobody is awaiting it, so without this it is an
   * unhandled rejection that takes the process down on a strict runtime.
   */
  readonly onError?: (error: mixed) => void,
|};

/**
 * What a fill may declare about itself while it is running.
 *
 * The declarations arrive from *inside* `produce` — `cacheLife` and `cacheTag`
 * are called by a loader or a component, six frames below the thing that
 * started the fill, which is the same reason `cookies()` reads an
 * `AsyncLocalStorage` rather than an argument.
 */
export type CacheScope = {|
  lifetime: CacheLifetime | null,
  readonly tags: Array<string>,
  /** Set when something decided this answer must not be stored, and why. */
  denied: string | null,
|};

const scopes: AsyncLocalStorage<CacheScope> = new AsyncLocalStorage();

/**
 * The fill this call is inside, or `null`.
 *
 * `null` rather than throwing, so each caller can name what *it* wanted the
 * fill for — `cacheTag() was called outside a cached scope` is a better error
 * than one generic message from here.
 */
export function currentScope(): CacheScope | null {
  return scopes.getStore() ?? null;
}

/** Run `body` as a fill, with `scope` collecting what it declares. */
export function runInScope<T>(scope: CacheScope, body: () => Promise<T>): Promise<T> {
  return scopes.run(scope, body);
}

/** A fresh scope, declaring nothing yet. */
export function newScope(request: CacheRequest): CacheScope {
  return {
    lifetime: request.lifetime ?? null,
    tags: request.tags == null ? [] : Array.from(request.tags),
    denied: null,
  };
}

/** Seconds to milliseconds, refusing anything that is not a real duration. */
function millis(seconds: number, name: string): number {
  if (!Number.isFinite(seconds) || seconds < 0) {
    throw new RangeError(
      `@uniflowed/server: ${name} is ${String(seconds)} seconds, which is not a duration. ` +
        "A cache entry with no stated end is one that is served until the process restarts.",
    );
  }
  return seconds * 1000;
}

/**
 * A cache: `resolve` a key, invalidate by tag or by path.
 *
 * Explicit rather than a module-level default, for the reason
 * `@uniflowed/query`'s cache is: a singleton is shared with every other test in
 * the process, and one test's cached answer then decides another's. On a
 * server it is worse than a flake — two requests being answered at once must
 * not be able to reach each other's data by accident — so a store is
 * constructed by whoever owns the process and handed to whoever answers a
 * request.
 */
export class CacheStore {
  readonly entries: Map<string, CacheEntry<mixed>> = new Map();
  readonly filling: Map<string, Promise<mixed>> = new Map();
  now: () => number;
  maxEntries: number;
  onError: (error: mixed) => void;
  hits: number = 0;
  staleServed: number = 0;
  misses: number = 0;
  coalesced: number = 0;
  fills: number = 0;
  evictions: number = 0;
  invalidations: number = 0;

  constructor(options?: CacheStoreOptions) {
    this.now = options?.now ?? Date.now;
    this.maxEntries = options?.maxEntries ?? 1024;
    this.onError =
      options?.onError ??
      ((error: mixed) => {
        // eslint-disable-next-line no-console
        console.error("uf: a cache refresh failed", error);
      });
    if (!Number.isInteger(this.maxEntries) || this.maxEntries < 1) {
      throw new RangeError(
        `@uniflowed/server: maxEntries is ${String(this.maxEntries)}; a store that can hold ` +
          "no entries is a store that only costs.",
      );
    }
  }

  /** How many entries are held. */
  size(): number {
    return this.entries.size;
  }

  /** The running totals. */
  stats(): CacheStats {
    return {
      hits: this.hits,
      stale: this.staleServed,
      misses: this.misses,
      coalesced: this.coalesced,
      fills: this.fills,
      evictions: this.evictions,
      invalidations: this.invalidations,
    };
  }

  /**
   * The entry under `key` as it stands, without filling or evicting anything.
   *
   * For a test and for a report. Nothing on the answering path uses it: a read
   * that finds an expired entry has to drop it, and a method that promises not
   * to would be a second, subtly different read.
   */
  peek(key: CacheKey): CacheEntry<mixed> | null {
    return this.entries.get(hashCacheKey(key)) ?? null;
  }

  /** Forget one entry. Answers whether there was one. */
  forget(key: CacheKey): boolean {
    return this.entries.delete(hashCacheKey(key));
  }

  /** Forget everything. Fills already in flight still finish, and are dropped. */
  clear(): void {
    this.entries.clear();
    this.filling.clear();
  }

  /**
   * The value for `request.key`, filling it with `produce` if it has to.
   *
   * The whole seam. Every one of the five answers above is in this method, and
   * the order of the branches is the order the contract states them in.
   */
  async resolve<T>(request: CacheRequest, produce: () => Promise<T>): Promise<CacheResult<T>> {
    const hash = hashCacheKey(request.key);
    const at = this.now();
    const existing = this.entries.get(hash);

    if (existing != null) {
      if (at >= existing.expiresAt) {
        // Expired: dropped on the read that found it, which is one of the
        // three things that evict. Falls through to a fill.
        this.entries.delete(hash);
      } else if (at < existing.revalidateAt) {
        this.hits += 1;
        this.touch(hash, existing);
        return { value: (existing.value: $FlowFixMe), outcome: "hit", stored: false };
      } else {
        // Stale and inside the stale-while-revalidate window the caller asked
        // for: the old value now, a refresh behind it. `void` and a `catch`
        // rather than an await — nobody is waiting for this, and an unhandled
        // rejection from a refresh nobody asked about would take the process
        // down on a runtime that treats them as fatal.
        this.staleServed += 1;
        this.touch(hash, existing);
        this.refresh(hash, request, produce);
        return { value: (existing.value: $FlowFixMe), outcome: "stale", stored: false };
      }
    }

    const inflight = this.filling.get(hash);
    if (inflight != null) {
      this.coalesced += 1;
      const value: T = (await inflight: $FlowFixMe);
      return { value, outcome: "coalesced", stored: false };
    }

    this.misses += 1;
    const scope = newScope(request);
    const filling = this.fill(hash, request, scope, produce);
    this.filling.set(hash, filling);
    let value: T;
    try {
      value = await filling;
    } finally {
      // Only if it is still ours: `clear()` may have run while this was in
      // flight, and a later fill may already have claimed the key.
      if (this.filling.get(hash) === filling) {
        this.filling.delete(hash);
      }
    }
    return {
      value,
      outcome: scope.denied == null && scope.lifetime != null ? "miss" : "uncacheable",
      stored: this.entries.has(hash),
    };
  }

  /**
   * Run `produce` as a fill and store what it produced, if it may be stored.
   *
   * Three ways it may not be, and each is a decision somebody made rather than
   * a failure: the fill declared no lifetime, so there is no answer to "when
   * does this stop being true"; something in it called `noStore`; or it threw,
   * and a cached failure is a failure served to callers who would have
   * succeeded.
   */
  async fill<T>(
    hash: string,
    request: CacheRequest,
    scope: CacheScope,
    produce: () => Promise<T>,
  ): Promise<T> {
    this.fills += 1;
    const value = await runInScope(scope, produce);
    const lifetime = scope.lifetime;
    if (scope.denied != null || lifetime == null) {
      return value;
    }
    const storedAt = this.now();
    const revalidate = millis(lifetime.revalidate, "revalidate");
    const expire = millis(lifetime.expire ?? lifetime.revalidate, "expire");
    if (expire < revalidate) {
      throw new RangeError(
        `@uniflowed/server: expire (${String(lifetime.expire)}s) is before revalidate ` +
          `(${String(lifetime.revalidate)}s), which asks for an entry that is unusable ` +
          "before it is stale.",
      );
    }
    this.store(hash, {
      value,
      storedAt,
      revalidateAt: storedAt + revalidate,
      expiresAt: storedAt + expire,
      tags: Array.from(new Set(scope.tags)),
      path: request.path ?? null,
    });
    return value;
  }

  /**
   * Refill `hash` behind a reader that was served the stale value.
   *
   * Alone: a second stale read while this is running is served the stale value
   * too and starts nothing, because the entry it would refresh is already
   * being refreshed. A failure leaves the stale entry exactly where it is —
   * the next reader inside the window tries again, and the one after the
   * window blocks on a fill that can fail properly.
   *
   * # The one thing to know about it
   *
   * A refresh is started synchronously inside the reader that was served the
   * stale value, so it inherits that reader's asynchronous context — including
   * the request `../cache.js`'s bindings answer about. A refresh that reads
   * `cookies()` therefore reads *that* reader's cookies, which would be a
   * document about one person filed under a URL everybody asks for.
   *
   * It cannot become one, and the reason is worth stating rather than
   * trusting: the same refusal that governs a cold fill governs this one.
   * `../fetch.js`'s fill compares the request-state read count across its own
   * run, so a refresh that read the request calls `noStore`, nothing is
   * written, and the value is discarded — `refresh` does not return it to
   * anybody. The stale entry stays until it expires and the reader after that
   * blocks on a fill of its own.
   *
   * The cost of inheriting the context is a smaller one: an `after()` callback
   * registered by a refresh lands on a request whose `settle` may already have
   * run, and is then never called. Deferred work registered by a render nobody
   * is waiting for is the least surprising thing to lose, and detaching the
   * refresh into a context of its own would mean this module knowing what a
   * request is, which it deliberately does not.
   */
  refresh<T>(hash: string, request: CacheRequest, produce: () => Promise<T>): void {
    if (this.filling.has(hash)) {
      return;
    }
    const scope = newScope(request);
    const running = this.fill(hash, request, scope, produce).catch((error: mixed) => {
      this.onError(error);
      return undefined;
    });
    this.filling.set(hash, running);
    void running.then(() => {
      if (this.filling.get(hash) === running) {
        this.filling.delete(hash);
      }
    });
  }

  /**
   * Put an entry in, evicting the least recently used one if there is no room.
   *
   * `Map` keeps insertion order, and [`touch`] re-inserts on every read, so the
   * first key the iterator yields is the one nothing has wanted for longest.
   */
  store(hash: string, entry: CacheEntry<mixed>): void {
    this.entries.delete(hash);
    while (this.entries.size >= this.maxEntries) {
      const oldest = this.entries.keys().next();
      if (oldest.done === true) {
        break;
      }
      this.entries.delete(oldest.value);
      this.evictions += 1;
    }
    this.entries.set(hash, entry);
  }

  /** Mark an entry as the most recently used one. */
  touch(hash: string, entry: CacheEntry<mixed>): void {
    this.entries.delete(hash);
    this.entries.set(hash, entry);
  }

  /**
   * Expire every entry filled under `tag`. Answers how many.
   *
   * Expired rather than marked stale, which is the distinction the module
   * header argues: a tag is invalidated because somebody changed the thing it
   * names, so the entry is known wrong rather than possibly old, and there is
   * no window in which serving it is acceptable.
   */
  revalidateTag(tag: string): number {
    return this.expireWhere((entry) => entry.tags.includes(tag));
  }

  /** Expire every entry filled for `path`. Answers how many. */
  revalidatePath(path: string): number {
    return this.expireWhere((entry) => entry.path === path);
  }

  /** Drop every entry `matches` describes, counting them. */
  expireWhere(matches: (entry: CacheEntry<mixed>) => boolean): number {
    let dropped = 0;
    for (const [hash, entry] of Array.from(this.entries.entries())) {
      if (matches(entry)) {
        this.entries.delete(hash);
        dropped += 1;
      }
    }
    this.invalidations += dropped;
    return dropped;
  }
}
