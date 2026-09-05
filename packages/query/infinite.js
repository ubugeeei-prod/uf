// @flow
//
// `@uniflowed/query/infinite`: many pages, one entry.
//
// "Load more" is not a sequence of queries. If page two were its own cache
// entry, then the list on screen would be the concatenation of three entries
// that can be invalidated, refetched and garbage-collected independently — and
// the first time one of them refreshed on its own the reader would see a list
// with a hole in it, or the same row twice.
//
// So a paged query is *one* entry whose value happens to be a list of pages:
//
// ```js
// { pages: [[…], […]], pageParams: [undefined, "cursor-2"] }
// ```
//
// One key, one staleness clock, one invalidation, one garbage collection, and
// one thing to render. `pageParams` is kept beside the pages because a refetch
// has to be able to ask for the same pages again, and a cursor is not
// recoverable from the rows it returned.
//
// # Why this is a fetcher and not a second cache
//
// Everything else about a paged query — de-duplication, retries, cancellation,
// staleness, structural sharing — is identical to an ordinary one. The only
// difference is what "fetch this entry" means, so that is the only thing this
// module replaces: [`infinitePages`] builds the function [`Query`] calls, and
// [`InfiniteQueryObserver`] is the ordinary observer with the page controls
// added to its snapshot.
//
// Structural sharing is why appending a page is cheap: the new value is a new
// array with the *same* page objects in it, so the pages already rendered keep
// their identity and a memoised row component does not re-render because its
// neighbour arrived.
//
// # Why a refetch re-asks for every page
//
// The alternative is to refetch page one and keep the rest, which produces a
// list that is coherent nowhere: the first page is from now and the second is
// from ten minutes ago, and an item that moved between them appears twice or
// not at all. So a refetch walks the recorded `pageParams` in order and asks
// again for each. It is more expensive, and it is the only version that is
// correct.
//
// The recorded parameters are reused rather than recomputed from
// `getNextPageParam`, so a refetch asks for the pages the reader is looking
// at. A cursor that has since expired is the server's to reject, and the
// failure is reported rather than hidden behind a silently different list.
//
// # What is out of scope
//
// `client.fetchInfiniteQuery` — prefetching a paged query from outside React —
// is not implemented. It needs the same page walk with a different entry
// point, and until there is a server-rendering path that wants it, it would be
// an untested API. `useInfiniteQuery` inside a tree covers what applications
// do today.

import type { QueryKey } from "./key.js";
import { QueryObserver } from "./observer.js";
import type { QueryResult, ResolvedQueryOptions } from "./observer.js";
import type { FetchContext, FetchDirection, Fetcher, Query, QueryState } from "./query.js";

/** The value a paged entry holds. */
export type InfiniteData<TPage, TParam> = {|
  readonly pages: $ReadOnlyArray<TPage>,
  readonly pageParams: $ReadOnlyArray<TParam>,
|};

/** What the query function is called with, once per page. */
export type InfinitePageContext<TParam> = {|
  readonly queryKey: QueryKey,
  readonly signal: AbortSignal,
  readonly pageParam: TParam,
  readonly direction: FetchDirection,
|};

/**
 * Where the next page starts, or `null` when there is no next page.
 *
 * Returning `null` is how the list ends, and it is why `hasNextPage` can be
 * answered without an extra request: the server already said so in the answer
 * it gave for the last page.
 */
export type PageParamFn<TPage, TParam> = (
  lastPage: TPage,
  allPages: $ReadOnlyArray<TPage>,
  lastPageParam: TParam,
  allPageParams: $ReadOnlyArray<TParam>,
) => TParam | null | void;

