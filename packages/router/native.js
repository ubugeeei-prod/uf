// @flow
//
// `@uniflowed/router/native`: the React Native navigator contract.
//
// Native routes use the same generated table as the web router, but not the
// browser's URL bar or History API. This adapter keeps the shared part small:
// it resolves a uf route path against the table, refuses destinations the
// native bundle cannot render, and hands a navigator-shaped event to the app's
// own navigation runtime. Rendering the tree is still the renderer/host-config
// half of the React Native target.

import type { RouteParams, RouteTable } from "./internal/routing.js";
import { hasClientPage, matchRoute, splitUrl } from "./internal/routing.js";

export type NativeNavigationKind = "push" | "replace" | "prefetch";

export type NativeNavigationEvent = {|
  readonly kind: NativeNavigationKind,
  readonly href: string,
  readonly pathname: string,
  readonly search: string,
  readonly route: string,
  readonly params: RouteParams,
|};

export type NativeScreenMap = {
  readonly [route: string]: string,
};

export type NativeScreenPayload = {|
  readonly screen: string,
  readonly href: string,
  readonly pathname: string,
  readonly search: string,
  readonly route: string,
  readonly params: RouteParams,
|};

export type NativeNavigator = {|
  readonly push?: (event: NativeNavigationEvent) => mixed | Promise<mixed>,
  readonly replace?: (event: NativeNavigationEvent) => mixed | Promise<mixed>,
  readonly prefetch?: (event: NativeNavigationEvent) => mixed | Promise<mixed>,
|};

export type NativeRouter = {|
  readonly push: (to: string) => Promise<void>,
  readonly replace: (to: string) => Promise<void>,
  readonly prefetch: (to: string) => Promise<void>,
  readonly resolve: (to: string, kind?: NativeNavigationKind) => NativeNavigationEvent,
|};

export type NativeNavigationErrorCode =
  | "external-url"
  | "fragment"
  | "relative-url"
  | "missing-route"
  | "missing-screen"
  | "server-only-route"
  | "missing-navigator-method";

export class NativeNavigationError extends Error {
  code: NativeNavigationErrorCode;
  href: string;
  route: ?string;

  constructor(code: NativeNavigationErrorCode, message: string, href: string, route?: ?string) {
    super(message);
    this.name = "NativeNavigationError";
    this.code = code;
    this.href = href;
    this.route = route ?? null;
  }
}

export function createNativeRouter(
  table: RouteTable<mixed, mixed, mixed, mixed, mixed>,
  navigator: NativeNavigator,
): NativeRouter {
  const resolve = (to: string, kind?: NativeNavigationKind = "push"): NativeNavigationEvent =>
    resolveNativeNavigation(table, to, kind);

  return {
    resolve,
    push: async (to) => {
      const event = resolve(to, "push");
      await invokeNavigator(navigator, event);
    },
    replace: async (to) => {
      const event = resolve(to, "replace");
      await invokeNavigator(navigator, event);
    },
    prefetch: async (to) => {
      const event = resolve(to, "prefetch");
      await loadNativeRoute(table, event.pathname);
      if (typeof navigator.prefetch === "function") {
        await navigator.prefetch(event);
      }
    },
  };
}

export function resolveNativeNavigation(
  table: RouteTable<mixed, mixed, mixed, mixed, mixed>,
  to: string,
  kind?: NativeNavigationKind = "push",
): NativeNavigationEvent {
  const href = normalizeNativeHref(to);
  const { pathname, search } = splitUrl(href);
  const matched = matchRoute(table.routes, pathname);
  if (matched == null) {
    throw new NativeNavigationError(
      "missing-route",
      `@uniflowed/router/native: ${href} does not match a native route in this table`,
      href,
    );
  }
  if (!hasClientPage(matched.route)) {
    throw new NativeNavigationError(
      "server-only-route",
      `@uniflowed/router/native: ${matched.route.path} is not in the native route table; ` +
        "it has no page module for this bundle",
      href,
      matched.route.path,
    );
  }
  return {
    kind,
    href,
    pathname,
    search,
    route: matched.route.path,
    params: matched.params,
  };
}

export function nativeScreenPayload(
  event: NativeNavigationEvent,
  screens: NativeScreenMap,
): NativeScreenPayload {
  const screen = screens[event.route];
  if (typeof screen !== "string" || screen === "") {
    throw new NativeNavigationError(
      "missing-screen",
      `@uniflowed/router/native: ${event.route} has no native screen mapping`,
      event.href,
      event.route,
    );
  }
  return {
    screen,
    href: event.href,
    pathname: event.pathname,
    search: event.search,
    route: event.route,
    params: event.params,
  };
}

function normalizeNativeHref(to: string): string {
  if (/^[A-Za-z][A-Za-z0-9+.-]*:/.test(to) || to.startsWith("//")) {
    throw new NativeNavigationError(
      "external-url",
      `@uniflowed/router/native: ${to} is an external URL, and a native navigator needs an app route`,
      to,
    );
  }
  if (!to.startsWith("/")) {
    throw new NativeNavigationError(
      "relative-url",
      `@uniflowed/router/native: ${to} is relative, and native navigation has no document URL to resolve it against`,
      to,
    );
  }
  if (to.includes("#")) {
    throw new NativeNavigationError(
      "fragment",
      `@uniflowed/router/native: ${to} contains a fragment, and native routes do not have document anchors`,
      to,
    );
  }
  return splitUrl(to).pathname + splitUrl(to).search;
}

async function invokeNavigator(
  navigator: NativeNavigator,
  event: NativeNavigationEvent,
): Promise<void> {
  if (event.kind === "push") {
    if (typeof navigator.push !== "function") {
      throw missingMethod(event);
    }
    await navigator.push(event);
    return;
  }
  if (event.kind === "replace") {
    if (typeof navigator.replace !== "function") {
      throw missingMethod(event);
    }
    await navigator.replace(event);
  }
}

function missingMethod(event: NativeNavigationEvent): NativeNavigationError {
  return new NativeNavigationError(
    "missing-navigator-method",
    `@uniflowed/router/native: the native navigator does not implement ${event.kind}()`,
    event.href,
    event.route,
  );
}

async function loadNativeRoute(
  table: RouteTable<mixed, mixed, mixed, mixed, mixed>,
  pathname: string,
): Promise<void> {
  const matched = matchRoute(table.routes, pathname);
  if (matched == null || !hasClientPage(matched.route)) {
    return;
  }
  await Promise.all([matched.route.page?.(), ...matched.route.layouts.map((layout) => layout())]);
}
