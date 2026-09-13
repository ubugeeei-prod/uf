// @flow
//
// Module-free summaries of resolved routes.

import { routeBoundaries, suspenseId } from "./boundary-data.js";
import type { RouteBoundary } from "./boundary-data.js";
import type { RouteParams, SearchParams } from "./routing.js";

type RouteStatus = 200 | 401 | 403 | 404 | 500;

type RouteMetadata = { readonly [string]: mixed, ... };

type RouteErrorLike = {
  readonly kind: "thrown" | "unauthorized" | "forbidden",
  ...
};

type BoundaryLike = {
  readonly above: number,
  ...
};

type ModuleBoundaryLike = {
  readonly above: number,
  readonly module?: mixed,
  ...
};

type SlotLike = {
  readonly name: string,
  readonly above: number,
  readonly page: mixed,
  readonly params: RouteParams,
  readonly layouts: $ReadOnlyArray<mixed>,
  readonly loading: $ReadOnlyArray<BoundaryLike>,
  readonly templates: $ReadOnlyArray<BoundaryLike>,
  readonly errorBoundary: ?ModuleBoundaryLike,
  readonly slots: $ReadOnlyArray<SlotLike>,
  ...
};

type RouteLike = {
  readonly pathname: string,
  readonly search: string,
  readonly path: string,
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly layouts: $ReadOnlyArray<mixed>,
  readonly data: mixed,
  readonly deferred: mixed,
  readonly metadata: RouteMetadata,
  readonly viewTransition: ?string,
  readonly status: RouteStatus,
  readonly error: ?RouteErrorLike,
  readonly errorBoundary: ModuleBoundaryLike,
  readonly loading: $ReadOnlyArray<BoundaryLike>,
  readonly templates: $ReadOnlyArray<BoundaryLike>,
  readonly slots: $ReadOnlyArray<SlotLike>,
  ...
};

export type ResolvedBoundarySummary = {|
  readonly id: string,
  readonly above: number,
|};

export type ResolvedTemplateSummary = {|
  readonly above: number,
|};

export type ResolvedRouteErrorSummary =
  | {| readonly kind: "thrown" |}
  | {| readonly kind: "unauthorized" |}
  | {| readonly kind: "forbidden" |};

export type ResolvedErrorBoundarySummary = {|
  readonly above: number,
  readonly custom: boolean,
  readonly rendered: boolean,
|};

export type ResolvedSlotSummary = {|
  readonly name: string,
  readonly above: number,
  readonly active: boolean,
  readonly params: RouteParams,
  readonly layoutCount: number,
  readonly loading: $ReadOnlyArray<ResolvedBoundarySummary>,
  readonly templates: $ReadOnlyArray<ResolvedTemplateSummary>,
  readonly errorBoundary: ?ResolvedErrorBoundarySummary,
  readonly slots: $ReadOnlyArray<ResolvedSlotSummary>,
|};

export type ResolvedRouteSummary = {|
  readonly pathname: string,
  readonly search: string,
  readonly path: string,
  readonly params: RouteParams,
  readonly searchParams: SearchParams,
  readonly layoutCount: number,
  readonly data: mixed,
  readonly deferred: boolean,
  readonly metadata: RouteMetadata,
  readonly viewTransition: ?string,
  readonly status: RouteStatus,
  readonly error: ?ResolvedRouteErrorSummary,
  readonly errorBoundary: ResolvedErrorBoundarySummary,
  readonly loading: $ReadOnlyArray<ResolvedBoundarySummary>,
  readonly templates: $ReadOnlyArray<ResolvedTemplateSummary>,
  readonly slots: $ReadOnlyArray<ResolvedSlotSummary>,
  readonly boundaries: $ReadOnlyArray<RouteBoundary>,
|};

/**
 * The part of a `ResolvedRoute` that can cross to a payload or diagnostic.
 *
 * A resolved route carries loaded modules so `RouteView` can render today.
 * The element payload in ubugeeei-prod/uf#519 needs the other half: route
 * identity, data, metadata, status, and boundary names, with page/layout/
 * fallback modules left behind in the renderer graph.
 */
export function summarizeResolvedRoute(
  route: RouteLike,
  errorSource?: ?string,
): ResolvedRouteSummary {
  return {
    pathname: route.pathname,
    search: route.search,
    path: route.path,
    params: route.params,
    searchParams: route.searchParams,
    layoutCount: route.layouts.length,
    data: route.data,
    deferred: route.deferred != null,
    metadata: route.metadata,
    viewTransition: route.viewTransition,
    status: route.status,
    error: summarizeError(route.error),
    errorBoundary: summarizeErrorBoundary(route.errorBoundary, route.error == null),
    loading: summarizeLoading(route.loading),
    templates: summarizeTemplates(route.templates),
    slots: route.slots.map(summarizeSlot),
    boundaries: [...routeBoundaries(route, errorSource).values()],
  };
}

function summarizeSlot(slot: SlotLike): ResolvedSlotSummary {
  return {
    name: slot.name,
    above: slot.above,
    active: slot.page != null,
    params: slot.params,
    layoutCount: slot.layouts.length,
    loading: summarizeLoading(slot.loading),
    templates: summarizeTemplates(slot.templates),
    errorBoundary:
      slot.errorBoundary == null ? null : summarizeErrorBoundary(slot.errorBoundary, true),
    slots: slot.slots.map(summarizeSlot),
  };
}

function summarizeLoading(
  boundaries: $ReadOnlyArray<BoundaryLike>,
): $ReadOnlyArray<ResolvedBoundarySummary> {
  return boundaries.map((boundary, index) => ({
    id: suspenseId(index),
    above: boundary.above,
  }));
}

function summarizeTemplates(
  templates: $ReadOnlyArray<BoundaryLike>,
): $ReadOnlyArray<ResolvedTemplateSummary> {
  return templates.map((template) => ({ above: template.above }));
}

function summarizeErrorBoundary(
  boundary: ModuleBoundaryLike,
  rendered: boolean,
): ResolvedErrorBoundarySummary {
  return {
    above: boundary.above,
    custom: boundary.module != null,
    rendered,
  };
}

function summarizeError(error: ?RouteErrorLike): ?ResolvedRouteErrorSummary {
  if (error == null) {
    return null;
  }
  if (error.kind === "unauthorized") {
    return { kind: "unauthorized" };
  }
  if (error.kind === "forbidden") {
    return { kind: "forbidden" };
  }
  return { kind: "thrown" };
}
