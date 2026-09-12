// @flow
//
// Internal to `@uniflowed/router`: the React-free route table primitives.
//
// Server Components need to be able to name routes, throw router control
// errors, and match generated tables without importing the client router,
// React components, or hooks. Keep this file to data, errors, and pure
// functions; rendering belongs in `runtime.js`.

/** One parameter a route path captures. */
export type RouteParamSpec = {| readonly name: string, readonly catchAll: boolean |};

/** The parameters captured from a URL. A catch-all captures the rest as a list. */
export type RouteParams = { readonly [string]: string | $ReadOnlyArray<string> };

/** The query string, as a read-only map. */
export type SearchParams = { readonly [string]: string };

/** A lazy route module entry from the generated route table. */
export type RouteModule<TModule = mixed> = () => Promise<TModule>;

/** One `$template.js`, as the route table carries it. */
export type TemplateRecord<TTemplate = mixed> = {|
  readonly above: number,
  readonly module: RouteModule<TTemplate>,
|};

/** One `$loading.js`, as the route table carries it. */
export type LoadingRecord<TLoading = mixed> = {|
  readonly above: number,
  readonly module: RouteModule<TLoading>,
|};

/** One parallel-route slot, as the route table carries it. */
export type SlotRecord<TPage = mixed, TLayout = mixed> = {|
  readonly name: string,
  readonly above: number,
  readonly defaultPage: ?RouteModule<TPage>,
  readonly defaultFile?: string,
  readonly defaultMdx?: boolean,
  readonly routes: $ReadOnlyArray<SlotRouteRecord<TPage, TLayout>>,
|};

/** One page inside a slot. */
export type SlotRouteRecord<TPage = mixed, TLayout = mixed> = {|
  readonly path: string,
  readonly params: $ReadOnlyArray<RouteParamSpec>,
  readonly mdx: boolean,
  readonly file: string,
  readonly page: RouteModule<TPage>,
  readonly layouts: $ReadOnlyArray<RouteModule<TLayout>>,
  readonly slots: $ReadOnlyArray<SlotRecord<TPage, TLayout>>,
|};

/** One entry of the generated route table. */
export type RouteRecord<TPage = mixed, TLayout = mixed, TTemplate = mixed, TLoading = mixed> = {|
  readonly path: string,
  readonly params: $ReadOnlyArray<RouteParamSpec>,
  readonly mdx: boolean,
  readonly file: string,
  readonly page?: RouteModule<TPage>,
  readonly layouts: $ReadOnlyArray<RouteModule<TLayout>>,
  readonly loading?: $ReadOnlyArray<LoadingRecord<TLoading>>,
  readonly templates?: $ReadOnlyArray<TemplateRecord<TTemplate>>,
  readonly slots?: $ReadOnlyArray<SlotRecord<TPage, TLayout>>,
|};

/** One not-found boundary: the page for a path under `path` that matched nothing. */
export type NotFoundBoundary<TPage = mixed, TLayout = mixed> = {|
  readonly path: string,
  readonly mdx: boolean,
  readonly file: string,
  readonly page: ?RouteModule<TPage>,
  readonly layouts: $ReadOnlyArray<RouteModule<TLayout>>,
|};

/** One error boundary: what renders in place of a subtree that threw. */
export type ErrorBoundary<TError = mixed, TLayout = mixed> = {|
  readonly path: string,
  readonly file: string,
  readonly module: ?RouteModule<TError>,
  readonly layouts: $ReadOnlyArray<RouteModule<TLayout>>,
|};

/** A route table plus the boundaries declared under it. */
export type RouteTable<
  TPage = mixed,
  TLayout = mixed,
  TTemplate = mixed,
  TLoading = mixed,
  TError = mixed,
> = {|
  readonly routes: $ReadOnlyArray<RouteRecord<TPage, TLayout, TTemplate, TLoading>>,
  readonly notFound: $ReadOnlyArray<NotFoundBoundary<TPage, TLayout>>,
  readonly errors: $ReadOnlyArray<ErrorBoundary<TError, TLayout>>,
|};

type UnknownRouteRecord = RouteRecord<mixed, mixed, mixed, mixed>;

/** A URL matched against a table. */
export type RouteMatch<TRoute: { +path: string, ... } = UnknownRouteRecord> = {|
  readonly route: TRoute,
  readonly params: RouteParams,
|};

/**
 * Why the router is rendering an error boundary instead of a page.
 *
 * One union rather than one file convention per status. `forbidden()` and
 * `unauthorized()` are not different *kinds* of file to write; they are
 * different sentences an error page says.
 */
