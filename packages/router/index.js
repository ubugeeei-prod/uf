// @flow
//
// `@uniflowed/router`.

import type * as React from "@uniflowed/react";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/router";

export type RoutePath = string;
export type RouteParams = { +[string]: string };
export type SearchParams = { +[string]: string | $ReadOnlyArray<string> };

export type Metadata = {
  +title?: string,
  +description?: string,
  +openGraph?: {
    +title?: string,
    +description?: string,
    +images?: $ReadOnlyArray<string>,
  },
};

export type PageProps<
  TParams: RouteParams = {},
  TSearch: SearchParams = {},
> = {
  +params: TParams,
  +searchParams: TSearch,
};

export type LayoutProps<TParams: RouteParams = {}> = {
  +params: TParams,
  +children: React.Node,
};

export type RouteModule = {
  +default?: component() renders React.Node,
  +loader?: () => mixed | Promise<mixed>,
  +action?: () => mixed | Promise<mixed>,
  +generateMetadata?: <TParams: RouteParams>(
    props: PageProps<TParams>,
  ) => Metadata | Promise<Metadata>,
  +generateStaticParams?: <TParams: RouteParams>() =>
    | $ReadOnlyArray<TParams>
    | Promise<$ReadOnlyArray<TParams>>,
};

export function FileRoute<TRoute: RoutePath>(
  path: TRoute,
  module: RouteModule,
): RouteModule {
  return nativeRuntimeRequired(MODULE, "FileRoute");
}

export function routerView(root: "./app"): RouteModule {
  return nativeRuntimeRequired(MODULE, "routerView");
}

export function loader<T>(body: () => T | Promise<T>): () => Promise<T> {
  return nativeRuntimeRequired(MODULE, "loader");
}

export function action<T>(body: () => T | Promise<T>): () => Promise<T> {
  return nativeRuntimeRequired(MODULE, "action");
}

export function next(): mixed {
  return nativeRuntimeRequired(MODULE, "next");
}

export function redirect<TRoute: RoutePath>(to: TRoute): empty {
  return nativeRuntimeRequired(MODULE, "redirect");
}

export function permanentRedirect<TRoute: RoutePath>(to: TRoute): empty {
  return nativeRuntimeRequired(MODULE, "permanentRedirect");
}

export function notFound(): empty {
  return nativeRuntimeRequired(MODULE, "notFound");
}

export function revalidatePath<TRoute: RoutePath>(to: TRoute): void {
  return nativeRuntimeRequired(MODULE, "revalidatePath");
}

export function revalidateTag(tag: string): void {
  return nativeRuntimeRequired(MODULE, "revalidateTag");
}

export function noStore(): void {
  return nativeRuntimeRequired(MODULE, "noStore");
}

export function useRoute<TRoute: RoutePath, TParams: RouteParams>(): {
  +path: TRoute,
  +params: TParams,
} {
  return nativeRuntimeRequired(MODULE, "useRoute");
}

export function useRouter<TRoute: RoutePath>(): {
  +push: (to: TRoute) => void,
  +replace: (to: TRoute) => void,
  +prefetch: (to: TRoute) => Promise<void>,
  +refresh: () => void,
} {
  return nativeRuntimeRequired(MODULE, "useRouter");
}

export function defineNavigationGuard<TRoute: RoutePath>(
  guard: (to: TRoute, from: TRoute) => boolean | Promise<boolean>,
): (to: TRoute, from: TRoute) => Promise<boolean> {
  return nativeRuntimeRequired(MODULE, "defineNavigationGuard");
}
