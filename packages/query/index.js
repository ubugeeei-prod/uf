// @flow
//
// `@uniflowed/query`: asking for the same thing twice should cost once.
//
// Server state is not application state. It is owned somewhere else, it goes
// out of date without anybody touching it, several components want the same
// piece of it at the same moment, and every read can fail. A `useState` beside
// a `useEffect` models none of that, which is why every application that
// starts with one ends up rebuilding this package badly.
//
// ```js
// const client = new QueryClient({ queries: { staleTime: 30_000 } });
//
// component Profile(id: string) {
//   const { data, isPending, error } = useQuery({
//     queryKey: ["user", id],
//     queryFn: ({ signal }) => fetch(`/users/${id}`, { signal }).then((r) => r.json()),
//   });
//   if (isPending) return <Spinner />;
//   if (error != null) return <Failure error={error} />;
//   return <Card user={data} />;
// }
// ```
//
// A header and a sidebar rendering that make one request. Navigating away and
// back shows the profile immediately and refreshes it behind. A response that
// is byte-identical to the cached one re-renders nothing at all.
//
// # The five decisions everything else follows from
//
// **A key is a value, not a reference.** `["user", id]` written in two files
// is one entry, because keys are compared by their contents. `key.js`.
//
// **An unchanged answer is the same object.** A response is merged into the
// cached one so that every deeply-equal subtree keeps its identity. That is
// what makes "did anything change" an `Object.is`, which is what makes a poll
// over unchanging data free. `structural.js`.
//
// **One entry, one request in flight.** Two components mounting in the same
// tick join one request — not only for the saving, but because two requests
// can answer in either order and the two components would then disagree.
// `query.js`.
//
// **Stale is not the same as absent.** A cached answer is shown while it is
// refreshed, so navigating back does not flash a spinner over data that is
// already on screen. `staleTime` is the whole of the policy. `query.js`.
//
// **The snapshot is stable when nothing changed.** A component reads through
// `useSyncExternalStore`, which re-renders only when the snapshot's identity
// changes; the observer returns the *same* result object whenever every field
// in it is unchanged. `observer.js`.
//
// # How the package is laid out
//
// Eleven modules beside this one, each named after the thing it decides. Nothing
// is under an `internal/`: every one of them is a reasonable thing to import
// on purpose, and hiding them would have cost a reader a directory hop to
// reach the first line of code without making anything safer.
//
// The value-level leaves, which have no state and no React in them:
//
// - `key.js` — when two requests are the same request, and what a prefix
//   filter matches.
// - `structural.js` — when two answers are the same answer, and how to keep
//   the old references for the parts that are.
// - `retry.js` — whether to try again, how long to wait, and how to stop.
//
// The cache, which is a plain object graph that works with nothing rendering:
//
// - `query.js` — one key: its state machine, its one in-flight request, its
//   abort signal and its collection timer.
// - `cache.js` — every entry, and the filter vocabulary that says which ones
//   an operation means.
// - `mutation.js` — the write side: optimistic context, rollback, and why a
//   write is never cancelled.
// - `presence.js` — the tab came back; the network came back.
// - `client.js` — the cache with the application's defaults on it, and the
//   imperative surface: `setQueryData`, `invalidateQueries`, `prefetchQuery`,
//   `cancelQueries`.
//
// The React edge:
//
// - `observer.js` — one component's options over one entry: the narrowing, the
//   placeholder, and the reference-stable snapshot that decides whether a
//   render happens at all.
// - `infinite.js` — what a paged query changes about an ordinary one, which is
//   only how the entry is filled.
// - `react.js` — the provider and the four hooks, each a
//   `useSyncExternalStore` and almost nothing else.
//
// `observer.js` is the one to read first if something re-renders when it
// should not, and `query.js` if something fetches when it should not.
//
// # What is not here, and where it went instead
//
// `initialData` is absent: `client.setQueryData` before rendering is the same
// thing with fewer rules, and the difference between initial data and
// placeholder data is a distinction this package would rather not make people
// learn. `placeholderData` is here, because "show the previous page while the
// next one loads" has no other spelling.
//
// Suspense, offline request queues, and `client.fetchInfiniteQuery` are named
// as out of scope in the modules that would own them, with the reason in each
// case. A single request in flight for `useAsync` — one call, no cache — is
// `@uniflowed/hooks`.

export type { QueryFilters } from "./cache.js";
export type {
  FetchQueryOptions,
  MutationDefaults,
  QueryClientOptions,
  QueryDefaults,
} from "./client.js";
export type {
  InfiniteData,
  InfinitePageContext,
  InfiniteQueryOptions,
  InfiniteQueryResult,
  PageParamFn,
} from "./infinite.js";
export type { QueryKey } from "./key.js";
export type {
  MutationCallbacks,
  MutationOptions,
  MutationResult,
  MutationState,
  MutationStatus,
} from "./mutation.js";
export type { QueryOptions, QueryResult, ResolvedQueryOptions } from "./observer.js";
export type { PresenceEvent } from "./presence.js";
export type {
  FetchContext,
  FetchDirection,
  FetchStatus,
  Fetcher,
  QueryState,
  QueryStatus,
  QueryWatcher,
} from "./query.js";
export type { RetryDelay, RetryPolicy } from "./retry.js";

export { QueryCache } from "./cache.js";
export { QueryClient } from "./client.js";
export { InfiniteQueryObserver, infinitePages } from "./infinite.js";
export { hashKey, matchesKey } from "./key.js";
export { Mutation } from "./mutation.js";
export { QueryObserver } from "./observer.js";
export { Presence } from "./presence.js";
export { DEFAULT_GC_TIME, Query } from "./query.js";
export {
  QueryClientProvider,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "./react.js";
export { CancelledError, backoffDelay } from "./retry.js";
export { structuralShare } from "./structural.js";
