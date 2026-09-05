// @flow
//
// `@uniflowed/query/react`: the binding, and why it is this thin.
//
// Every hook here is a `useSyncExternalStore` over a store that already knows
// how to keep itself correct, and almost nothing else. There is no effect that
// starts a request, no ref holding the latest query function, and no
// dependency array deciding when to refetch. Subscribing *is* the statement
// that a value is wanted, so the store starts the request; the component only
// reads.
//
// That is not a stylistic preference. Reading an external store through the
// API React provides for it is what makes the value React committed with the
// value the component rendered — which is what stops two components sharing a
// key from painting different versions of it during a concurrent render — and
// it is what lets a prerender state the server's answer rather than fall
// through to it.
//
// # What runs during render, and what does not
//
// During render: reading the cache, applying `select`, and comparing the
// result with the previous one. All of it is pure with respect to anything
// outside this hook — the entry is not created, no request is started, no
// listener is registered, and running it twice produces the same object. The
// observer's memo slots are written, and that is a cache in the `useMemo`
// sense; the module docs in `observer.js` set out why that is the only way
// `getSnapshot` can meet its contract.
//
// Outside render, in the effect React calls `subscribe` from: building the
// entry, registering the listener, starting the fetch, and setting the stale
// and interval timers. Every one is undone by the function `subscribe`
// returns, so a component that mounts and unmounts leaves nothing behind.
//
// # Why the query function is not a dependency
//
// Callers write `queryFn` inline, so its identity changes on every render, and
// a subscription keyed on it would be torn down and rebuilt on every
// keystroke. But it is something to *call*, not something to react to: it is
// read through a callback whose identity never changes and whose body is
// always the latest, so a fresh closure changes nothing and the request that
// is eventually made is the current one.
//
// What the subscription *is* keyed on is the plan: the key's hash, whether it
// is enabled, and the times and intervals. Those are the values that change
// what is being watched, and there are few enough of them to write down —
// which is better than an exhaustive dependency array that resubscribes for
// reasons nobody can name.
//
// # Why there is no `Suspense` integration here
//
// `useQuery` returning a result rather than suspending is a decision, not an
// omission. Suspending on read makes waterfalls the default — each component
// suspends in turn, and each one's request starts only after the one above it
// resolves — and the fix is to hoist fetching to the route, which is
// `client.prefetchQuery` and `@uniflowed/router`'s loaders. A `useSuspenseQuery`
// belongs with that work, where the prefetch can be arranged, not here where
// it would quietly make every list slower.

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

import type { QueryClient } from "./client.js";
import { InfiniteQueryObserver } from "./infinite.js";
import type { InfiniteData, InfiniteQueryOptions, InfiniteQueryResult } from "./infinite.js";
import { hashKey } from "./key.js";
import { Mutation } from "./mutation.js";
import type {
  MutationCallbacks,
  MutationOptions,
  MutationResult,
  MutationState,
} from "./mutation.js";
import { QueryObserver } from "./observer.js";
import type { QueryOptions, QueryResult, ResolvedQueryOptions } from "./observer.js";

const ClientContext: React.Context<QueryClient | null> = createContext(null);

/**
 * Make a client available to the tree.
 *
 * Required rather than falling back to a module-level default. A default is
 * shared with every other tree in the process, which on a server means one
 * reader's data is in the cache the next request renders from, and in a test
 * means the previous test's answer decides this one's result.
 */
export component QueryClientProvider(client: QueryClient, children: React.Node) {
  return <ClientContext.Provider value={client}>{children}</ClientContext.Provider>;
}

/** The client this subtree uses. */
export function useQueryClient(): QueryClient {
  const client = useContext(ClientContext);
  if (client == null) {
    throw new Error(
      "useQuery needs a QueryClientProvider above it; render <QueryClientProvider client={new QueryClient()}>",
    );
  }
  return client;
}

/**
 * Read a key, fetching it when it is missing or stale.
 *
 * A cached answer is returned immediately and refreshed behind it, so
 * navigating back to a page shows it at once. `isFetching` says a request is
 * in flight; `isPending` says there is nothing to show yet — conflating those
 * is why applications flash a spinner over data they already have.
 */
