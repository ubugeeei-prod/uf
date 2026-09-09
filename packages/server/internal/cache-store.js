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
// it is narrower than `@uniflowed/query`'s. Read that file first. In memory
// the identity of the code that filled an entry is a constant rather than an
// input, because an in-memory entry cannot outlive the process that filled it;
// **durably it is the first member of the key**, because a durable entry can,
// and a deploy that served the previous build's documents under the new
// build's URLs would be the two disk caches' lesson learned a third time.
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
// A durable provider evicts on its own terms as well, and it has to: uf's
// `maxEntries` is a bound on this process's memory and says nothing about a
// disk four processes share. What a provider may **not** do is decide that an
// entry is stale — that is question 3 and it is answered here, from the entry's
// own timestamps, whichever store the entry came out of.
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
// In memory by default, in one process: four server processes behind a load
// balancer have four of these and they disagree, a restart empties it, and
// `revalidateTag` in one of them does not reach the other three. That is still
// what a store constructed with nothing looks like, and it is still the
// default, because persistence is a thing to opt into and not a thing to find
// out about.
//
// A store constructed with a `provider` is the other half, and it is what makes
// the existing machinery incremental static regeneration rather than a
// per-process accelerator. `./cache-provider.js` is the seam — five methods
// over strings — and `../cache-filesystem.js` is one implementation of it; a
// deployment adapter with a Redis or a KV namespace writes its own and uf never
// learns which. Then: a restart finds the entries where it left them, four
// processes read one store, and `revalidateTag` in any of them takes the entry
// out of the store all four fill from.
//
// Memory stays in front of it, and stays exactly what it was — an L1 whose five
// answers are the five above. Two consequences worth stating rather than
// discovering:
//
// * An entry another process **already holds in memory** is not reached by this
//   process's `revalidateTag`. It is gone from the shared store, so nothing
//   fills from it again and no process without a copy can serve it; a process
//   with a copy serves it until its own `revalidate`, which is the number the
//   application named as how stale that page may be. Bounded, and by the
//   application's own statement rather than by uf's convenience.
// * A durable write does not block the answer. It is tracked, reported through
//   `onError` when it fails, and awaited by [`CacheStore.settled`] — which
//   `../cache.js` hands to the request so a host that freezes its process the
//   moment a response is written does not freeze it mid-write.

import { AsyncLocalStorage } from "node:async_hooks";

import type { CacheKey } from "./cache-key.js";
import { hashCacheKey, hashDurableCacheKey } from "./cache-key.js";
import type { CacheProvider } from "./cache-provider.js";
import { decodeCacheValue, encodeCacheValue } from "./cache-provider.js";

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
  /**
   * Entries this process took out of a durable provider rather than rendering.
   *
   * Counted separately from `hits` rather than folded into them, because the
   * two answer different questions: `hits` is whether the cache is working and
   * this is whether *persistence* is — a process that restarts into a warm
   * store and one that restarts into an empty one look identical on `hits`
   * alone, and which of the two happened is the whole claim of a durable cache.
   * Always zero with no provider.
   */
  readonly restored: number,
  /**
   * Entries this process handed to a durable provider. Zero with none.
   *
   * Counted when the write is started rather than when it lands, because it is
   * not awaited — see [`CacheStore.persist`] — and a counter that waited would
   * be a counter that made the request wait. A write that then failed is
   * reported through `onError`, which is where a failure belongs.
   */
  readonly persisted: number,
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
   * unhandled rejection that takes the process down on a strict runtime. A
   * durable write's failure and a value that cannot be encoded go here too, for
   * the same reason and with the same consequence: the answer was correct, the
   * cache is colder than it meant to be, and nothing that a request can see
   * changed.
   */
  readonly onError?: (error: mixed) => void,
  /**
   * Somewhere entries outlive this process.
   *
   * Absent is the default and is the whole store as it was: memory, one
   * process, emptied by a restart. Present makes it incremental static
   * regeneration — see the module header — and obliges `build` below.
   *
   * A provider is an object rather than a name, and that is the replaceability
   * red line rather than a convenience. `docs/red-lines.md` line 3 says every
   * built-in provider must be replaceable and that "a provider a project can
   * replace has to be a name it can write"; here the *host* writes it, in
   * JavaScript, at the point it constructs the store — so a deployment adapter
   * with a Redis, a KV namespace or an S3 bucket puts it behind this seam
   * without uf shipping a release, an enumeration, or the word Redis.
   */
  readonly provider?: CacheProvider,
  /**
   * The identity of the build whose code fills these entries.
   *
   * Required whenever `provider` is given, refused as empty, and unused
   * without one. `./cache-key.js` argues why at length: a durable entry
   * outlives the build that produced it, so the build is the first member of
   * every durable key, and a deploy therefore reads a cold cache rather than
   * the previous build's documents.
   *
   * Two processes of the *same* build must agree on it — that is what makes
   * four instances one cache — so it is minted once where a build is minted and
   * carried in the artefact, not generated per process. `uf build` writes one;
   * `UF_BUILD_ID` overrides it, exactly as it does for the build id
   * `crates/uf_rsc` derives server action ids from.
   */
  readonly build?: string,
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

