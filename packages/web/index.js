// @flow
//
// `@uniflowed/web`.

import type * as React from "@uniflowed/react";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/web";

export type LinkPrefetch = "off" | "intent" | "render";
export type RoutePath = string;
export type RouteParams = { readonly [string]: string };

export type LinkProps<TRoute extends RoutePath> = {
  readonly to: TRoute,
  readonly prefetch?: LinkPrefetch,
  readonly children?: React.Node,
};

export component Font(src: string, family: string, preload?: boolean) {
  return nativeRuntimeRequired(MODULE, "Font");
}

export component Image(src: string, alt: string, width?: number, height?: number) {
  return nativeRuntimeRequired(MODULE, "Image");
}

export component OgImage(title: string, description?: string) {
  return nativeRuntimeRequired(MODULE, "OgImage");
}

export component Link<TRoute extends RoutePath>(
  to: TRoute,
  prefetch?: LinkPrefetch,
  children?: React.Node,
) renders React.Node {
  return nativeRuntimeRequired(MODULE, "Link");
}

export component Page(children?: React.Node) renders React.Node {
  return nativeRuntimeRequired(MODULE, "Page");
}

export component Layout(children?: React.Node) renders React.Node {
  return nativeRuntimeRequired(MODULE, "Layout");
}

export component Time(value: Date | string, format?: string) renders React.Node {
  return nativeRuntimeRequired(MODULE, "Time");
}

export component Announcer(children?: React.Node) renders React.Node {
  return nativeRuntimeRequired(MODULE, "Announcer");
}

export component Picture(
  src: string,
  alt: string,
  sources?: $ReadOnlyArray<string>,
) renders React.Node {
  return nativeRuntimeRequired(MODULE, "Picture");
}

export function useCookie<T>(
  name: string,
  options?: { readonly httpOnly?: boolean, readonly sameSite?: "lax" | "strict" | "none" },
): [null | T, (next: T) => void] {
  return nativeRuntimeRequired(MODULE, "useCookie");
}

export function useHead(head: {
  readonly title?: string,
  readonly meta?: $ReadOnlyArray<{ readonly name: string, readonly content: string }>,
  readonly links?: $ReadOnlyArray<{ readonly rel: string, readonly href: string }>,
}): void {
  return nativeRuntimeRequired(MODULE, "useHead");
}

export function useRoute<TRoute extends RoutePath, TParams extends RouteParams>(): {
  readonly path: TRoute,
  readonly params: TParams,
} {
  return nativeRuntimeRequired(MODULE, "useRoute");
}

export function useRouter<TRoute extends RoutePath>(): {
  readonly push: (to: TRoute) => void,
  readonly replace: (to: TRoute) => void,
  readonly prefetch: (to: TRoute) => Promise<void>,
} {
  return nativeRuntimeRequired(MODULE, "useRouter");
}

export function defineNavigationGuard<TRoute extends RoutePath>(
  guard: (to: TRoute, from: TRoute) => boolean | Promise<boolean>,
): (to: TRoute, from: TRoute) => Promise<boolean> {
  return nativeRuntimeRequired(MODULE, "defineNavigationGuard");
}
