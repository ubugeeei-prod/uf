// @flow
//
// `@uniflowed/query/observer`: turning an entry into something React can read.
//
// A [`Query`] holds facts. A component needs a *snapshot*: one immutable
// object, with the narrowing already applied, that is the same object as last
// time whenever nothing it contains has changed. The distance between those
// two is this module, and almost all of it is the word "same".
//
// # Why the snapshot has to be identical when nothing changed
//
// `useSyncExternalStore` compares the value `getSnapshot` returns with the one
// it returned before, using `Object.is`, and re-renders only if they differ.
// So an observer that builds a fresh result object every time it is asked
// re-renders its component on every notification, including the ones that say
// nothing changed — and, worse, the loop is not obviously wrong from the
// component's side. React's own diagnostic for it is "The result of
// getSnapshot should be cached to avoid an infinite loop".
//
// So [`QueryObserver.readResult`] builds the candidate, compares it shallowly
// with the one it returned last time, and returns the *old* object when they
// agree. Every field it compares is either a primitive or a value that has
// already been through structural sharing, so shallow identity is exactly the
// right question.
//
// That single rule is what makes the interesting behaviours fall out:
//
//   * A poll whose answer has not changed does not re-render anything.
//   * `setQueryData` re-renders the components watching that key and no
//     others, because no other entry's snapshot moved.
//   * A `select` that narrows to a field re-renders only when *that field*
//     changes, because the narrowed value goes through structural sharing too.
//
// # Why this object caches, and why that is not "mutation during render"
//
// `readResult` runs during render and writes to three private slots: the last
// result, the last `select` input and output, and the last real data seen (for
// `placeholderData`). None of that is state — it is a memo, invisible outside
// the hook that owns this observer, idempotent under React's double
// invocation, and identical in result whether the render is committed or
// thrown away. It is the same category as `useMemo`, and `useSyncExternalStore`
// cannot be used correctly without it.
//
// Everything that is *not* a memo — building the cache entry, starting a
// request, registering a listener, setting a timer — happens in
// [`QueryObserver.subscribe`], which React calls from an effect, and every one
// of them is undone by the function it returns.
//
// # Why options arrive as an argument during render and as a callback outside it
//
// A component passes a new options object on every render, usually with a
// fresh `queryFn` closure. Two things want it, and they want different
// versions:
//
//   * The snapshot wants *this* render's options, so it is passed in. An
//     observer that had cached them would narrow with a `select` from the
//     previous render, which is a stale render, which is a bug React cannot
//     see.
//   * A fetch started later — from an interval, from a refetch button, from
//     the tab regaining focus — wants the *latest* options. So it reads them
//     through a callback whose identity never changes and whose body is always
//     current. That is why a caller writing `queryFn` inline does not
//     resubscribe on every keystroke, and why the request it eventually makes
//     is not the one from three renders ago.

import { hashKey } from "./key.js";
import type { QueryKey } from "./key.js";
import { EMPTY_STATE } from "./query.js";
import type {
  FetchContext,
  FetchDirection,
  FetchStatus,
  Fetcher,
  Query,
  QueryState,
  QueryStatus,
} from "./query.js";
import type { RetryDelay, RetryPolicy } from "./retry.js";
import { shallowEqual, structuralShare } from "./structural.js";

// Type-only: an observer is handed its client, and never imports one.
import type { QueryClient } from "./client.js";

/**
 * How a component asks for a key.
 *
 * Everything except the key and the function has a default on the client, so
 * an application sets `staleTime` once instead of at every call site.
 */
export type QueryOptions<TData, TSelected = TData> = {|
  readonly queryKey: QueryKey,
  readonly queryFn: (context: FetchContext<TData>) => Promise<TData>,
  /** `false` means "do not fetch this yet"; a manual refetch still works. */
  readonly enabled?: boolean,
  /** How long an answer counts as fresh. `Infinity` means "until I say so". */
  readonly staleTime?: number,
  /** How long an unwatched entry is kept before it is collected. */
  readonly gcTime?: number,
  readonly retry?: RetryPolicy,
  readonly retryDelay?: RetryDelay,
  /** Narrow the data. See the module docs for what it buys. */
  readonly select?: (data: TData) => TSelected,
  /** Shown while there is nothing yet; never written to the cache. */
  readonly placeholderData?: mixed | ((previous: TData | void) => mixed),
  readonly refetchInterval?: number | null,
  readonly refetchOnWindowFocus?: boolean,
  readonly refetchOnReconnect?: boolean,
|};