export type InfiniteQueryOptions<TPage, TParam, TSelected = InfiniteData<TPage, TParam>> = {|
  readonly queryKey: QueryKey,
  readonly queryFn: (context: InfinitePageContext<TParam>) => Promise<TPage>,
  /** The parameter the first page is asked for with. */
  readonly initialPageParam: TParam,
  readonly getNextPageParam: PageParamFn<TPage, TParam>,
  readonly getPreviousPageParam?: PageParamFn<TPage, TParam>,
  /** Keep at most this many pages, dropping from the far end. */
  readonly maxPages?: number,
  readonly enabled?: boolean,
  readonly staleTime?: number,
  readonly gcTime?: number,
  readonly retry?: mixed,
  readonly retryDelay?: mixed,
  readonly select?: (data: InfiniteData<TPage, TParam>) => TSelected,
  readonly placeholderData?: mixed,
  readonly refetchInterval?: number | null,
  readonly refetchOnWindowFocus?: boolean,
  readonly refetchOnReconnect?: boolean,
|};

/** An ordinary result, plus the two ends of the list. */
export type InfiniteQueryResult<TSelected> = {|
  ...QueryResult<TSelected>,
  readonly hasNextPage: boolean,
  readonly hasPreviousPage: boolean,
  readonly isFetchingNextPage: boolean,
  readonly isFetchingPreviousPage: boolean,
  readonly fetchNextPage: () => Promise<void>,
  readonly fetchPreviousPage: () => Promise<void>,
|};

/**
 * The function the cache calls to fill a paged entry.
 *
 * `direction` decides which of the three things it means: extend the list
 * forward, extend it backward, or — when there is no direction — refetch every
 * page the entry already holds. A first fetch has nothing to extend, so it
 * asks for `initialPageParam` whatever the direction says.
 */
export function infinitePages<TPage, TParam>(
  options: InfiniteQueryOptions<TPage, TParam, mixed>,
  direction: FetchDirection | null,
): Fetcher<InfiniteData<TPage, TParam>> {
  return async (context: FetchContext<InfiniteData<TPage, TParam>>) => {
    const previous = context.previousData;
    const askFor = (pageParam: TParam, towards: FetchDirection): Promise<TPage> =>
      options.queryFn({
        queryKey: context.queryKey,
        pageParam,
        direction: towards,
        // Delegated rather than read, so a paged query function that ignores
        // the signal leaves the entry uncancellable in exactly the same way an
        // ordinary one does. Reading it here instead would opt every paged
        // query in on its behalf. See `query.js` for what the getter means.
        // uf-lint-disable-next-line flow/unsafe-getters-setters
        get signal(): AbortSignal {
          return context.signal;
        },
      });

    if (previous == null || previous.pages.length === 0) {
      const page = await askFor(options.initialPageParam, "forward");
      return { pages: [page], pageParams: [options.initialPageParam] };
    }

    if (direction === "forward") {
      const param = nextParam(options, previous);
      if (param == null) {
        return previous;
      }
      const page = await askFor(param, "forward");
      return trim(
        { pages: [...previous.pages, page], pageParams: [...previous.pageParams, param] },
        options.maxPages,
        "forward",
      );
    }

    if (direction === "backward") {
      const param = previousParam(options, previous);
      if (param == null) {
        return previous;
      }
      const page = await askFor(param, "backward");
      return trim(
        { pages: [page, ...previous.pages], pageParams: [param, ...previous.pageParams] },
        options.maxPages,
        "backward",
      );
    }

    // No direction: refresh what is on screen, page by page, in order.
    const pages: Array<TPage> = [];
    for (const pageParam of previous.pageParams) {
      pages.push(await askFor(pageParam, "forward"));
    }
    return { pages, pageParams: previous.pageParams.slice() };
  };
}

/** Whether the server said there is another page after the ones held. */
export function hasMore<TPage, TParam>(
  options: InfiniteQueryOptions<TPage, TParam, mixed>,
  data: InfiniteData<TPage, TParam> | void,
  towards: FetchDirection,
): boolean {
  if (data == null || data.pages.length === 0) {
    return false;
  }
  return (towards === "forward" ? nextParam(options, data) : previousParam(options, data)) != null;
}

/**
 * The observer for a paged query.
 *
 * Everything about watching, fetching, retrying and snapshotting is inherited;
 * the two differences are that the entry is filled a page at a time and that
 * the snapshot carries the controls for the two ends of the list.
 */
