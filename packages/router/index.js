// @flow
//
// `@uniflowed/router`: the file-system router.
//
// Pages live in `app/` as `_uf.page.js` (or `.mdx`), layouts as
// `_uf.layout.js`, and `app.js` exports `routerView("./app")`. The route table
// is generated from the directory at build time; this module is the runtime
// that matches, loads, navigates and renders it.

export type {
  AppProps,
  LayoutModule,
  LinkPrefetch,
  LoaderArgs,
  Metadata,
  MetadataArgs,
  NavigateOptions,
  PageModule,
  ResolvedRoute,
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
  Link,
  NotFoundError,
  RedirectError,
  RouteView,
  RouterProvider,
  matchRoute,
  notFound,
  parseSearch,
  permanentRedirect,
  redirect,
  resolveMatch,
  routerView,
  splitUrl,
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

/** Props a layout receives. */
export type LayoutProps<
  TParams extends { readonly [string]: string | $ReadOnlyArray<string> } = {},
> = {|
  readonly params: TParams,
  readonly children: React$Node,
|};
