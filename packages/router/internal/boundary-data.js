// @flow
//
// React-free names for the boundaries a resolved route renders.

/**
 * What `file` says for the error boundary the build synthesises.
 *
 * `routesModuleSource` writes this string where a declared boundary has a path,
 * because the record has no module to name: the framework's own error page
 * renders in its place. This module is plain data so the routing surface and
 * the dev-only DOM marker code can share one spelling without importing React.
 */
export const SYNTHESISED_SOURCE: string = "@uniflowed/router";

/** Which kind of boundary a mark or payload row belongs to. */
export type BoundaryKind = "suspense" | "error";

/** The id of the `<Suspense>` boundary at `index` of a route's `loading`. */
export function suspenseId(index: number): string {
  return `suspense:${index}`;
}

/** The id of the boundary a route's own `$error.js` renders. */
export const ROUTE_ERROR_ID: string = "error:route";

/**
 * The id of the boundary that stands outside every layout.
 *
 * It has no module and renders the framework's page; it is what is between a
 * throw in a root layout, or in the error component itself, and an unmounted
 * document.
 */
export const ROOT_ERROR_ID: string = "error:root";

/**
 * One boundary of a resolved route, in uf's vocabulary rather than React's.
 *
 * `id` is the stable name `RouteView` marks in the DOM today and the name a
 * future element payload can put beside the chunk that completed it. `above`
 * is how many layouts are outside the boundary.
 */
export type RouteBoundary = {|
  readonly id: string,
  readonly kind: BoundaryKind,
  readonly above: number,
  readonly source: ?string,
|};

/** The part of a resolved route the boundary map reads. */
type BoundedRoute = {
  readonly errorBoundary: { readonly above: number, ... },
  readonly loading: $ReadOnlyArray<{ readonly above: number, ... }>,
  readonly error: mixed,
  ...
};

/**
 * Every boundary a resolved route renders, in the order they nest.
 *
 * A `Map` rather than a list because callers read it both ways: `RouteView`
 * asks for one by id as it builds the stack, while diagnostics and payload
 * summaries walk the values.
 */
export function routeBoundaries(
  resolved: BoundedRoute,
  errorSource: ?string,
): Map<string, RouteBoundary> {
  const found: Map<string, RouteBoundary> = new Map();
  found.set(ROOT_ERROR_ID, {
    id: ROOT_ERROR_ID,
    kind: "error",
    above: 0,
    source: SYNTHESISED_SOURCE,
  });
  if (resolved.error == null) {
    found.set(ROUTE_ERROR_ID, {
      id: ROUTE_ERROR_ID,
      kind: "error",
      above: resolved.errorBoundary.above,
      source: errorSource,
    });
  }
  resolved.loading.forEach((boundary, index) => {
    const id = suspenseId(index);
    found.set(id, { id, kind: "suspense", above: boundary.above, source: null });
  });
  return found;
}