export class InfiniteQueryObserver<TPage, TParam, TSelected> extends QueryObserver<
  InfiniteData<TPage, TParam>,
  TSelected,
> {
  readonly fetchNextPage: () => Promise<void>;
  readonly fetchPreviousPage: () => Promise<void>;

  constructor(
    client: $FlowFixMe,
    getOptions: () => InfiniteQueryOptions<TPage, TParam, TSelected>,
  ) {
    super(client, getOptions as $FlowFixMe);
    // Stable for the observer's life: a snapshot that carried a new function
    // every render would never compare equal to the one before it, and every
    // notification would become a re-render.
    // `cancelRefetch: true`, because a page control is a request for something
    // the entry does not have yet. Joining whatever is in flight would return
    // the refetch's answer and add no page at all — the directional fetcher
    // would never run — so pressing "load more" while a background refetch was
    // going did nothing, silently.
    this.fetchNextPage = () => this.fetch({ cancelRefetch: true, direction: "forward" });
    this.fetchPreviousPage = () => this.fetch({ cancelRefetch: true, direction: "backward" });
  }

  pageOptions(): InfiniteQueryOptions<TPage, TParam, mixed> {
    return this.getOptions() as $FlowFixMe;
  }

  buildFetcher(
    _options: ResolvedQueryOptions<InfiniteData<TPage, TParam>, TSelected>,
    direction: FetchDirection | null,
  ): Fetcher<InfiniteData<TPage, TParam>> {
    return infinitePages(this.pageOptions(), direction);
  }

  // $FlowFixMe[incompatible-extend] the paged snapshot is the base snapshot plus
  // the page controls. Flow has no way to state "the same exact object with
  // more fields" for an override, and widening the base result to an inexact
  // type to allow it would stop catching typos in every ordinary query.
  buildResult(
    query: Query<InfiniteData<TPage, TParam>> | void,
    state: QueryState<InfiniteData<TPage, TParam>>,
    options: ResolvedQueryOptions<InfiniteData<TPage, TParam>, TSelected>,
  ): InfiniteQueryResult<TSelected> {
    const base = super.buildResult(query, state, options);
    const pages = this.pageOptions();
    // Asked of the raw pages rather than of `base.data`, which may have been
    // narrowed by `select` into something with no pages in it at all.
    const data = state.data;
    const isFetching = state.fetchStatus === "fetching";
    return {
      ...base,
      hasNextPage: hasMore(pages, data, "forward"),
      hasPreviousPage: hasMore(pages, data, "backward"),
      isFetchingNextPage: isFetching && state.direction === "forward",
      isFetchingPreviousPage: isFetching && state.direction === "backward",
      fetchNextPage: this.fetchNextPage,
      fetchPreviousPage: this.fetchPreviousPage,
    };
  }
}

function nextParam<TPage, TParam>(
  options: InfiniteQueryOptions<TPage, TParam, mixed>,
  data: InfiniteData<TPage, TParam>,
): TParam | null | void {
  const index = data.pages.length - 1;
  return options.getNextPageParam(
    data.pages[index],
    data.pages,
    data.pageParams[index],
    data.pageParams,
  );
}

function previousParam<TPage, TParam>(
  options: InfiniteQueryOptions<TPage, TParam, mixed>,
  data: InfiniteData<TPage, TParam>,
): TParam | null | void {
  const get = options.getPreviousPageParam;
  if (get == null) {
    return null;
  }
  return get(data.pages[0], data.pages, data.pageParams[0], data.pageParams);
}

/** Keep at most `maxPages`, dropping from the end the reader is moving away from. */
function trim<TPage, TParam>(
  data: InfiniteData<TPage, TParam>,
  maxPages: number | void,
  direction: FetchDirection,
): InfiniteData<TPage, TParam> {
  if (maxPages == null || maxPages <= 0 || data.pages.length <= maxPages) {
    return data;
  }
  const from = direction === "forward" ? data.pages.length - maxPages : 0;
  return {
    pages: data.pages.slice(from, from + maxPages),
    pageParams: data.pageParams.slice(from, from + maxPages),
  };
}