export type RouteError =
  | {| readonly kind: "thrown", readonly error: mixed |}
  | {| readonly kind: "unauthorized" |}
  | {| readonly kind: "forbidden" |};

/** The status a `RouteError` answers with. */
export function routeErrorStatus(error: RouteError): 401 | 403 | 500 {
  return match (error) {
    {kind: "unauthorized"} => 401,
    {kind: "forbidden"} => 403,
    {kind: "thrown"} => 500,
  };
}

/** Thrown by `notFound()`; the renderer answers with the not-found page. */
export class NotFoundError extends Error {
  constructor() {
    super("not found");
    this.name = "NotFoundError";
  }
}

/** Thrown by `unauthorized()`; the renderer answers with the error boundary. */
export class UnauthorizedError extends Error {
  constructor() {
    super("unauthorized");
    this.name = "UnauthorizedError";
  }
}

/** Thrown by `forbidden()`; the renderer answers with the error boundary. */
export class ForbiddenError extends Error {
  constructor() {
    super("forbidden");
    this.name = "ForbiddenError";
  }
}

/** Thrown by `redirect()`; the renderer answers with a redirect. */
export class RedirectError extends Error {
  to: string;
  permanent: boolean;

  constructor(to: string, permanent: boolean) {
    super(`redirect to ${to}`);
    this.name = "RedirectError";
    this.to = to;
    this.permanent = permanent;
  }
}

type Segment =
  | {| readonly kind: "static", readonly value: string |}
  | {| readonly kind: "param", readonly name: string |}
  | {| readonly kind: "catchAll", readonly name: string |};

function compile(routePath: string): $ReadOnlyArray<Segment> {
  return routePath
    .split("/")
    .filter((segment) => segment !== "")
    .map((segment): Segment => {
      if (segment.startsWith(":") && segment.endsWith("*")) {
        return { kind: "catchAll", name: segment.slice(1, -1) };
      }
      if (segment.startsWith(":")) {
        return { kind: "param", name: segment.slice(1) };
      }
      return { kind: "static", value: segment };
    });
}

/**
 * How specific a route is, for ranking: a static segment outranks a parameter,
 * which outranks a catch-all, and a longer path outranks a shorter one.
 */
function specificity(segments: $ReadOnlyArray<Segment>): number {
  let score = 0;
  for (const segment of segments) {
    score += match (segment) {
      {kind: "static"} => 3,
      {kind: "param"} => 2,
      {kind: "catchAll"} => 1,
    };
  }
  return score;
}

function matchSegments(
  segments: $ReadOnlyArray<Segment>,
  parts: $ReadOnlyArray<string>,
): ?RouteParams {
  const params: { [string]: string | $ReadOnlyArray<string> } = {};
  let index = 0;
  for (const segment of segments) {
    match (segment) {
      {kind: "static", value: const value} => {
        if (parts[index] !== value) {
          return null;
        }
        index += 1;
      }
      {kind: "param", name: const name} => {
        if (index >= parts.length) {
          return null;
        }
        params[name] = decodeSegment(parts[index]);
        index += 1;
      }
      {kind: "catchAll", name: const name} => {
        params[name] = parts.slice(index).map(decodeSegment);
        index = parts.length;
      }
    }
  }
  return index === parts.length ? params : null;
}

/**
 * The URL for a route pattern and the parameters it takes.
 *
 * The inverse of [`matchSegments`], and deliberately built out of the same
 * [`compile`]: a builder that parsed patterns its own way would drift from the
 * matcher, and the drift would show up as a link that 404s rather than as a
 * failure anybody could see.
 */
export function buildRoute(routePath: string, params?: RouteParams): string {
  const values: RouteParams = params ?? {};
  const parts: Array<string> = [];
  for (const segment of compile(routePath)) {
    match (segment) {
      {kind: "static", value: const value} => {
        parts.push(value);
      }
      {kind: "param", name: const name} => {
        const value = values[name];
        if (typeof value !== "string") {
          throw new Error(
            `route ${routePath} takes a string for :${name}, and got ${describeParam(value)}`,
          );
        }
        parts.push(encodeURIComponent(value));
      }
      {kind: "catchAll", name: const name} => {
        const value = values[name];
        if (value == null || typeof value === "string") {
          throw new Error(
            `route ${routePath} takes an array of segments for :${name}*, and got ` +
              describeParam(value),
          );
        }
        for (const part of value) {
          parts.push(encodeURIComponent(part));
        }
      }
    }
  }
  return parts.length === 0 ? "/" : `/${parts.join("/")}`;
}