/** The same options with the client's defaults filled in. */
export type ResolvedQueryOptions<TData, TSelected = TData> = {|
  readonly queryKey: QueryKey,
  readonly queryFn: (context: FetchContext<TData>) => Promise<TData>,
  readonly enabled: boolean,
  readonly staleTime: number,
  readonly gcTime: number,
  readonly retry: RetryPolicy,
  readonly retryDelay: RetryDelay,
  readonly select?: (data: TData) => TSelected,
  readonly placeholderData?: mixed | ((previous: TData | void) => mixed),
  readonly refetchInterval: number | null,
  readonly refetchOnWindowFocus: boolean,
  readonly refetchOnReconnect: boolean,
|};

/**
 * What a component sees.
 *
 * Deliberately without timestamps. A clock reading in a render-visible
 * snapshot re-renders every observer on every successful refresh, even when
 * the answer is identical — which is exactly what structural sharing exists to
 * prevent, and it also defeats `select`: narrowing to `user.name` is only
 * worth having if the snapshot is insensitive to the fields it discarded, and
 * a `dataUpdatedAt` beside it moves whenever *any* field does.
 *
 * So the snapshot carries decisions rather than readings. `isStale` is the one
 * the clock reaches, because it is the one a component can act on.
 * `client.getQueryState(key)` has `dataUpdatedAt`, `checkedAt` and the rest for
 * a devtool or a "last updated" label that wants them.
 */
export type QueryResult<T> = {|
  readonly data: T | void,
  readonly error: Error | null,
  readonly status: QueryStatus,
  readonly fetchStatus: FetchStatus,
  /** There is no answer yet, not even a failed one. */
  readonly isPending: boolean,
  /** The first load: pending *and* a request is in flight. */
  readonly isLoading: boolean,
  readonly isSuccess: boolean,
  readonly isError: boolean,
  /** A request is in flight, first load or refresh. */
  readonly isFetching: boolean,
  /** A request is in flight over data that is already on screen. */
  readonly isRefetching: boolean,
  readonly isStale: boolean,
  readonly isPlaceholderData: boolean,
  /** Failed attempts in the request in flight, for "retrying (2 of 3)". */
  readonly failureCount: number,
  /** Refetch now, superseding anything in flight. Never rejects. */
  readonly refetch: () => Promise<void>,
|};

export class QueryObserver<TData, TSelected = TData> {
  client: QueryClient;
  readonly getOptions: () => QueryOptions<TData, TSelected>;
  /** Stable for the observer's life, so it never changes a snapshot. */
  readonly refetch: () => Promise<void>;

  listener: (() => void) | null = null;
  query: Query<TData> | null = null;

  // The three memos. See the module docs: caches, not state.
  result: QueryResult<TSelected> | null = null;
  selection: {| raw: mixed, select: mixed, output: mixed |} | null = null;
  lastData: TData | void = undefined;

  staleTimer: TimeoutID | null = null;
  intervalTimer: IntervalID | null = null;
  stopPresence: (() => void) | null = null;

  constructor(client: QueryClient, getOptions: () => QueryOptions<TData, TSelected>) {
    this.client = client;
    this.getOptions = getOptions;
    this.refetch = () => this.fetch({ cancelRefetch: true });
  }

  /**
   * The snapshot for this render.
   *
   * Reads the cache without touching it: an entry that does not exist yet
   * stays not existing, and the component is told there is nothing yet. The
   * entry is built by [`subscribe`] one effect later.
   */
  readResult(
    client: QueryClient,
    options: ResolvedQueryOptions<TData, TSelected>,
  ): QueryResult<TSelected> {
    const query = client.cache.get(hashKey(options.queryKey)) as $FlowFixMe;
    const candidate = this.buildResult(query, query?.state ?? EMPTY_STATE, options);
    const previous = this.result;
    if (previous != null && shallowEqual(previous, candidate)) {
      return previous;
    }
    this.result = candidate;
    return candidate;
  }

  /**
   * Start watching, and keep the entry fresh while anybody is.
   *
   * Subscribing *is* the statement that a value is wanted, so the fetch
   * belongs here rather than in an effect beside it. That is what lets the
   * React binding be a `useSyncExternalStore` and nothing else: no effect to
   * trigger the request, no ref holding the latest query function, no
   * dependency array to argue with.
   */
  subscribe(client: QueryClient, listener: () => void): () => void {
    this.client = client;
    this.listener = listener;

    const options = this.resolved();
    const query = this.attach(options);

    if (options.enabled && query.isStale(options.staleTime)) {
      void this.fetch({ cancelRefetch: false });
    }
    this.updateStaleTimer(options);
    this.startInterval(options);
    this.watchPresence(client, options);

    return () => {
      this.stopTimers();
      this.stopPresence?.();
      this.stopPresence = null;
      this.listener = null;
      // Whatever it is watching *now*, which is not necessarily the entry it
      // started on: `attach` may have moved it since.
      const held = this.query;
      this.query = null;
      held?.removeObserver(this);
    };
  }

