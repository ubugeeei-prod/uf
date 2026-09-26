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
//
// # No interception here, deliberately
//
// An intercepting route — a slot's `(.)photo` — is not one of this adapter's
// features, and that is a decision rather than a gap. What interception does
// is a browser's: the address bar says `/feed/photo/1` while the page
// underneath stays on screen with the photo in a slot over it, and a reload
// renders the page the URL names instead. A native navigator has no address bar
// for the screen to disagree with, and it already has the thing interception
// imitates — a screen presented modally over the one below it, with its own
// back gesture. Doing both would give a native app two answers to "open this
// over the feed", one of them the router's and one the navigator's.
//
// So `resolveNativeNavigation` matches `table.routes` and nothing else:
// `/feed/photo/1` resolves to the ordinary `app/feed/photo/[id]` route every
// time, a slot's `intercepts` are never read, and presenting that screen as a
// modal is the navigator's call, made where the rest of its presentation is.

import type { RouteParams, RouteRecord, RouteTable } from "./internal/routing.js";
import { hasClientPage, matchRoute, splitUrl } from "./internal/routing.js";
import { nativeLinkHref } from "./internal/native-links.js";
export { createNativeLinking } from "./internal/native-links.js";
export type { NativeLinkSource } from "./internal/native-links.js";

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

export type NativeScreenEntry = {|
  readonly screen: string,
  readonly route: string,
  readonly file: string,
|};

export type NativeScreenManifest = {|
  readonly screens: NativeScreenMap,
  readonly entries: $ReadOnlyArray<NativeScreenEntry>,
|};

export type NativeScreenNameRoute = {|
  readonly path: string,
  readonly file: string,
|};

export type NativeScreenManifestOptions = {|
  readonly name?: (route: NativeScreenNameRoute) => string,
|};

export type NativeScreenPayload = {|
  readonly screen: string,
  readonly href: string,
  readonly pathname: string,
  readonly search: string,
  readonly route: string,
  readonly params: RouteParams,
|};

export type NativeScreenNavigationState = {|
  readonly params: RouteParams,
  readonly href: string,
  readonly pathname: string,
  readonly search: string,
  readonly route: string,
|};

export type NativeNavigator = {|
  readonly push?: (event: NativeNavigationEvent) => mixed | Promise<mixed>,
  readonly replace?: (event: NativeNavigationEvent) => mixed | Promise<mixed>,
  readonly prefetch?: (event: NativeNavigationEvent) => mixed | Promise<mixed>,
|};

export type NativeScreenNavigator = {|
  readonly push?: (screen: string, state: NativeScreenNavigationState) => mixed | Promise<mixed>,
  readonly replace?: (screen: string, state: NativeScreenNavigationState) => mixed | Promise<mixed>,
  readonly prefetch?: (
    screen: string,
    state: NativeScreenNavigationState,
  ) => mixed | Promise<mixed>,
|};

export type NativeRouter = {|
  readonly push: (to: string) => Promise<void>,
  readonly replace: (to: string) => Promise<void>,
  readonly prefetch: (to: string) => Promise<void>,
  readonly resolve: (to: string, kind?: NativeNavigationKind) => NativeNavigationEvent,
|};

export type NativeNavigationErrorCode =
  | "duplicate-screen"
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

export function createNativeScreenRouter(
  table: RouteTable<mixed, mixed, mixed, mixed, mixed>,
  screens: NativeScreenMap,
  navigator: NativeScreenNavigator,
): NativeRouter {
  return createNativeRouter(table, {
    push: (event) => invokeScreenNavigator(navigator, screens, event),
    replace: (event) => invokeScreenNavigator(navigator, screens, event),
    prefetch: (event) => {
      if (typeof navigator.prefetch !== "function") return;
      return invokeScreenNavigator(navigator, screens, event);
    },
  });
}