export function useQuery<TData, TSelected = TData>(
  options: QueryOptions<TData, TSelected>,
): QueryResult<TSelected> {
  const client = useQueryClient();
  const getOptions = useStableCallback(() => options);

  // Created once and never during a commit: the constructor touches nothing
  // outside the object, so React discarding one of a double-invoked render's
  // two observers costs an allocation and changes no behaviour.
  const [observer] = useState(() => new QueryObserver<TData, TSelected>(client, getOptions));

  // Once per render, and used for both what is watched and what is read, so
  // the two cannot disagree about a default.
  const resolved = client.resolveQuery(options);
  const plan = subscriptionPlan(resolved);
  const subscribe = useCallback(
    (listener: () => void) => observer.subscribe(client, listener),
    [observer, client, plan],
  );

  // Deliberately a fresh closure: it must narrow with *this* render's `select`
  // and read *this* render's client. Its identity is free to change — React
  // calls the latest one — while the value it returns must not, which is the
  // observer's job.
  const read = () => observer.readResult(client, resolved);
  return useSyncExternalStore(subscribe, read, read);
}

/**
 * Read a key that arrives a page at a time.
 *
 * One cache entry holding every page, so the list is invalidated, refetched
 * and collected as the single thing the reader sees. See `infinite.js` for why
 * the alternative — a query per page — cannot stay coherent.
 */
export function useInfiniteQuery<TPage, TParam, TSelected = InfiniteData<TPage, TParam>>(
  options: InfiniteQueryOptions<TPage, TParam, TSelected>,
): InfiniteQueryResult<TSelected> {
  const client = useQueryClient();
  const getOptions = useStableCallback(() => options);
  const [observer] = useState(
    () => new InfiniteQueryObserver<TPage, TParam, TSelected>(client, getOptions),
  );

  const resolved = client.resolveQuery(options as $FlowFixMe);
  const plan = subscriptionPlan(resolved);
  const subscribe = useCallback(
    (listener: () => void) => observer.subscribe(client, listener),
    [observer, client, plan],
  );

  // The paged observer's snapshot is the ordinary one with the page controls
  // added; `readResult` is inherited, so the widening is stated here.
  const read = () => observer.readResult(client, resolved) as InfiniteQueryResult<TSelected>;
  return useSyncExternalStore(subscribe, read, read);
}

/**
 * Run something that changes state, with the callbacks a rollback needs.
 *
 * `mutate` is called from an event, so the closure it captures is the one from
 * the render the reader was looking at, and there is nothing to go stale.
 * `mutateAsync` is the same call for a caller who needs to await it — the
 * difference is only whether a failure arrives as a rejected promise or as
 * `error` on the next render.
 */
export function useMutation<TVariables, TData, TContext = mixed>(
  options: MutationOptions<TVariables, TData, TContext>,
): MutationResult<TVariables, TData, TContext> {
  const client = useQueryClient();
  const getOptions = useStableCallback(() => options);
  const [mutation] = useState(() => new Mutation<TVariables, TData, TContext>());

  const subscribe = useCallback((listener: () => void) => mutation.subscribe(listener), [mutation]);
  const read = () => mutation.state;
  const state: MutationState<TVariables, TData> = useSyncExternalStore(subscribe, read, read);

  const mutateAsync = useStableCallback(
    (variables: TVariables, callbacks?: MutationCallbacks<TVariables, TData, TContext>) =>
      mutation.execute(variables, client.resolveMutation(getOptions()), callbacks),
  );
  const mutate = useStableCallback(
    (variables: TVariables, callbacks?: MutationCallbacks<TVariables, TData, TContext>) => {
      // Swallowed on purpose: the failure is on the next render as `error`,
      // and an uncaught rejection for a state the UI is already showing is
      // noise in the console and nothing else.
      void mutateAsync(variables, callbacks).catch(ignore);
    },
  );
  const reset = useStableCallback(() => mutation.reset());

  return useMemo(
    () => ({
      data: state.data,
      error: state.error,
      status: state.status,
      variables: state.variables,
      failureCount: state.failureCount,
      isIdle: state.status === "idle",
      isPending: state.status === "pending",
      isSuccess: state.status === "success",
      isError: state.status === "error",
      mutate,
      mutateAsync,
      reset,
    }),
    [state, mutate, mutateAsync, reset],
  );
}

/**
 * What makes one subscription different from another.
 *
 * A string rather than a dependency array so the reason is readable at the
 * point of failure: these are the values that change *what is being watched*.
 * `queryFn`, `select` and `placeholderData` are deliberately absent — they
 * change what happens when a value arrives, not whether to watch for one, and
 * including them would resubscribe on every render for no effect.
 */
function subscriptionPlan<TData, TSelected>(
  options: ResolvedQueryOptions<TData, TSelected>,
): string {
  return [
    hashKey(options.queryKey),
    String(options.enabled),
    String(options.staleTime),
    String(options.gcTime),
    String(options.refetchInterval),
    String(options.refetchOnWindowFocus),
    String(options.refetchOnReconnect),
  ].join("|");
}

function ignore(): void {}
