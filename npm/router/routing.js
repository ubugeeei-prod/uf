// @flow
//
// `@uniflowed/router/routing`: React-free route table helpers.
//
// Import this from server-oriented modules that need matching, URL building,
// or router control errors without importing the client router, React
// components, or hooks.

export type {
  ErrorBoundary,
  LoadingRecord,
  NotFoundBoundary,
  RouteError,
  RouteMatch,
  RouteModule,
  RouteParamSpec,
  RouteParams,
  RouteRecord,
  RouteTable,
  SearchParams,
  SearchParamsAll,
  SearchParamsIssue,
  SlotRecord,
  SlotRouteRecord,
  TemplateRecord,
} from "./internal/routing.js";
export type {
  ResolvedBoundarySummary,
  ResolvedErrorBoundarySummary,
  ResolvedRouteErrorSummary,
  ResolvedRouteSummary,
  ResolvedSlotSummary,
  ResolvedTemplateSummary,
} from "./internal/resolved-summary.js";

export {
  ForbiddenError,
  NotFoundError,
  RedirectError,
  SearchParamsError,
  UnauthorizedError,
  buildRoute,
  forbidden,
  hasClientPage,
  matchRoute,
  notFound,
  parseSearch,
  parseSearchAll,
  permanentRedirect,
  redirect,
  routeErrorStatus,
  splitUrl,
  unauthorized,
} from "./internal/routing.js";
export { summarizeResolvedRoute } from "./internal/resolved-summary.js";
