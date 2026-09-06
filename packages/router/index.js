// @flow
//
// `@uniflowed/router`: the file-system router.
//
// Pages live in `app/` as `_uf.page.js` (or `.mdx`), layouts as
// `_uf.layout.js`, and `app.js` exports `routerView("./app")`. The route table
// is generated from the directory at build time; this module is the runtime
// that matches, loads, navigates and renders it.
//
// `_uf.not-found.js` and `_uf.error.js` are the two boundaries: the page for a
// path that matched nothing, and what renders in place of a subtree that threw.
// Both are segment files, resolved by the nearest one above the path.

import * as React from "react";

import type { RouteError } from "./internal/runtime.js";

export type {
  AppProps,
  ErrorBoundary,
  ErrorModule,
  LayoutModule,
  LinkPrefetch,
  LoaderArgs,
  Metadata,
  MetadataArgs,
  NavigateOptions,
  NotFoundBoundary,
  PageModule,
  ResolvedRoute,
  RouteError,
  RouteInfo,
  RouteMatch,
  RouteParamSpec,
  RouteParams,
  RouteRecord,
  RouteTable,
  Router,
  SearchParams,
} from "./internal/runtime.js";

export {
  ForbiddenError,
  Link,
  NotFoundError,
  RedirectError,
  RouteView,
  RouterProvider,
  UnauthorizedError,
  forbidden,
  hasClientPage,
  matchRoute,
  notFound,
  parseSearch,
  permanentRedirect,
  redirect,
  resolveFailure,
  resolveMatch,
  routeErrorStatus,
  routerView,
  splitUrl,
  unauthorized,
  useIsServer,
  useLoaderData,
  useRoute,
  useRouter,
} from "./internal/runtime.js";

/** Props a page receives. */
export type PageProps<
  TParams extends { readonly [string]: string | $ReadOnlyArray<string> } = {},
  TData = void,
> = {|
  readonly params: TParams,
  readonly searchParams: { readonly [string]: string },
  readonly data: TData,
|};

/** Props an `_uf.error.js` component receives. */
export type ErrorProps = {|
  readonly error: RouteError,
  readonly reset: () => void,
|};

/** Props a layout receives. */
export type LayoutProps<
  TParams extends { readonly [string]: string | $ReadOnlyArray<string> } = {},
> = {|
  readonly params: TParams,
  readonly children: React.Node,
|};
