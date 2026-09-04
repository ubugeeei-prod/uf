// @flow
//
// The React binding: `useQuery`, `useMutation`, and the provider they read.
//
// Deliberately thin. The cache is an external store, so `useQuery` is a
// `useSyncExternalStore` over it and nothing else — no effect to trigger the
// request, no ref holding the latest query function, no dependency array to
// argue with. Subscribing is the signal that a value is wanted, so keeping it
// fresh is the store's job; the component only reads.
//
// That is not a stylistic preference. A component that reads an external store
// through the API React provides for it gets the value React commits with,
// which is what stops two components sharing a key from rendering different
// versions of it, and it states the server's value rather than falling through
// to it.

import * as React from "@uniflowed/react";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  useSyncExternalStore,
} from "@uniflowed/react";

import { useStableCallback } from "@uniflowed/hooks";

import { QueryCache, hash } from "./cache.js";
import type { Entry, QueryKey } from "./cache.js";

const CacheContext: React.Context<QueryCache | null> = createContext(null);

/**
 * Make a cache available to the tree.
 *
 * Required rather than falling back to a module-level default: a default is
 * shared with every test in the process, and one test's cached answer then
 * decides another's result.
 */
export component QueryProvider(cache: QueryCache, children: React.Node) {
  return <CacheContext.Provider value={cache}>{children}</CacheContext.Provider>;
}

/** The cache this subtree uses. */
export function useQueryCache(): QueryCache {
  const cache = useContext(CacheContext);
  if (cache == null) {
    throw new Error(
      "useQuery needs a QueryProvider above it; render <QueryProvider cache={new QueryCache()}>",
    );
  }
  return cache;
}

/** What a query looks like to a component. */
export type QueryResult<T> = {|
  readonly value: T | void,
  readonly error: Error | null,
  /** A request is in flight. True on the first load and on a refresh. */
  readonly pending: boolean,
  /** There has never been a value, so there is nothing to show yet. */
  readonly loading: boolean,
  /** The value is older than `staleTime`. */
  readonly stale: boolean,
  readonly refetch: () => Promise<T>,
|};

/** How a query behaves. */
export type QueryOptions = {|
  /** How long a value is fresh. Defaults to none, so a mount refetches. */
  readonly staleTime?: number,
|};

/**
 * Read `key`, fetching it when it is missing or stale.
 *
 * A cached value is returned immediately and refreshed behind it, so
 * navigating back to a page shows it at once. `pending` says a request is in
 * flight; `loading` says there is nothing to show yet — conflating those is
 * why applications flash a spinner over data they already have.
 */
export function useQuery<T>(
  key: QueryKey,
  query: () => Promise<T>,
  options?: QueryOptions,
): QueryResult<T> {
  const cache = useQueryCache();
  const staleTime = options?.staleTime ?? 0;

  // The key's identity is its hash. A caller writes the array inline, so
  // depending on the array itself would resubscribe on every render; two
  // arrays with the same hash are the same request by definition, so holding
  // the first one is not a stale closure.
  const id = hash(key);
  const stable = useMemo(() => key, [id]); // eslint-disable-line react-hooks/exhaustive-deps

  // The query is something to *call*, not something to react to. React's own
  // answer for that is an effect event — a function whose identity never
  // changes and whose body is always the latest — and without it `subscribe`
  // would change every render and `useSyncExternalStore` would resubscribe
  // every render.
  const fetcher = useStableCallback(query);

  const subscribe = useCallback(
    (listener: () => void) => cache.watch(stable, fetcher, { listener, staleTime }),
    [cache, stable, fetcher, staleTime],
  );

  const snapshot = useCallback(() => cache.read(stable), [cache, stable]);
  const entry: Entry<mixed> = useSyncExternalStore(subscribe, snapshot, snapshot);

  const refetch = useCallback(() => cache.fetch(stable, fetcher), [cache, stable, fetcher]);

  return useMemo(
    () => ({
      value: (entry.value: $FlowFixMe),
      error: entry.error,
      pending: entry.pending,
      loading: entry.pending && entry.updatedAt === 0,
      stale: entry.updatedAt === 0 || Date.now() - entry.updatedAt >= staleTime,
      refetch: (refetch: $FlowFixMe),
    }),
    [entry, staleTime, refetch],
  );
}

/** What a mutation looks like to a component. */
export type MutationResult<TInput, TOutput> = {|
  readonly run: (input: TInput) => Promise<TOutput>,
  readonly value: TOutput | void,
  readonly error: Error | null,
  readonly pending: boolean,
|};

/**
 * Run something that changes state, and invalidate what it affected.
 *
 * `run` is called from an event, so the closure it captures is the one from
 * the render the reader was looking at — there is no ref holding a "latest"
 * anything, and nothing to go stale. Nor is there a guard against settling
 * after unmount: React has not warned about that since 18, and adding one
 * would only hide a real leak if there were one.
 *
 * `invalidates` takes key prefixes, because that is how invalidation is
 * expressed: creating a user refreshes `["users"]` and every `["users", id]`
 * under it without the caller listing them.
 */
export function useMutation<TInput, TOutput>(
  mutation: (input: TInput) => Promise<TOutput>,
  options?: {| readonly invalidates?: $ReadOnlyArray<QueryKey> |},
): MutationResult<TInput, TOutput> {
  const cache = useQueryCache();
  const [state, setState] = useState<{|
    value: TOutput | void,
    error: Error | null,
    pending: boolean,
  |}>({ value: undefined, error: null, pending: false });

  const run = async (input: TInput): Promise<TOutput> => {
    setState((current) => ({ ...current, pending: true, error: null }));
    try {
      const value = await mutation(input);
      setState({ value, error: null, pending: false });
      // After it settles, so a watcher refetching sees the change rather than
      // racing it.
      for (const prefix of options?.invalidates ?? []) {
        cache.invalidate(prefix);
      }
      return value;
    } catch (thrown) {
      const error = thrown instanceof Error ? thrown : new Error(String(thrown));
      setState({ value: undefined, error, pending: false });
      // Rethrown: a caller awaiting `run` has to be able to tell that it
      // failed, and the failure state alone would not let it.
      throw error;
    }
  };

  return { run, value: state.value, error: state.error, pending: state.pending };
}
