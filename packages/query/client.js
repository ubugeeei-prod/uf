// @flow
//
// `@uniflowed/query/client`: the cache as something you can talk to.
//
// A [`QueryClient`] is the cache plus the two things a cache is useless
// without: the application's defaults, and a vocabulary for acting on entries
// from outside React. Everything here is callable from an event handler, a
// route loader, a test, or a server render — nothing on this object needs a
// component to exist.
//
// # Why defaults live here rather than at the call site
//
// `staleTime` is an application-wide decision — how long is an answer good for
// around here — and repeating it at three hundred call sites means the three
// hundred and first is different and nobody knows why. So the client holds
// them and every option falls back to them, one level, with no merging of
// nested objects to reason about.
//
// # Why `setQueryData` takes an updater
//
// The value being replaced is the one in the cache *now*, which is not
// necessarily the one the component rendered — a background refresh may have
// landed in between. Reading it out, changing it, and writing it back would
// then silently discard that refresh. The updater form reads and writes in the
// same step, and returning `undefined` from it means "on reflection, do
// nothing", which is how an optimistic update declines to guess about an entry
// that has since been dropped.
//
// # Why invalidation refetches only what is on screen
//
// `invalidateQueries(["users"])` in an application holding a hundred cached
// users must not make a hundred requests. Every match is *marked* stale, so
// whichever of them is looked at next refetches on sight; only the ones with a
// live observer are refetched now. The distinction is what makes invalidation
// by prefix safe to use freely, which in turn is what makes it the right way
// to express "something about users changed".

import { QueryCache } from "./cache.js";
import type { QueryFilters } from "./cache.js";
import { hashKey } from "./key.js";
import type { QueryKey } from "./key.js";
import type { QueryOptions, ResolvedQueryOptions } from "./observer.js";
import { Presence } from "./presence.js";
import { DEFAULT_GC_TIME } from "./query.js";
import type { FetchContext, QueryState } from "./query.js";
import type { MutationOptions, ResolvedMutationOptions } from "./mutation.js";
import { backoffDelay } from "./retry.js";
import type { RetryDelay, RetryPolicy } from "./retry.js";

/** What a whole application decides once. */
export type QueryDefaults = {|
  readonly staleTime?: number,
  readonly gcTime?: number,
  readonly retry?: RetryPolicy,
  readonly retryDelay?: RetryDelay,
  readonly refetchInterval?: number | null,
  readonly refetchOnWindowFocus?: boolean,
  readonly refetchOnReconnect?: boolean,
|};

export type MutationDefaults = {|
  readonly retry?: RetryPolicy,
  readonly retryDelay?: RetryDelay,
|};

export type QueryClientOptions = {|
  readonly queries?: QueryDefaults,
  readonly mutations?: MutationDefaults,
  /** Replaceable so React Native and tests can drive focus themselves. */
  readonly presence?: Presence,
|};

/** What `fetchQuery` needs, which is a query without anything React-shaped. */
export type FetchQueryOptions<TData> = {|
  readonly queryKey: QueryKey,
  readonly queryFn: (context: FetchContext<TData>) => Promise<TData>,
  readonly staleTime?: number,
  readonly gcTime?: number,
  readonly retry?: RetryPolicy,
  readonly retryDelay?: RetryDelay,
|};

/**
 * Three retries with doubling backoff, five minutes of grace, fresh for no
 * time at all.
 *
 * `staleTime: 0` is the conservative default and the surprising one: a mount
 * refetches, showing the cached answer immediately and replacing it when the
 * new one lands. Applications that know better should say so — `staleTime` is
 * the single most valuable option on this object.
 */
const QUERY_DEFAULTS = {
  staleTime: 0,
  gcTime: DEFAULT_GC_TIME,
  retry: (3: RetryPolicy),
  retryDelay: (backoffDelay: RetryDelay),
  refetchInterval: null,
  refetchOnWindowFocus: true,
  refetchOnReconnect: true,
};

/**
 * Writes are not retried by default.
 *
 * A failed read can be repeated because reading twice is free. A failed write
 * may well have succeeded on the server and lost its answer on the way back,
 * and repeating it creates the second invoice. Retrying a mutation is a
 * decision about idempotency that only the caller can make.
 */
const MUTATION_DEFAULTS = {
  retry: (false: RetryPolicy),
  retryDelay: (backoffDelay: RetryDelay),
};

export class QueryClient {
  readonly cache: QueryCache = new QueryCache();
  readonly presence: Presence;
  readonly queryDefaults: typeof QUERY_DEFAULTS;
  readonly mutationDefaults: typeof MUTATION_DEFAULTS;

  constructor(options?: QueryClientOptions) {
    this.presence = options?.presence ?? new Presence();
    this.queryDefaults = { ...QUERY_DEFAULTS, ...stripUndefined(options?.queries) };
    this.mutationDefaults = { ...MUTATION_DEFAULTS, ...stripUndefined(options?.mutations) };
  }