  /** The query changed. Whether that is a render is React's decision. */
  onQueryUpdate(): void {
    const options = this.resolved();
    this.attach(options, true);
    this.updateStaleTimer(options);
    this.listener?.();
  }

  /**
   * Make sure this observer is registered on the entry its key resolves to now.
   *
   * Usually a no-op. It is not one after `removeQueries` on a key somebody is
   * watching: the entry that was dropped is not coming back, and an observer
   * left holding it would be listening to an object nothing writes to any more
   * while the next fetch quietly filled its replacement. The dropped entry
   * announces its own destruction, which is what brings this path here, and
   * the component finds itself watching a fresh entry that is refetching.
   *
   * Only ever reached from an effect, a timer or an event — never from a
   * render, where building an entry would be a mutation.
   *
   * `refill` is what separates "make sure the registration is right before I
   * fetch" from "I have just been moved to an empty entry and somebody is
   * looking at it".
   *
   * The check is against the held entry's own hash rather than the options',
   * because this runs on every notification and hashing a key is a
   * `JSON.stringify`. It is the same question: while an observer is
   * subscribed, the entry it holds is the one its key hashes to, because the
   * key's hash is part of what makes a subscription different — a key that
   * changes tears this subscription down and builds another.
   */
  attach(options: ResolvedQueryOptions<TData, TSelected>, refill: boolean = false): Query<TData> {
    const held = this.query;
    if (held != null && this.client.cache.get(held.hash) === held) {
      return held;
    }

    const current = this.client.cache.build(options.queryKey, options.gcTime) as $FlowFixMe;
    if (this.listener == null) {
      return current;
    }
    held?.removeObserver(this);
    this.query = current;
    current.addObserver(this, options.gcTime);
    if (refill && options.enabled && current.isStale(options.staleTime)) {
      void this.fetch({ cancelRefetch: false });
    }
    return current;
  }

  isEnabled(): boolean {
    return this.listener != null && this.resolved().enabled;
  }

  /** Refetch on behalf of invalidation. Never rejects; errors land in state. */
  refetchNow(): Promise<void> {
    return this.fetch({ cancelRefetch: true });
  }

  /**
   * Ask for the key again.
   *
   * Never rejects. A failed request is a state a component renders, not an
   * exception a click handler has to catch — and a rejected promise nobody
   * awaited is an unhandled rejection in the console for something the UI is
   * already showing.
   */
  fetch(options: {|
    readonly cancelRefetch: boolean,
    readonly direction?: FetchDirection | null,
  |}): Promise<void> {
    const resolved = this.resolved();
    const direction = options.direction ?? null;
    // Through `attach`, so a fetch can never fill an entry this observer is
    // not registered on and then wonder why nothing re-rendered.
    const query = this.attach(resolved);
    return query
      .fetch(this.buildFetcher(resolved, direction), {
        retry: resolved.retry,
        retryDelay: resolved.retryDelay,
        cancelRefetch: options.cancelRefetch,
        direction,
      })
      .then(ignore, ignore);
  }

  /**
   * How the entry is refilled.
   *
   * The context is passed straight through rather than destructured, because
   * `signal` is a getter and reading it is what tells the cache the request
   * can be aborted. Unpacking it here would opt every query in, including the
   * ones that cannot honour it.
   */
  buildFetcher(
    options: ResolvedQueryOptions<TData, TSelected>,
    _direction: FetchDirection | null,
  ): Fetcher<TData> {
    return (context) => options.queryFn(context);
  }

