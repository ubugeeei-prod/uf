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
  SlotRecord,
  SlotRouteRecord,
  TemplateRecord,
} from "./internal/routing.js";

export {
  ForbiddenError,
  NotFoundError,
  RedirectError,
  UnauthorizedError,
  buildRoute,
  forbidden,
  hasClientPage,
  matchRoute,
  notFound,
  parseSearch,
  permanentRedirect,
  redirect,
  routeErrorStatus,
  splitUrl,
  unauthorized,
} from "./internal/routing.js";