export function createNativeScreenManifest(
  table: RouteTable<mixed, mixed, mixed, mixed, mixed>,
  options?: NativeScreenManifestOptions,
): NativeScreenManifest {
  const screens: { [string]: string } = {};
  const entries: Array<NativeScreenEntry> = [];
  const seen = new Map<string, string>();
  for (const route of table.routes) {
    if (!hasClientPage(route)) {
      continue;
    }
    const screen = screenNameFor(route, options);
    const already = seen.get(screen);
    if (already != null) {
      throw new NativeNavigationError(
        "duplicate-screen",
        `@uniflowed/router/native: ${route.path} and ${already} both map to native screen ${screen}`,
        route.path,
        route.path,
      );
    }
    seen.set(screen, route.path);
    screens[route.path] = screen;
    entries.push({ screen, route: route.path, file: route.file });
  }
  return { screens, entries };
}

export function nativeScreenName(routePath: string): string {
  const segments = routePath.split("/").filter((segment) => segment !== "");
  if (segments.length === 0) {
    return "Home";
  }
  const name = segments
    .map((segment) => {
      if (segment.startsWith(":") && segment.endsWith("*?")) {
        return `Any${titlePart(segment.slice(1, -2))}`;
      }
      if (segment.startsWith(":") && segment.endsWith("*")) {
        return `All${titlePart(segment.slice(1, -1))}`;
      }
      if (segment.startsWith(":")) {
        return `By${titlePart(segment.slice(1))}`;
      }
      return titlePart(segment);
    })
    .join("");
  return name === "" ? "Screen" : name;
}

export function resolveNativeNavigation(
  table: RouteTable<mixed, mixed, mixed, mixed, mixed>,
  to: string,
  kind?: NativeNavigationKind = "push",
): NativeNavigationEvent {
  const href = normalizeNativeHref(to, table);
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

export function nativeScreenNavigationState(
  payload: NativeScreenPayload,
): NativeScreenNavigationState {
  return {
    params: payload.params,
    href: payload.href,
    pathname: payload.pathname,
    search: payload.search,
    route: payload.route,
  };
}

function normalizeNativeHref(
  to: string,
  table: RouteTable<mixed, mixed, mixed, mixed, mixed>,
): string {
  if (/^[A-Za-z][A-Za-z0-9+.-]*:/.test(to) || to.startsWith("//")) {
    const claimed = nativeLinkHref(table, to);
    if (claimed != null) return claimed;
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

function screenNameFor(
  route: RouteRecord<mixed, mixed, mixed, mixed, mixed>,
  options?: NativeScreenManifestOptions,
): string {
  const screen =
    options?.name?.({ path: route.path, file: route.file }) ?? nativeScreenName(route.path);
  if (screen === "") {
    throw new NativeNavigationError(
      "missing-screen",
      `@uniflowed/router/native: ${route.path} mapped to an empty native screen name`,
      route.path,
      route.path,
    );
  }
  return screen;
}

function titlePart(segment: string): string {
  const cleaned = segment
    .replace(/^\[+|\]+$/g, "")
    .replace(/[^A-Za-z0-9]+/g, " ")
    .trim();
  if (cleaned === "") {
    return "Segment";
  }
  return cleaned
    .split(/\s+/)
    .map((part) => part.slice(0, 1).toUpperCase() + part.slice(1))
    .join("");
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

async function invokeScreenNavigator(
  navigator: NativeScreenNavigator,
  screens: NativeScreenMap,
  event: NativeNavigationEvent,
): Promise<void> {
  const payload = nativeScreenPayload(event, screens);
  const state = nativeScreenNavigationState(payload);
  if (event.kind === "push") {
    if (typeof navigator.push !== "function") {
      throw missingMethod(event);
    }
    await navigator.push(payload.screen, state);
    return;
  }
  if (event.kind === "replace") {
    if (typeof navigator.replace !== "function") {
      throw missingMethod(event);
    }
    await navigator.replace(payload.screen, state);
    return;
  }
  if (typeof navigator.prefetch !== "function") {
    throw missingMethod(event);
  }
  await navigator.prefetch(payload.screen, state);
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
