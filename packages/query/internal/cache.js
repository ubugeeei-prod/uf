// @flow
//
// The cache a query reads from, and who else is reading it.
//
// This is the part that makes a query library worth having rather than a
// `useEffect` and a `useState`:
//
//   * **Two components asking for the same thing make one request.** Without
//     that, a page with a header and a sidebar both showing the current user
//     fetches the user twice, and they can disagree.
//   * **A cached answer is shown immediately and refreshed behind it.** The
//     alternative is a spinner every time a reader navigates back, which is
//     the single most common way an application feels slow while being fast.
//   * **An answer that is old enough is refetched; one that is merely used is
//     not.** Those are different questions, and conflating them either
//     hammers the server or shows stale data forever.
//
// The store is deliberately not React-aware: it is a map, a set of listeners
// per key, and a promise per in-flight request. `useQuery` is a
// `useSyncExternalStore` over it, which is what makes a cached value correct
// during a prerender and free of tearing on the client.

/** A key, as a caller writes it. */
export type QueryKey = $ReadOnlyArray<mixed>;

/** What the cache holds for one key. */
export type Entry<T> = {|
  +value: T | void,
  +error: Error | null,
  /** When the value arrived, so staleness is a question the reader can ask. */
  +updatedAt: number,
  +pending: boolean,
  /** Bumped on every change, so a snapshot can be compared by identity. */
  +version: number,
|};

type Slot = {
  entry: Entry<mixed>,
  listeners: Set<() => void>,
  inFlight: Promise<mixed> | null,
  /** Cleared when the last listener goes and the entry has outlived its use. */
  collect: TimeoutID | null,
  /**
   * How to fetch this key again, recorded by `watch`.
   *
   * Without it, invalidation can only mark an entry stale — and a component
   * already watching would re-render, see that it is stale, and do nothing,
   * because nothing was asked to fetch. Keeping the query here is what lets
   * the store refresh what somebody is looking at.
   */
  query: (() => Promise<mixed>) | null,
};

/** How long an unused entry is kept, so a quick navigation back is free. */
const DEFAULT_GARBAGE_MILLIS = 5 * 60_000;

const EMPTY: Entry<mixed> = {
  value: undefined,
  error: null,
  updatedAt: 0,
  pending: false,
  version: 0,
};

/**
 * A cache. An application usually has one, and a test has one per test.
 *
 * Explicit rather than a module-level singleton, because a singleton is shared
 * with every other test in the process and one test's cached answer then
 * decides another's result.
 */
export class QueryCache {
  +slots: Map<string, Slot> = new Map();
  +garbageMillis: number;

  constructor(options?: {| +garbageMillis?: number |}) {
    this.garbageMillis = options?.garbageMillis ?? DEFAULT_GARBAGE_MILLIS;
  }

  /** What is known about `key` right now. */
  read(key: QueryKey): Entry<mixed> {
    return this.slots.get(hash(key))?.entry ?? EMPTY;
  }

  /** Watch `key`. Returns the unsubscribe. */
  subscribe(key: QueryKey, listener: () => void): () => void {
    const id = hash(key);
    const slot = this.slot(id);
    slot.listeners.add(listener);
    if (slot.collect != null) {
      clearTimeout(slot.collect);
      slot.collect = null;
    }
    return () => {
      slot.listeners.delete(listener);
      this.maybeCollect(id);
    };
  }

  /**
   * Watch `key`, and keep it fresh while anybody is watching.
   *
   * Subscribing *is* the signal that a value is wanted, so the fetch belongs
   * here rather than in an effect beside it. That is what lets `useQuery` be a
   * plain `useSyncExternalStore` over this store — no effect to trigger the
   * request, no ref to hold the latest query function, and no dependency array
   * to argue with. The store owns its own freshness, which is what an external
   * store is for.
   *
   * The query function is captured per subscription, which is per key: a
   * component re-rendering with a new closure does not re-subscribe, and the
   * request for a given key does not change meaning between renders.
   */
  watch<T>(
    key: QueryKey,
    query: () => Promise<T>,
    options: {| +listener: () => void, +staleTime: number |},
  ): () => void {
    const id = hash(key);
    const slot = this.slot(id);
    slot.query = (query: $FlowFixMe);

    const unsubscribe = this.subscribe(key, options.listener);
    if (this.isStale(key, options.staleTime)) {
      // The rejection is recorded on the entry, and every watcher is told.
      // Letting it reach the console as well would report it twice.
      this.fetch(key, query).catch(() => {});
    }
    return unsubscribe;
  }