  /** The options with this client's defaults filled in. A pure function. */
  resolveQuery<TData, TSelected>(
    options: QueryOptions<TData, TSelected>,
  ): ResolvedQueryOptions<TData, TSelected> {
    const defaults = this.queryDefaults;
    return {
      queryKey: options.queryKey,
      queryFn: options.queryFn,
      enabled: options.enabled ?? true,
      staleTime: options.staleTime ?? defaults.staleTime,
      gcTime: options.gcTime ?? defaults.gcTime,
      retry: options.retry ?? defaults.retry,
      retryDelay: options.retryDelay ?? defaults.retryDelay,
      select: options.select,
      placeholderData: options.placeholderData,
      refetchInterval: options.refetchInterval ?? defaults.refetchInterval,
      refetchOnWindowFocus: options.refetchOnWindowFocus ?? defaults.refetchOnWindowFocus,
      refetchOnReconnect: options.refetchOnReconnect ?? defaults.refetchOnReconnect,
    };
  }

  resolveMutation<TVariables, TData, TContext>(
    options: MutationOptions<TVariables, TData, TContext>,
  ): ResolvedMutationOptions<TVariables, TData, TContext> {
    return {
      ...options,
      retry: options.retry ?? this.mutationDefaults.retry,
      retryDelay: options.retryDelay ?? this.mutationDefaults.retryDelay,
    };
  }

  /**
   * What is cached for `key`, without asking for it.
   *
   * `mixed`, not a generic the caller instantiates. A key is an array of
   * strings and numbers; it carries no type, and a signature that pretended
   * otherwise would be an unchecked cast wearing a type parameter. Narrow it
   * where you read it, with the same schema that validated the response.
   */
  getQueryData(key: QueryKey): mixed {
    return this.cache.get(hashKey(key))?.state.data;
  }

  /** Everything known about `key`, including the timestamps a result omits. */
  getQueryState(key: QueryKey): QueryState<mixed> | void {
    return this.cache.get(hashKey(key))?.state;
  }

  /**
   * Put a value in, creating the entry if there is none.
   *
   * The write goes through structural sharing, so setting data that is deeply
   * equal to what is there changes no identity and re-renders nothing.
   */
  setQueryData(key: QueryKey, updater: mixed | ((previous: mixed) => mixed)): mixed {
    const query = this.cache.build(key, this.queryDefaults.gcTime);
    const next =
      typeof updater === "function" ? (updater as $FlowFixMe)(query.state.data) : updater;
    if (next === undefined) {
      return query.state.data;
    }
    return query.setData(next);
  }

  /**
   * Fetch `key` now unless it is fresh, and hand back the answer.
   *
   * This is the imperative half of `useQuery`: the same cache, the same
   * de-duplication, no component. A route loader that calls this before
   * navigating hands the component a cache that is already warm, and the
   * component's own mount finds nothing to do.
   */
  fetchQuery<TData>(options: FetchQueryOptions<TData>): Promise<mixed> {
    const resolved = this.resolveQuery(options as $FlowFixMe);
    const query = this.cache.build(resolved.queryKey, resolved.gcTime);
    if (!query.isStale(resolved.staleTime)) {
      return Promise.resolve(query.state.data);
    }
    return query.fetch((context) => resolved.queryFn(context as $FlowFixMe), {
      retry: resolved.retry,
      retryDelay: resolved.retryDelay,
      cancelRefetch: false,
    });
  }

  /**
   * The same thing, for when the answer is not wanted here.
   *
   * Never rejects: a prefetch is an optimisation, and an optimisation that can
   * take down the page that started it is not one. The failure is recorded on
   * the entry, where the component that eventually reads it will find it.
   */
  prefetchQuery<TData>(options: FetchQueryOptions<TData>): Promise<void> {
    return this.fetchQuery(options).then(ignore, ignore);
  }

  /**
   * Mark matching entries stale, and refetch the ones being watched.
   *
   * The default filter is "everything", which is the right thing after signing
   * in or out.
   */
  invalidateQueries(filters?: QueryFilters): Promise<void> {
    for (const query of this.cache.findAll(filters)) {
      query.invalidate();
    }
    return this.refetchQueries({ ...(filters ?? {}), type: "active" });
  }

  /** Refetch matching entries now. Never rejects. */
  refetchQueries(filters?: QueryFilters): Promise<void> {
    const matched = this.cache.findAll(filters);
    return Promise.all(matched.map((query) => query.refetch())).then(ignore);
  }

  /**
   * Abort matching requests and put their entries back as they were.
   *
   * The call an optimistic update makes first: a refetch already in flight
   * would otherwise land after the optimistic write and replace it with the
   * server's previous answer, which looks exactly like the mutation being
   * undone at random.
   */
  cancelQueries(filters?: QueryFilters): Promise<void> {
    for (const query of this.cache.findAll(filters)) {
      query.cancel({ revert: true });
    }
    return Promise.resolve();
  }

  /** Drop matching entries entirely, cancelling anything in flight. */
  removeQueries(filters?: QueryFilters): void {
    for (const query of this.cache.findAll(filters)) {
      this.cache.remove(query);
    }
  }

  /** How many matching requests are in flight, for a global progress bar. */
  isFetching(filters?: QueryFilters): number {
    return this.cache.findAll(filters).filter((query) => query.state.fetchStatus === "fetching")
      .length;
  }

  /** Forget everything. A test's `afterEach`, or a sign-out. */
  clear(): void {
    this.cache.clear();
  }
}

function stripUndefined<T: { +[string]: mixed }>(source: T | void): { [string]: mixed } {
  const out: { [string]: mixed } = {};
  if (source == null) {
    return out;
  }
  for (const name of Object.keys(source)) {
    if (source[name] !== undefined) {
      out[name] = source[name];
    }
  }
  return out;
}

function ignore(): void {}
