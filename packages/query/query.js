// @flow
//
// `@uniflowed/query/query`: one key, and everything that can happen to it.
//
// A [`Query`] is the entry behind a single key: the last answer, the last
// failure, whether a request is in flight, who is watching, and the timer that
// will throw it away when nobody is. It is deliberately ignorant of React —
// every decision here is one a cache has to make whether or not anything is
// rendering, and keeping them here is what makes them testable without a DOM.
//
// # Why the state is replaced rather than edited
//
// `state` is a new object on every transition and is never mutated in place.
// That is not ceremony: React reads this store through
// `useSyncExternalStore`, whose contract is that a snapshot is immutable and
// comparable by identity. An entry edited in place would hand React the same
// object it already had, React would see no change and skip the render, and
// the screen would sit on data the cache had already replaced. The bug looks
// like "sometimes it doesn't update", which is the hardest kind to find.
//
// # Why one entry can only have one request in flight
//
// Two components mounting in the same tick both want `["user", 1]`. If each
// gets its own request the application makes two calls, and worse, the answers
// can arrive in either order — so the two components can end up rendering
// different versions of the same key. Joining the second caller to the first
// request is what makes a shared cache *coherent*, not just cheaper.
//
// # Why a superseded fetch cannot write
//
// Every request carries the `fetchId` it was started with, and only the
// current one may touch the state. Without that, a refetch that overtakes the
// request it replaced writes the *older* answer last and it sticks. This is
// the single most common way a hand-rolled cache goes wrong, and it does not
// reproduce on a fast connection.
//
// # Why cancelling depends on whether the query function looked at the signal
//
// `signal` on the fetch context is a getter, and reading it is treated as the
// query function opting in to cancellation. When the last observer leaves, an
// opted-in request is aborted — that is the point of `fetch(url, {signal})`.
// A query function that ignores the signal cannot be stopped, so aborting it
// would only throw away an answer that is already on its way and that the next
// mount would have to ask for again. The flag is how one mechanism serves both
// without asking the caller to configure it.
//
// An explicit `cancelQueries` aborts either way: there the caller has said
// what they want, and the cache does not get to second-guess it.

import { hashKey } from "./key.js";
import type { QueryKey } from "./key.js";
import { CancelledError, asError, runWithRetry } from "./retry.js";
import type { RetryDelay, RetryPolicy } from "./retry.js";
import { structuralShare } from "./structural.js";

// Type-only: the cache constructs queries and queries ask it to drop them, but
// nothing here needs the class at runtime, so the modules do not form a cycle.
import type { QueryCache } from "./cache.js";

/** Whether the entry has an answer yet, and whether that answer is a failure. */
export type QueryStatus = "pending" | "success" | "error";

/** Whether a request is in flight, independent of whether there is data. */
export type FetchStatus = "idle" | "fetching";

/** Which end of a paged query is being extended. */
export type FetchDirection = "forward" | "backward";

/**
 * Everything known about one key.
 *
 * `dataUpdatedAt` and `checkedAt` are two different facts and conflating them
 * is a mistake with a visible cost. `checkedAt` is when the server was last
 * asked and answered; it is what staleness is measured from. `dataUpdatedAt`
 * is when the answer last *changed*. A poll that confirms the same rows moves
 * `checkedAt` and leaves `dataUpdatedAt` alone — which is exactly what lets a
 * component that displays "updated 2 minutes ago" stay still while the cache
 * quietly keeps itself fresh underneath.
 */
export type QueryState<T> = {|
  readonly status: QueryStatus,
  readonly fetchStatus: FetchStatus,
  readonly data: T | void,
  readonly dataUpdatedAt: number,
  readonly checkedAt: number,
  readonly error: Error | null,
  readonly errorUpdatedAt: number,
  /** Failed attempts in the current request, reset when one is started. */
  readonly failureCount: number,
  readonly failureReason: Error | null,
  /** Set by invalidation: stale regardless of the clock. */
  readonly invalidated: boolean,
  readonly direction: FetchDirection | null,
|};

/**
 * What a query function is called with.
 *
 * `signal` is a getter; see the module docs for what reading it means.
 * `previousData` is what the cache holds right now, which a paged query needs
 * and an ordinary one can use for a conditional request.
 */
export type FetchContext<T> = {|
  readonly queryKey: QueryKey,
  readonly signal: AbortSignal,
  readonly previousData: T | void,
  readonly failureCount: number,
|};