/**
 * What one owned fill turned out to be, reported back out of the promise.
 *
 * A mutable box, and it exists because a `Promise` carries one thing and this
 * needs two: the value, which the joiners in the in-flight map are also
 * awaiting, and where it came from, which only the caller that owns the fill
 * reports. `null` means the fill actually rendered.
 */
type FillAttempt = {| restored: "hit" | "stale" | null |};

/** The five methods a durable provider owes, checked once when it is wired. */
const PROVIDER_METHODS = ["read", "write", "remove", "invalidateTag", "invalidatePath", "clear"];

/**
 * Refuse a provider that cannot answer the seam, at the line that wired it.
 *
 * Checked at construction rather than at the first call, which is the opposite
 * of what `../fetch.js` does with `app.runMiddleware` and is the opposite for a
 * reason. A missing `runMiddleware` is a server bundle that is wrong for every
 * request, so the first request finds it and the stack says where. A missing
 * `invalidateTag` is a store that answers every request perfectly and silently
 * stops invalidating — nothing fails, and the symptom is a stale page hours
 * later in a process nobody is watching. That one has to be found where it was
 * written.
 */
function assertProvider(provider: CacheProvider): void {
  if (typeof provider.name !== "string" || provider.name === "") {
    throw new TypeError(
      "@uniflowed/server: a durable cache provider needs a `name`, so that what is holding " +
        "the entries is something a log can say rather than something a reader has to infer.",
    );
  }
  const methods: { readonly [string]: mixed } = (provider: $FlowFixMe);
  for (const method of PROVIDER_METHODS) {
    if (typeof methods[method] !== "function") {
      throw new TypeError(
        `@uniflowed/server: the cache provider ${JSON.stringify(provider.name)} has no ` +
          `${method}(). All of ${PROVIDER_METHODS.join(", ")} are required — a provider ` +
          "missing one is a cache that answers every request and quietly stops doing one " +
          "of the things a cache is for. See internal/cache-provider.js.",
      );
    }
  }
}

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
  /**
   * Durable work started and not finished.
   *
   * A set rather than a count, so [`settled`] can await the actual promises
   * instead of polling a number, and so a store with nothing outstanding
   * settles without yielding at all.
   */
  readonly writing: Set<Promise<mixed>> = new Set();
  now: () => number;
  maxEntries: number;
  onError: (error: mixed) => void;
  provider: CacheProvider | null;
  build: string | null;
  hits: number = 0;
  staleServed: number = 0;
  misses: number = 0;
  coalesced: number = 0;
  fills: number = 0;
  evictions: number = 0;
  invalidations: number = 0;
  restored: number = 0;
  persisted: number = 0;

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
    const provider = options?.provider ?? null;
    this.provider = provider;
    this.build = options?.build ?? null;
    if (provider != null) {
      assertProvider(provider);
      // Here rather than at the first durable write, and it is the one place
      // this store refuses instead of degrading. `@uniflowed/host`'s transform
      // cache, faced with a host that cannot name its compiler, reads nothing
      // and writes nothing — "slower, never wrong" — because it discovers the
      // identity at run time and may legitimately not find one. This does not
      // discover anything: a host passed a provider and forgot the build, on
      // purpose, once, in a line of wiring. Falling back to memory there would
      // turn a fixable typo into a deployment that quietly is not what it says
      // it is, and per the deployment rules a target that cannot provide a
      // durable store has to say so rather than degrade.
      if (typeof this.build !== "string" || this.build === "") {
        throw new TypeError(
          `@uniflowed/server: a store with a durable provider (${provider.name}) needs ` +
            "`build`, the identity of the build whose code fills its entries, and it is " +
            `${JSON.stringify(this.build)}. A durable entry outlives the build that wrote ` +
            "it, so without one a deploy serves the previous build's documents under the " +
            "new build's URLs.",
        );
      }
    }
  }

  /** How many entries are held in memory. */
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
      restored: this.restored,
      persisted: this.persisted,
    };
  }

  /**
   * Every durable write and invalidation this store has started.
   *
   * A durable write does not block the answer — a reader waiting on a disk for
   * a document it already has in its hand is paying for somebody else's next
   * request — so the work outlives the response, and something has to be able
   * to wait for it. Two callers do: a test that wants to see the entry land,
   * and a host whose process stops the moment a response is written, which is
   * every serverless one. `../cache.js` gives it to the request through
   * `after()`, so `settle()` covers it wherever a host settles.
   *
   * Resolves immediately with no provider, and loops rather than awaiting once
   * because an invalidation started while this was waiting is work this promise
   * is also about.
   */
  async settled(): Promise<void> {
    while (this.writing.size > 0) {
      await Promise.all(Array.from(this.writing));
    }
  }

  /**
   * Run `operation` against the provider, outside anybody's request.
   *
   * The single place durable work is started, so there is one answer to what
   * happens when it fails and one place that answer is written: it goes to
   * `onError` and nothing a request can see changes. That is the same
   * disposition [`refresh`] has and it rests on the same fact — the answer
   * this cache exists to speed up has already been produced correctly, so a
   * store that could not keep it is slower rather than wrong.
   */
  durably(operation: (provider: CacheProvider) => Promise<mixed>): void {
    const provider = this.provider;
    if (provider == null) {
      return;
    }
    let running: Promise<mixed>;
    try {
      running = operation(provider);
    } catch (error) {
      // A provider that throws synchronously is a provider, not an exception
      // to the rule about them.
      this.onError(error);
      return;
    }
    const tracked = running.catch((error: mixed) => {
      this.onError(error);
    });
    this.writing.add(tracked);
    void tracked.then(() => {
      this.writing.delete(tracked);
    });
  }

  /**
   * The name this key has in a durable store.
   *
   * Separate from [`hashCacheKey`]'s answer rather than replacing it, because
   * the two name entries in two stores with two lifetimes: memory is emptied by
   * the restart that the durable store exists to survive, so putting the build
   * in the in-memory key would cost a string concatenation per lookup to
   * distinguish entries that cannot coexist.
   */
  durableKey(key: CacheKey): string {
    return hashDurableCacheKey(this.build ?? "", key);
  }

  /**
   * The entry under `key` as it stands, without filling or evicting anything.
   *
   * For a test and for a report. Nothing on the answering path uses it: a read
   * that finds an expired entry has to drop it, and a method that promises not
   * to would be a second, subtly different read.
   *
   * Memory only, and synchronous because of it. A durable entry this process
   * has not read yet is not an entry this process has; asking the provider here
   * would make a method whose whole purpose is to observe without changing
   * anything into one that reaches over a network.
   */
  peek(key: CacheKey): CacheEntry<mixed> | null {
    return this.entries.get(hashCacheKey(key)) ?? null;
  }

  /** Forget one entry, here and durably. Answers whether there was one here. */
  forget(key: CacheKey): boolean {
    this.durably((provider) => provider.remove(this.durableKey(key)));
    return this.entries.delete(hashCacheKey(key));
  }

  /** Forget everything. Fills already in flight still finish, and are dropped. */
  clear(): void {
    this.entries.clear();
    this.filling.clear();
    this.durably((provider) => provider.clear());
  }

  /**
   * The value for `request.key`, filling it with `produce` if it has to.
   *
   * The whole seam. Every one of the five answers above is in this method, and
   * the order of the branches is the order the contract states them in.
   *
   * # Where the durable store went in, and why it went there
   *
   * After memory and after the in-flight map, before the fill. Memory first
   * because it is free and the answer is the same; the in-flight map before the
   * provider because a durable read is exactly the kind of wait a second caller
   * must not duplicate.
   *
   * That ordering forced the shape of the rest of this method. The durable read
   * is asynchronous, so it cannot sit between "nothing in memory" and "claim the
   * key" as a plain `await` would — two callers would both find memory empty,
   * both find the in-flight map empty, both go to the disk, and both then
   * render, which is answer 5 broken by the change meant to make the store
   * better. So the claim is made *first*, synchronously, over a promise that
   * does the durable read and the fill together: [`fillThrough`]. What that
   * call learned about where the value came from comes back on `attempt`,
   * because a promise can carry a value and this method has to report an
   * outcome as well.
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

    const scope = newScope(request);
    const attempt: FillAttempt = { restored: null };
    const filling = this.fillThrough(hash, request, scope, attempt, produce);
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

    if (attempt.restored === "hit") {
      this.hits += 1;
      return { value, outcome: "hit", stored: false };
    }
    if (attempt.restored === "stale") {
      // The refresh is started here rather than inside `fillThrough`, and the
      // two lines above are the reason: [`refresh`] declines while the key is
      // being filled, and until the `finally` ran this call *was* the fill. A
      // refresh started from in there would have found the map occupied by
      // itself and quietly not happened.
      this.staleServed += 1;
      this.refresh(hash, request, produce);
      return { value, outcome: "stale", stored: false };
    }

    this.misses += 1;
    return {
      value,
      outcome: scope.denied == null && scope.lifetime != null ? "miss" : "uncacheable",
      stored: this.entries.has(hash),
    };
  }

  /**
   * The durable store, then a fill: what one caller does once it owns the key.
   *
   * Answers the value either way, so that a second caller joining through the
   * in-flight map gets the same thing whichever half produced it — a joiner
   * cannot tell a disk read from a render, and should not be able to.
   *
   * With no provider this is [`fill`] with one comparison in front of it, which
   * is the shape the default store keeps: nothing about a memory-only cache got
   * slower to make a durable one possible.
   */
  async fillThrough<T>(
    hash: string,
    request: CacheRequest,
    scope: CacheScope,
    attempt: FillAttempt,
    produce: () => Promise<T>,
  ): Promise<T> {
    if (this.provider != null) {
      const entry = await this.restore(hash, request);
      if (entry != null) {
        attempt.restored = this.now() < entry.revalidateAt ? "hit" : "stale";
        return (entry.value: $FlowFixMe);
      }
    }
    return this.fill(hash, request, scope, produce);
  }

  /**
   * The entry a provider is holding for this key, promoted into memory.
   *
   * `null` for every way there is not one: nothing stored, an entry past its
   * `expiresAt`, a provider that failed, or a value that will not decode. The
   * last two go to `onError` and then read as a miss, which is the disposition
   * the whole durable half has — a store that cannot answer costs a render.
   *
   * **Staleness is decided here and not by the provider.** The timestamps come
   * out of the record exactly as they went in, so an entry written by another
   * process is judged by the same clock arithmetic as one this process wrote,
   * and a provider has no way to hand back something it considers fresh that
   * this store considers expired. That is question 3 staying answered in one
   * place across a seam.
   */
  async restore(hash: string, request: CacheRequest): Promise<CacheEntry<mixed> | null> {
    const provider = this.provider;
    if (provider == null) {
      return null;
    }
    const key = this.durableKey(request.key);
    let record;
    try {
      record = await provider.read(key);
    } catch (error) {
      this.onError(error);
      return null;
    }
    if (record == null) {
      return null;
    }
    if (this.now() >= record.expiresAt) {
      // The same rule memory has — a read that finds an expired entry drops it
      // — pointed at the provider, so an entry nobody comes back for is not
      // left for the provider's own bound to notice eventually.
      this.durably((durable) => durable.remove(key));
      return null;
    }
    let value: mixed;
    try {
      value = decodeCacheValue(record.value);
    } catch (error) {
      this.onError(error);
      return null;
    }
    const entry: CacheEntry<mixed> = {
      value,
      storedAt: record.storedAt,
      revalidateAt: record.revalidateAt,
      expiresAt: record.expiresAt,
      tags: record.tags,
      path: record.path,
    };
    this.restored += 1;
    this.store(hash, entry);
    return entry;
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
    const entry: CacheEntry<mixed> = {
      value,
      storedAt,
      revalidateAt: storedAt + revalidate,
      expiresAt: storedAt + expire,
      tags: Array.from(new Set(scope.tags)),
      path: request.path ?? null,
    };
    this.store(hash, entry);
    this.persist(request, entry);
    return value;
  }

  /**
   * Write an entry through to the provider, if there is one.
   *
   * Not awaited, and that is the trade this method exists to make in one place.
   * The caller is holding the answer already — a rendered document, whole, in
   * memory — and making it wait on a disk or a network round trip would charge
   * this request for the *next* request's saving. So the write is started,
   * tracked in `writing`, and awaited by whoever has a reason to: a test, or a
   * host whose process may stop the moment the response is written.
   *
   * A value that cannot be encoded is not a failed request and not a failed
   * fill. It is reported through `onError` and the entry stays in memory,
   * exactly as `@uniflowed/host`'s transform cache tolerates a directory it
   * cannot write — the answer was produced correctly and only its keeping
   * failed. `./cache-provider.js` argues what "cannot be encoded" covers and
   * why it is a refusal rather than a best effort.
   */
  persist(request: CacheRequest, entry: CacheEntry<mixed>): void {
    if (this.provider == null) {
      return;
    }
    let value: string;
    try {
      value = encodeCacheValue(entry.value);
    } catch (error) {
      this.onError(error);
      return;
    }
    const key = this.durableKey(request.key);
    this.persisted += 1;
    this.durably((provider) =>
      provider.write(key, {
        value,
        storedAt: entry.storedAt,
        revalidateAt: entry.revalidateAt,
        expiresAt: entry.expiresAt,
        tags: entry.tags,
        path: entry.path,
      }),
    );
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
   *
   * # The count, and the two things it counts
   *
   * The number is what went **here**, synchronously, and it stays that with a
   * provider attached rather than becoming a promise: a mutation handler that
   * has to `await` an invalidation to learn a number is a handler that now
   * waits on a disk before it can answer, and nothing it does with the number
   * is worth that. The durable half is started beside it and finishes with the
   * request — see [`durably`] and [`settled`].
   *
   * The two are different quantities and the difference is real. This process
   * may hold three entries under `posts` while the shared store holds forty,
   * and after this call both are gone from the shared store; a *fourth*
   * process holding its own copy in memory keeps serving it until its own
   * `revalidate`, because nothing here can reach into another process's memory
   * and this store does not pretend otherwise. The module header states that
   * window and why it is the application's own number.
   */
  revalidateTag(tag: string): number {
    this.durably((provider) => provider.invalidateTag(tag));
    return this.expireWhere((entry) => entry.tags.includes(tag));
  }

  /** Expire every entry filled for `path`, here and durably. Answers how many here. */
  revalidatePath(path: string): number {
    this.durably((provider) => provider.invalidatePath(path));
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