  /**
   * Run `query` for `key`, or join the request already in flight.
   *
   * The de-duplication is the whole point: two components mounting in the same
   * tick both call this, and one request is made.
   */
  fetch<T>(key: QueryKey, query: () => Promise<T>): Promise<T> {
    const id = hash(key);
    const slot = this.slot(id);
    if (slot.inFlight != null) {
      return (slot.inFlight: $FlowFixMe);
    }

    this.write(id, { pending: true });
    const promise = query().then(
      (value) => {
        slot.inFlight = null;
        this.write(id, { value, error: null, updatedAt: now(), pending: false });
        return value;
      },
      (thrown) => {
        slot.inFlight = null;
        const error = thrown instanceof Error ? thrown : new Error(String(thrown));
        // The previous value is kept beside the error. A failed refresh should
        // not blank a page that was showing something.
        this.write(id, { error, pending: false });
        throw error;
      },
    );
    slot.inFlight = promise;
    return promise;
  }

  /** Whether `key`'s value is older than `millis`. */
  isStale(key: QueryKey, millis: number): boolean {
    const entry = this.read(key);
    if (entry.updatedAt === 0) {
      return true;
    }
    return now() - entry.updatedAt >= millis;
  }

  /** Put a value in without running a query, for an optimistic update. */
  set<T>(key: QueryKey, value: T): void {
    this.write(hash(key), { value, error: null, updatedAt: now(), pending: false });
  }

  /**
   * Mark matching keys stale so their watchers refetch.
   *
   * A key *prefix* matches, because that is how invalidation is actually
   * expressed: after a mutation, `["users"]` should refresh `["users", 1]` and
   * `["users", 2]` without the caller listing them.
   */
  invalidate(prefix: QueryKey): void {
    const wanted = hash(prefix);
    for (const [id, slot] of Array.from(this.slots)) {
      if (id !== wanted && !id.startsWith(`${wanted.slice(0, -1)},`)) {
        continue;
      }
      // `updatedAt: 0` is "never fetched", which is what makes every reader
      // treat it as stale without inventing a separate flag.
      this.write(id, { updatedAt: 0 });

      // And refetch what somebody is looking at. Marking it stale alone would
      // leave a mounted component re-rendering, seeing that it is stale, and
      // doing nothing — the freshness is this store's job, not the
      // component's.
      const query = slot.query;
      if (query != null && slot.listeners.size > 0) {
        this.fetch(JSON.parse(id), query).catch(() => {});
      }
    }
  }

  /** Forget everything. */
  clear(): void {
    for (const slot of this.slots.values()) {
      if (slot.collect != null) {
        clearTimeout(slot.collect);
      }
    }
    this.slots.clear();
  }

  slot(id: string): Slot {
    let slot = this.slots.get(id);
    if (slot == null) {
      slot = {
        entry: EMPTY,
        listeners: new Set(),
        inFlight: null,
        collect: null,
        query: null,
      };
      this.slots.set(id, slot);
    }
    return slot;
  }

  write(id: string, patch: { +[string]: mixed }): void {
    const slot = this.slot(id);
    slot.entry = ({
      ...slot.entry,
      ...patch,
      version: slot.entry.version + 1,
    }: $FlowFixMe);
    for (const listener of Array.from(slot.listeners)) {
      if (slot.listeners.has(listener)) {
        listener();
      }
    }
  }

  maybeCollect(id: string): void {
    const slot = this.slots.get(id);
    if (slot == null || slot.listeners.size > 0 || slot.collect != null) {
      return;
    }
    // Kept for a while after the last watcher goes: navigating away and back
    // is the common case, and it should not cost a request.
    slot.collect = setTimeout(() => {
      const current = this.slots.get(id);
      if (current != null && current.listeners.size === 0) {
        this.slots.delete(id);
      }
    }, this.garbageMillis);
    // A timer must not hold the process open — a test that finishes before the
    // collection is due should still exit.
    if (typeof (slot.collect: $FlowFixMe)?.unref === "function") {
      (slot.collect: $FlowFixMe).unref();
    }
  }
}

/**
 * A key as a string.
 *
 * `JSON.stringify` of the array, so `["users", 1]` and `["users", "1"]` are
 * different keys — they are different requests, and treating them as one is a
 * bug that shows up as the wrong data rather than as an error.
 */
export function hash(key: QueryKey): string {
  return JSON.stringify(key);
}

function now(): number {
  return Date.now();
}