  buildResult(
    query: Query<TData> | void,
    state: QueryState<TData>,
    options: ResolvedQueryOptions<TData, TSelected>,
  ): QueryResult<TSelected> {
    let raw: mixed = state.data;
    let status = state.status;
    let isPlaceholderData = false;

    // Remembered so `placeholderData` can be given the previous answer: that
    // is how a paged list keeps the page it is showing while the next one
    // loads, instead of blanking between pages.
    if (status === "success" && state.data !== undefined) {
      this.lastData = state.data;
    }

    if (status === "pending" && options.placeholderData !== undefined) {
      const placeholder =
        typeof options.placeholderData === "function"
          ? options.placeholderData(this.lastData)
          : options.placeholderData;
      if (placeholder !== undefined) {
        raw = placeholder;
        status = "success";
        isPlaceholderData = true;
      }
    }

    const isFetching = state.fetchStatus === "fetching";
    const isPending = status === "pending";
    return {
      data: raw === undefined ? undefined : (this.narrow(raw, options.select) as $FlowFixMe),
      error: state.error,
      status,
      fetchStatus: state.fetchStatus,
      isPending,
      isLoading: isPending && isFetching,
      isSuccess: status === "success",
      isError: status === "error",
      isFetching,
      isRefetching: isFetching && !isPending,
      isStale: query == null || query.isStale(options.staleTime),
      isPlaceholderData,
      failureCount: state.failureCount,
      refetch: this.refetch,
    };
  }

  /**
   * Apply `select`, keeping the narrowed value's identity when it can.
   *
   * Two layers, because callers write `select` inline and a fresh closure each
   * render defeats a memo keyed on identity alone. So: reuse the output when
   * both the data and the function are the same, and otherwise recompute and
   * hand the result through structural sharing — which returns the previous
   * narrowed value when the new one is deeply equal to it. The first layer
   * makes it cheap; the second makes it *correct*, and correctness here is
   * what stops a component from re-rendering on data it does not read.
   */
  narrow(raw: mixed, select: ((data: TData) => TSelected) | void): mixed {
    if (select == null) {
      return raw;
    }
    const memo = this.selection;
    if (memo != null && memo.raw === raw && memo.select === select) {
      return memo.output;
    }
    const computed = select(raw as $FlowFixMe);
    const output = memo == null ? computed : structuralShare(memo.output, computed);
    this.selection = { raw, select, output };
    return output;
  }

  resolved(): ResolvedQueryOptions<TData, TSelected> {
    return this.client.resolveQuery(this.getOptions());
  }

  /**
   * Wake up when the entry turns stale.
   *
   * Staleness is a fact about the clock, and nothing else in this package ever
   * looks at the clock on its own. Without this timer a component that shows
   * "this may be out of date" would be told only by the next unrelated
   * re-render, which may never come.
   */
  updateStaleTimer(options: ResolvedQueryOptions<TData, TSelected>): void {
    if (this.staleTimer != null) {
      clearTimeout(this.staleTimer);
      this.staleTimer = null;
    }
    const query = this.query;
    if (query == null || this.listener == null) {
      return;
    }
    const { staleTime } = options;
    if (staleTime <= 0 || staleTime === Number.POSITIVE_INFINITY) {
      return;
    }
    const remaining = query.state.checkedAt + staleTime - Date.now();
    if (query.state.checkedAt === 0 || remaining <= 0) {
      return;
    }
    this.staleTimer = setTimeout(() => {
      this.staleTimer = null;
      this.listener?.();
    }, remaining + 1);
    (this.staleTimer as $FlowFixMe)?.unref?.();
  }

  startInterval(options: ResolvedQueryOptions<TData, TSelected>): void {
    const interval = options.refetchInterval;
    if (interval == null || interval <= 0 || !options.enabled) {
      return;
    }
    this.intervalTimer = setInterval(() => {
      void this.fetch({ cancelRefetch: false });
    }, interval);
    (this.intervalTimer as $FlowFixMe)?.unref?.();
  }

  /**
   * Refresh when the reader comes back, or the network does.
   *
   * Only when the entry is actually stale: coming back to a tab that was
   * hidden for two seconds should not refetch anything, and `staleTime` is
   * already the application's statement about how long an answer is good for.
   */
  watchPresence(client: QueryClient, options: ResolvedQueryOptions<TData, TSelected>): void {
    if (!options.refetchOnWindowFocus && !options.refetchOnReconnect) {
      return;
    }
    this.stopPresence = client.presence.subscribe((event) => {
      const current = this.resolved();
      const wanted = event === "focus" ? current.refetchOnWindowFocus : current.refetchOnReconnect;
      const query = this.query;
      if (!wanted || !current.enabled || query == null || !query.isStale(current.staleTime)) {
        return;
      }
      void this.fetch({ cancelRefetch: false });
    });
  }

  stopTimers(): void {
    if (this.staleTimer != null) {
      clearTimeout(this.staleTimer);
      this.staleTimer = null;
    }
    if (this.intervalTimer != null) {
      clearInterval(this.intervalTimer);
      this.intervalTimer = null;
    }
  }
}

function ignore(): void {}
