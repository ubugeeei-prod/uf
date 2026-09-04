// @flow
//
// `@uniflowed/query`: asking for the same thing twice should cost once.
//
// Three behaviours an application writes by hand around `fetch` and gets
// wrong, and which are the reason a query library exists at all:
//
//   * **De-duplication.** A header and a sidebar both showing the current user
//     make one request, not two, and cannot disagree about the answer.
//   * **Stale-while-revalidate.** A cached value is shown at once and
//     refreshed behind it, so navigating back does not flash a spinner over
//     data that is already there.
//   * **Invalidation by prefix.** After creating a user, `["users"]`
//     invalidates `["users", 1]` and `["users", 2]` without listing them.
//
// The cache is a plain object graph with no React in it, and `useQuery` is a
// `useSyncExternalStore` over it — so a value is correct during a prerender,
// two components reading one key cannot render different versions of it, and
// the cache is testable without rendering anything.

export type { Entry, QueryKey } from "./internal/cache.js";
export type { MutationResult, QueryOptions, QueryResult } from "./internal/react.js";

export { QueryCache, hash } from "./internal/cache.js";
export { QueryProvider, useMutation, useQuery, useQueryCache } from "./internal/react.js";