/** What a parameter was, for the message that says it was the wrong thing. */
function describeParam(value: string | $ReadOnlyArray<string> | void): string {
  if (value === undefined) {
    return "nothing";
  }
  return typeof value === "string" ? `the string ${JSON.stringify(value)}` : "an array";
}

function decodeSegment(segment: string): string {
  try {
    return decodeURIComponent(segment);
  } catch {
    return segment;
  }
}

/** Whether this table can render the route in the browser. */
export function hasClientPage(route: { +page?: mixed, ... }): boolean {
  return route.page != null;
}

/** Match a pathname against the table, preferring the most specific route. */
export function matchRoute<TRoute: { +path: string, ... }>(
  routes: $ReadOnlyArray<TRoute>,
  pathname: string,
): ?RouteMatch<TRoute> {
  return matchIn(routes, pathname);
}

/**
 * The same match, over anything that has a route path.
 *
 * A slot is a second table matched against the same URL, and it has to be
 * matched by this function rather than by one of its own.
 */
export function matchIn<TRoute: { +path: string, ... }>(
  routes: $ReadOnlyArray<TRoute>,
  pathname: string,
): ?RouteMatch<TRoute> {
  const parts = pathname.split("/").filter((part) => part !== "");
  let best: ?RouteMatch<TRoute> = null;
  let bestScore = -1;
  for (const route of routes) {
    const segments = compile(route.path);
    const params = matchSegments(segments, parts);
    if (params == null) {
      continue;
    }
    const score = specificity(segments);
    if (score > bestScore) {
      best = { route, params };
      bestScore = score;
    }
  }
  return best;
}

/**
 * Whether a boundary declared at `segments` is at or above `parts`.
 *
 * The same segment kinds as [`matchSegments`], stopping when the boundary's
 * own segments run out instead of requiring the path to.
 */
function covers(segments: $ReadOnlyArray<Segment>, parts: $ReadOnlyArray<string>): boolean {
  let index = 0;
  for (const segment of segments) {
    const next = match (segment) {
      {kind: "static", value: const value} => parts[index] === value ? index + 1 : -1,
      {kind: "param"} => index < parts.length ? index + 1 : -1,
      {kind: "catchAll"} => parts.length,
    };
    if (next === -1) {
      return false;
    }
    index = next;
  }
  return true;
}

/** The nearest boundary above `pathname`, or `null` when none covers it. */
export function nearestBoundary<TBoundary: { readonly path: string, ... }>(
  boundaries: $ReadOnlyArray<TBoundary>,
  pathname: string,
): ?TBoundary {
  const parts = pathname.split("/").filter((part) => part !== "");
  let best: ?TBoundary = null;
  let bestDepth = -1;
  for (const boundary of boundaries) {
    const segments = compile(boundary.path);
    if (!covers(segments, parts)) {
      continue;
    }
    if (segments.length > bestDepth) {
      best = boundary;
      bestDepth = segments.length;
    }
  }
  return best;
}

/** Split a URL into its pathname and search string. */
export function splitUrl(url: string): {| readonly pathname: string, readonly search: string |} {
  const hash = url.indexOf("#");
  const withoutHash = hash === -1 ? url : url.slice(0, hash);
  const question = withoutHash.indexOf("?");
  if (question === -1) {
    return { pathname: normalizePathname(withoutHash), search: "" };
  }
  return {
    pathname: normalizePathname(withoutHash.slice(0, question)),
    search: withoutHash.slice(question),
  };
}

function normalizePathname(pathname: string): string {
  if (pathname === "" || pathname === "/") {
    return "/";
  }
  const trimmed = pathname.replace(/\/+$/, "");
  return trimmed === "" ? "/" : trimmed;
}

/** Parse a search string into a flat map; a repeated key keeps its last value. */
export function parseSearch(search: string): SearchParams {
  const params: { [string]: string } = {};
  for (const [key, value] of new URLSearchParams(search)) {
    params[key] = value;
  }
  return params;
}

/** Stop rendering the current page and show the not-found page instead. */
export function notFound(): empty {
  throw new NotFoundError();
}

/** Stop rendering the current page and show the error boundary, as a 401. */
export function unauthorized(): empty {
  throw new UnauthorizedError();
}

/** Stop rendering the current page and show the error boundary, as a 403. */
export function forbidden(): empty {
  throw new ForbiddenError();
}

/** Stop rendering the current page and send the visitor elsewhere. */
export function redirect(to: string): empty {
  throw new RedirectError(to, false);
}

/** `redirect`, with a permanent status. */
export function permanentRedirect(to: string): empty {
  throw new RedirectError(to, true);
}
