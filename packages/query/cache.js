// @flow
//
// `@uniflowed/query/cache`: every entry, and how to say which ones you mean.
//
// The cache is a `Map` from a key's hash to its [`Query`], plus the vocabulary
// for describing a *set* of entries. Those are two different jobs and this
// module owns the second: a query knows what happened to one key, and the
// cache knows how "everything under `["users"]` that somebody is looking at"
// turns into a list of them.
//
// # Why filters are a shape rather than a list of methods
//
// Invalidation, cancellation, refetching and removal all need the same
// question answered — which entries does this describe? — and they need it
// answered identically, or `invalidateQueries` and `cancelQueries` given the
// same argument would act on different entries. So there is one
// [`QueryFilters`] shape and one [`findAll`], and every operation on the
// client is that plus a verb.
//
// `type: "active"` is the one that carries weight. Invalidation has to refetch
// what is on screen and merely mark the rest, because refetching an entry
// nobody is watching is a request whose answer will be garbage-collected
// before it is read. Without the distinction, invalidating `["users"]` in an
// application with a hundred cached users makes a hundred requests.
//
// # Why the cache is explicit rather than a module-level default
//
// A singleton is shared with every other test in the process, and one test's
// cached answer then decides another test's result — a class of flake that
// costs hours to attribute because the failing test is not the one at fault.
// It is also wrong in production: a server rendering two requests must not let
// one reader's data reach the other. One cache per [`QueryClient`], one client
// per tree, one tree per request.

import { hashKey, matchesKey } from "./key.js";
import type { QueryKey } from "./key.js";
import { DEFAULT_GC_TIME, Query } from "./query.js";

/**
 * Which entries an operation applies to.
 *
 * Everything is optional and everything narrows: no filter at all means every
 * entry, which is what `invalidateQueries()` with no argument means and why
 * that call is worth writing.
 */
export type QueryFilters = {|
  /** A key prefix, or an exact key with `exact`. */
  readonly queryKey?: QueryKey,
  readonly exact?: boolean,
  /** `active` is "somebody is watching"; see the module docs. */
  readonly type?: "all" | "active" | "inactive",
  readonly predicate?: (query: Query<mixed>) => boolean,
|};

export class QueryCache {
  readonly queries: Map<string, Query<mixed>> = new Map();

  /** The entry for `hash`, if there is one. Never creates. */
  get(hash: string): Query<mixed> | void {
    return this.queries.get(hash);
  }

  /**
   * The entry for `key`, creating it if it is not there.
   *
   * Creating is a mutation, which is why nothing on the render path calls
   * this: a component reads with [`get`] and returns "nothing yet" when the
   * answer is nothing yet. The entry is built in the effect that subscribes,
   * where a mutation is allowed to happen and where React can undo it.
   */
  build(key: QueryKey, gcTime: number = DEFAULT_GC_TIME): Query<mixed> {
    const hash = hashKey(key);
    const existing = this.queries.get(hash);
    if (existing != null) {
      return existing;
    }
    const query = new Query<mixed>(this, key, gcTime);
    this.queries.set(hash, query);
    return query;
  }

  /**
   * Drop `query`, if it is still the entry under its key.
   *
   * The identity check matters: garbage collection is scheduled on a timer,
   * and by the time it fires the entry may already have been removed and
   * rebuilt by a remount. Deleting by hash alone would then collect the *new*
   * entry — with its observers, its data and its request — because an old
   * timer said so.
   */
  remove(query: Query<mixed>): void {
    if (this.queries.get(query.hash) === query) {
      this.queries.delete(query.hash);
    }
    query.destroy();
  }

  /** Every entry the filter describes, in insertion order. */
  findAll(filters?: QueryFilters): Array<Query<mixed>> {
    const all = Array.from(this.queries.values());
    if (filters == null) {
      return all;
    }
    return all.filter((query) => matches(query, filters));
  }

  /** The first entry the filter describes. */
  find(filters: QueryFilters): Query<mixed> | void {
    return this.findAll(filters)[0];
  }

  /** Forget everything, cancelling anything in flight. */
  clear(): void {
    for (const query of Array.from(this.queries.values())) {
      this.remove(query);
    }
    this.queries.clear();
  }
}

function matches(query: Query<mixed>, filters: QueryFilters): boolean {
  if (
    filters.queryKey != null &&
    !matchesKey(query.key, filters.queryKey, filters.exact === true)
  ) {
    return false;
  }
  const type = filters.type ?? "all";
  if (type === "active" && !query.isActive()) {
    return false;
  }
  if (type === "inactive" && query.isActive()) {
    return false;
  }
  return filters.predicate == null || filters.predicate(query);
}