/** How an entry is refilled. One attempt; retrying is [`runWithRetry`]'s job. */
export type Fetcher<T> = (context: FetchContext<T>) => Promise<T>;

/**
 * What a query needs from the thing watching it.
 *
 * An interface rather than a concrete observer type, so this module does not
 * depend on the React-facing one. A query has to be able to say "somebody
 * refresh me" during invalidation, and it must not have to know what a React
 * hook is to say it.
 */
export interface QueryWatcher {
  /** The state changed. Whether that is worth a render is the watcher's call. */
  onQueryUpdate(): void;
  /** Whether this watcher currently wants the key fetched at all. */
  isEnabled(): boolean;
  /** Refetch now, superseding anything in flight. Never rejects. */
  refetchNow(): Promise<void>;
}

/** How long an unobserved entry is kept, so a quick navigation back is free. */
export const DEFAULT_GC_TIME: number = 5 * 60_000;

/**
 * What is known about a key nobody has fetched.
 *
 * One shared object rather than one per entry, and safe to share precisely
 * because state is replaced rather than edited. It is also what a component
 * reads on its first render, before the effect that builds the entry has run —
 * reading a key that does not exist yet must not create it.
 */
export const EMPTY_STATE: QueryState<empty> = Object.freeze({
  status: "pending",
  fetchStatus: "idle",
  data: undefined,
  dataUpdatedAt: 0,
  checkedAt: 0,
  error: null,
  errorUpdatedAt: 0,
  failureCount: 0,
  failureReason: null,
  invalidated: false,
  direction: null,
});

export class Query<T> {
  readonly cache: QueryCache;
  readonly key: QueryKey;
  readonly hash: string;

  state: QueryState<T>;
  gcTime: number;

  observers: Array<QueryWatcher> = [];

  /** The request in flight, which a second caller joins rather than repeats. */
  pending: Promise<T | void> | null = null;
  controller: AbortController | null = null;
  /** Whether the query function read `signal`. See the module docs. */
  signalConsumed: boolean = false;
  /** Only the current request may write. Bumped by every start and cancel. */
  fetchId: number = 0;
  /** The state to put back if the request in flight is cancelled and reverted. */
  restore: QueryState<T> | null = null;

  gcTimer: TimeoutID | null = null;

  constructor(cache: QueryCache, key: QueryKey, gcTime: number = DEFAULT_GC_TIME) {
    this.cache = cache;
    this.key = key;
    this.hash = hashKey(key);
    this.gcTime = gcTime;
    this.state = EMPTY_STATE;
    // Scheduled from the moment it exists, not from the moment an observer
    // leaves: an entry created by a prefetch that nothing ever mounted has no
    // observer to lose, and without this it would sit in the map forever. The
    // first `addObserver` clears it.
    this.scheduleGc();
  }

  /**
   * Fill the entry, or join the request that is already doing it.
   *
   * `cancelRefetch` is the difference between "somebody mounted and wants this
   * key" and "somebody pressed refresh". The first joins whatever is in
   * flight; the second replaces it, because a reader who asked for fresh data
   * after typing into a filter must not be given the answer to the previous
   * filter just because it was already on its way.
   */
  fetch(
    fetcher: Fetcher<T>,
    options: {|
      readonly retry: RetryPolicy,
      readonly retryDelay: RetryDelay,
      readonly cancelRefetch?: boolean,
      readonly direction?: FetchDirection | null,
    |},
  ): Promise<T | void> {
    if (this.pending != null) {
      if (options.cancelRefetch !== true) {
        return this.pending;
      }
      this.cancel({ revert: false });
    }

    const id = this.fetchId + 1;
    this.fetchId = id;
    const controller = new AbortController();
    this.controller = controller;
    this.signalConsumed = false;
    this.restore = this.state;
    this.setState({
      fetchStatus: "fetching",
      direction: options.direction ?? null,
      failureCount: 0,
      failureReason: null,
    });

    const promise = this.run(id, controller, fetcher, options);
    this.pending = promise;
    return promise;
  }

  /**
   * The body of one request, from the first attempt to the state it leaves.
   *
   * Separate from [`fetch`] only because it is `async`: `fetch` has to assign
   * `pending` synchronously — a second caller in the same tick is exactly the
   * case de-duplication exists for — and an `async` function would not have
   * returned yet at the point that assignment has to happen.
   */
  async run(
    id: number,
    controller: AbortController,
    fetcher: Fetcher<T>,
    options: {| readonly retry: RetryPolicy, readonly retryDelay: RetryDelay |},
  ): Promise<T | void> {
    try {
      const value = await runWithRetry({
        attempt: (failureCount) => fetcher(this.contextFor(controller, failureCount)),
        retry: options.retry,
        retryDelay: options.retryDelay,
        signal: controller.signal,
        onFailure: (failureCount, error) => {
          if (this.fetchId === id) {
            this.setState({ failureCount, failureReason: error });
          }
        },
      });

      // Superseded while it was in flight. The newer request owns the entry;
      // writing here would put the older answer down last.
      if (this.fetchId !== id) {
        return this.state.data;
      }
      if (value === undefined) {
        // Almost always a query function that forgot to return. Reported as a
        // failure rather than stored, because `undefined` in the cache is
        // indistinguishable from "nothing has been fetched" and would leave the
        // entry loading forever.
        throw new Error(
          `the query function for ${this.hash} resolved with undefined; return null for "there is no such thing"`,
        );
      }
      this.settle(id);
      this.setData(value, Date.now());
      return this.state.data;
    } catch (thrown) {
      const error = asError(thrown);
      if (this.fetchId !== id) {
        throw error;
      }
      this.settle(id);
      // A cancellation is not a failure to report: `cancel` has already put the
      // entry back the way the reader asked for, and recording an error would
      // paint a message over data that is still perfectly good.
      if (!(error instanceof CancelledError)) {
        this.setState({
          status: "error",
          error,
          errorUpdatedAt: Date.now(),
          fetchStatus: "idle",
          direction: null,
        });
      }
      throw error;
    }
  }

  /**
   * Stop the request in flight.
   *
   * `revert` puts the entry back as it was before the request started, which
   * is what an interrupted refresh should look like: the reader keeps what
   * they were reading and no spinner is left running. Without it the entry
   * would sit at `fetching` forever, because the request that was going to
   * clear that flag is the one being thrown away.
   */
  cancel(options?: {| readonly revert?: boolean |}): void {
    const controller = this.controller;
    const restore = this.restore;

    // Before the abort, so the rejection it causes already sees an id that
    // says "you no longer own this entry".
    this.fetchId += 1;
    this.pending = null;
    this.controller = null;
    this.restore = null;
    controller?.abort(new CancelledError());

    if (options?.revert === true && restore != null) {
      this.state = restore;
      this.notify();
      return;
    }
    if (this.state.fetchStatus !== "idle") {
      this.setState({ fetchStatus: "idle", direction: null });
    }
  }

  /**
   * Put a value in without asking anybody.
   *
   * The value goes through structural sharing first, so writing data that is
   * deeply equal to what is already there changes nothing observable — no new
   * identity, no re-render, no memoised child re-running. That is what makes
   * an optimistic update that guessed right free, and it is why `checkedAt`
   * moves while `dataUpdatedAt` does not.
   */
  setData(value: T, at: number = Date.now()): T | void {
    const shared = structuralShare(this.state.data, value);
    const changed = shared !== this.state.data || this.state.status !== "success";
    this.setState({
      data: shared,
      status: "success",
      error: null,
      errorUpdatedAt: 0,
      fetchStatus: "idle",
      direction: null,
      invalidated: false,
      failureCount: 0,
      failureReason: null,
      dataUpdatedAt: changed ? at : this.state.dataUpdatedAt,
      checkedAt: at,
    });
    return this.state.data;
  }

  /** Mark the entry stale whatever the clock says. */
  invalidate(): void {
    if (!this.state.invalidated) {
      this.setState({ invalidated: true });
    }
  }

  /**
   * Whether the entry is old enough to be worth asking again.
   *
   * An entry that has never been answered is stale, an invalidated one is
   * stale, and `Infinity` means never. Note that this is measured from
   * `checkedAt`: a refresh that returned identical data still counts as having
   * checked, or a poll over unchanging data would refetch on every render.
   */
  isStale(staleTime: number): boolean {
    if (this.state.invalidated || this.state.checkedAt === 0) {
      return true;
    }
    if (staleTime === Number.POSITIVE_INFINITY) {
      return false;
    }
    return Date.now() - this.state.checkedAt >= staleTime;
  }

  /** Whether anything currently wants this key. */
  isActive(): boolean {
    return this.observers.some((watcher) => watcher.isEnabled());
  }

  /**
   * Refetch on behalf of whoever is watching.
   *
   * Invalidation reaches entries, not components, so the entry has to be able
   * to ask. The first enabled watcher's request speaks for the key: they all
   * fetch the same key, and de-duplication joins the rest to it.
   */
  refetch(): Promise<void> {
    const watcher = this.observers.find((candidate) => candidate.isEnabled());
    return watcher == null ? Promise.resolve() : watcher.refetchNow();
  }

  addObserver(watcher: QueryWatcher, gcTime: number): void {
    this.clearGc();
    // The longest-lived observer wins, because dropping the entry while a
    // component that asked to keep it is still mounted is the one outcome
    // neither of them wanted.
    this.gcTime = Math.max(this.gcTime, gcTime);
    if (!this.observers.includes(watcher)) {
      this.observers.push(watcher);
    }
  }

  removeObserver(watcher: QueryWatcher): void {
    const index = this.observers.indexOf(watcher);
    if (index < 0) {
      return;
    }
    this.observers.splice(index, 1);
    if (this.observers.length > 0) {
      return;
    }
    if (this.pending != null && this.signalConsumed) {
      this.cancel({ revert: true });
    }
    // Unless this entry has already been dropped — which is how the last
    // observer usually leaves a removed one. Scheduling its collection would
    // leave a timer holding a dead entry for the whole `gcTime`, to collect
    // something the cache stopped answering for long ago.
    if (this.cache.get(this.hash) === this) {
      this.scheduleGc();
    }
  }

  /** Replace the state and tell everybody. */
  setState(patch: { +[string]: mixed }): void {
    this.state = { ...this.state, ...patch } as $FlowFixMe;
    this.notify();
  }

  notify(): void {
    // A copy, because a watcher may well unsubscribe in response — a component
    // unmounting on the error it was just told about is an ordinary thing.
    for (const watcher of this.observers.slice()) {
      watcher.onQueryUpdate();
    }
  }

  /**
   * Schedule the entry's removal, now that nobody is watching.
   *
   * Kept for a while rather than dropped immediately, because navigating away
   * and straight back is the common case and it should not cost a request.
   */
  scheduleGc(): void {
    this.clearGc();
    if (this.gcTime === Number.POSITIVE_INFINITY) {
      return;
    }
    this.gcTimer = setTimeout(() => {
      this.gcTimer = null;
      if (this.observers.length === 0) {
        this.cache.remove(this);
      }
    }, this.gcTime);
    // A pending collection must not hold the process open: a script that has
    // finished its work should exit, not wait five minutes to throw away a
    // cache it is about to lose anyway.
    (this.gcTimer as $FlowFixMe)?.unref?.();
  }

  clearGc(): void {
    if (this.gcTimer != null) {
      clearTimeout(this.gcTimer);
      this.gcTimer = null;
    }
  }

  /**
   * Give up on the entry entirely: no request, no timer, nothing kept.
   *
   * The watchers are told, and told *after* the cache has stopped answering
   * for this key. Usually there are none — collection only happens once the
   * last one has gone. When there are, it is because `removeQueries` reached a
   * key somebody is looking at, and this notification is how they find out
   * that the entry they hold is not the one their key means any more.
   */
  destroy(): void {
    this.clearGc();
    this.cancel({ revert: false });
    this.notify();
  }

  settle(id: number): void {
    if (this.fetchId === id) {
      this.pending = null;
      this.controller = null;
      this.restore = null;
    }
  }

  contextFor(controller: AbortController, failureCount: number): FetchContext<T> {
    const query = this;
    return {
      queryKey: this.key,
      previousData: this.state.data,
      failureCount,
      // The one getter in this package, and the effect it hides is the point:
      // asking for the signal *is* how a query function says it can be
      // cancelled. The alternative is an option the caller has to remember to
      // set beside the `signal` they already passed to `fetch`, which would be
      // wrong by default in whichever direction it defaulted. Nothing that
      // renders ever reads this object.
      // uf-lint-disable-next-line flow/unsafe-getters-setters
      get signal(): AbortSignal {
        query.signalConsumed = true;
        return controller.signal;
      },
    };
  }
}
