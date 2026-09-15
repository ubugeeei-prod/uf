// @flow
//
// Internal to `@uniflowed/server`: `app.router.redirects`, `app.router.rewrites`
// and `app.router.headers`, read one way for every front door.
//
// `uf.config.js` declares the three lists, `@uniflowed/vite` writes them into
// the server bundle as `routing`, and this module is what every host asks —
// `uf dev`, `uf preview`, `uf start`, each `--adapter` target and a compiled
// binary. One reading, because a rule that answered differently in the
// deployment than in the preview it was checked with is the failure the
// front-door comparison in `tests/library/deploy.test.js` exists to catch.
//
// # Where each list is applied
//
// A redirect and a response header are about the URL a visitor asked for, so
// a front door applies both *in front of* its static files: a redirect away
// from a path the build still wrote a file for is still a redirect, and a
// `cache-control` for `/assets/:file*` has to reach the asset. `redirectFor`
// and `headersFor` are what a host's static half calls first.
//
// A rewrite is about which *route* answers, so it is applied where the
// application begins — after the static files and before the middleware.
// That order is two decisions. A catch-all rewrite never swallows a hashed
// chunk, because the chunk was answered before the rewrite was asked. And the
// middleware that runs is the one guarding the route the rewrite reached, so a
// rewrite is never an unguarded way to a guarded page. `./fetch.js`,
// `./standalone.js` and `uf dev` call `rewriteFor`. A rewrite serves a route of
// this application and never another origin: proxying is a route handler that
// fetches, and `uf_config` refuses an absolute destination where it is written.
//
// # The grammar is a route's
//
// `/blog/:slug` and `/docs/:path*`, which is the grammar the route table
// itself is written in — a literal segment, a `:name` that takes one, and a
// trailing `:name*` that takes the rest. Next.js's regular expressions and
// `:name+`/`:name?` modifiers are refused by `uf_config` when the file is
// read, and matching here is a split and a comparison, never a regular
// expression over the request (docs/security.md rule 5).
//
// Segments are compared as they arrived, percent-encoded, and substituted into
// a destination the same way, so a slug is never decoded and encoded again on
// its way through.
//
// # A payload is its document
//
// A browser navigating a React Server Components application fetches
// `/blog/x/__uf.flight` rather than `/blog/x` (`./flight.js`). Every rule is
// matched against the document path, and what it produces is turned back into
// a payload URL — so a redirect during a client navigation lands on the
// target's payload, and a rewrite renders the destination's, exactly as the
// document request for the same address would.

import { flightDocumentPath, flightPath } from "./flight.js";

/** One entry of `app.router.redirects`. */
export type RedirectRule = {|
  readonly source: string,
  readonly destination: string,
  readonly permanent: boolean,
|};

/** One entry of `app.router.rewrites`. */
export type RewriteRule = {|
  readonly source: string,
  readonly destination: string,
|};

/** One entry of `app.router.headers`. */
export type HeaderRule = {|
  readonly source: string,
  readonly headers: { readonly [name: string]: string },
|};

/** What the server bundle exports as `routing`. */
export type RoutingRules = {|
  readonly redirects?: $ReadOnlyArray<RedirectRule>,
  readonly rewrites?: $ReadOnlyArray<RewriteRule>,
  readonly headers?: $ReadOnlyArray<HeaderRule>,
|};

type Segment =
  | {| readonly kind: "static", readonly value: string |}
  | {| readonly kind: "param", readonly name: string |}
  | {| readonly kind: "catchAll", readonly name: string |};

type Params = { [name: string]: string | $ReadOnlyArray<string> };

type Compiled = {|
  readonly redirects: $ReadOnlyArray<{|
    readonly pattern: $ReadOnlyArray<Segment>,
    readonly destination: string,
    readonly status: 307 | 308,
  |}>,
  readonly rewrites: $ReadOnlyArray<{|
    readonly pattern: $ReadOnlyArray<Segment>,
    readonly destination: string,
  |}>,
  readonly headers: $ReadOnlyArray<{|
    readonly pattern: $ReadOnlyArray<Segment>,
    readonly pairs: $ReadOnlyArray<[string, string]>,
  |}>,
|};

const NONE: Compiled = { redirects: [], rewrites: [], headers: [] };

/**
 * Rules already compiled, by the object the bundle exported.
 *
 * The object is a module constant, so its identity is the build's and a
 * request pays a map lookup rather than a parse of every pattern.
 */
const compiledRules: WeakMap<RoutingRules, Compiled> = new WeakMap();

function compile(rules: ?RoutingRules): Compiled {
  if (rules == null) {
    return NONE;
  }
  const cached = compiledRules.get(rules);
  if (cached != null) {
    return cached;
  }
  const compiled: Compiled = {
    redirects: (rules.redirects ?? []).map((rule, index) => ({
      pattern: patternOf(rule.source, `app.router.redirects[${index}].source`),
      destination: rule.destination,
      status: rule.permanent === true ? 308 : 307,
    })),
    rewrites: (rules.rewrites ?? []).map((rule, index) => ({
      pattern: patternOf(rule.source, `app.router.rewrites[${index}].source`),
      destination: rule.destination,
    })),
    headers: (rules.headers ?? []).map((rule, index) => ({
      pattern: patternOf(rule.source, `app.router.headers[${index}].source`),
      pairs: Object.keys(rule.headers).map((name) => [name.toLowerCase(), rule.headers[name]]),
    })),
  };
  compiledRules.set(rules, compiled);
  return compiled;
}

/**
 * A source, as segments.
 *
 * `uf_config` has already refused every spelling this does not read, with a
 * sentence per spelling; what is checked here is what would otherwise be
 * matched wrongly by a driver started by hand on a config `uf` never read.
 */
function patternOf(source: string, where: string): $ReadOnlyArray<Segment> {
  if (!source.startsWith("/")) {
    throw new Error(
      `uf: ${where} is ${JSON.stringify(source)}, and a source is a path starting with \`/\``,
    );
  }
  const parts = segmentsOf(source);
  return parts.map((part, index): Segment => {
    if (!part.startsWith(":")) {
      return { kind: "static", value: part };
    }
    const catchAll = part.endsWith("*");
    const name = catchAll ? part.slice(1, -1) : part.slice(1);
    if (catchAll && index !== parts.length - 1) {
      throw new Error(
        `uf: ${where} is ${JSON.stringify(source)}, and \`:${name}*\` takes the rest of the ` +
          "path, so it has to be the last segment",
      );
    }
    return catchAll ? { kind: "catchAll", name } : { kind: "param", name };
  });
}

function segmentsOf(path: string): $ReadOnlyArray<string> {
  return path.split("/").filter((part) => part !== "");
}

function matchSegments(pattern: $ReadOnlyArray<Segment>, parts: $ReadOnlyArray<string>): ?Params {
  const params: Params = {};
  let index = 0;
  for (const segment of pattern) {
    if (segment.kind === "catchAll") {
      params[segment.name] = parts.slice(index);
      return params;
    }
    if (index >= parts.length) {
      return null;
    }
    if (segment.kind === "param") {
      params[segment.name] = parts[index];
    } else if (parts[index] !== segment.value) {
      return null;
    }
    index += 1;
  }
  return index === parts.length ? params : null;
}

/** The path the rules are matched against, and whether the request was for a payload. */
function documentOf(url: URL): {| readonly pathname: string, readonly payload: boolean |} {
  const document = flightDocumentPath(url.pathname);
  return document == null
    ? { pathname: url.pathname, payload: false }
    : { pathname: document, payload: true };
}

type Destination = {|
  /** Scheme and authority for an absolute destination, and `""` for a path. */
  readonly origin: string,
  readonly path: string,
  /** The query without its `?`, or `null` when the destination has none. */
  readonly query: string | null,
  readonly hash: string,
|};

/** A destination, taken apart without a URL parser: `:path*` is not a URL yet. */
function parseDestination(destination: string): Destination {
  let rest = destination;
  let origin = "";
  const scheme = rest.indexOf("://");
  if (!rest.startsWith("/") && scheme !== -1) {
    let end = scheme + 3;
    while (end < rest.length && rest[end] !== "/" && rest[end] !== "?" && rest[end] !== "#") {
      end += 1;
    }
    origin = rest.slice(0, end);
    rest = rest.slice(end);
  }
  let hash = "";
  const hashAt = rest.indexOf("#");
  if (hashAt !== -1) {
    hash = rest.slice(hashAt);
    rest = rest.slice(0, hashAt);
  }
  const queryAt = rest.indexOf("?");
  const path = queryAt === -1 ? rest : rest.slice(0, queryAt);
  return {
    origin,
    path: path === "" ? "/" : path,
    query: queryAt === -1 ? null : rest.slice(queryAt + 1),
    hash,
  };
}

/**
 * `path` with every `:name` and `:name*` segment replaced by what the source
 * matched.
 *
 * A catch-all that matched nothing takes its segment with it, so
 * `/docs/:path*` for `/old` is `/docs` rather than `/docs/`.
 */
function fill(path: string, params: Params): string {
  const out: Array<string> = [];
  for (const segment of path.split("/")) {
    if (!segment.startsWith(":")) {
      out.push(segment);
      continue;
    }
    const name = segment.endsWith("*") ? segment.slice(1, -1) : segment.slice(1);
    const value = params[name];
    if (value == null) {
      out.push(segment);
      continue;
    }
    const text = typeof value === "string" ? value : value.join("/");
    if (text !== "") {
      out.push(text);
    }
  }
  const joined = out.join("/");
  return joined === "" ? "/" : joined;
}

/**
 * The destination's query, with the request's parameters it does not name.
 *
 * Next.js passes the query through the same way, so a campaign parameter on a
 * redirected link survives the redirect.
 */
function mergedSearch(own: string | null, requested: string): string {
  if (own == null) {
    return requested;
  }
  const params = new URLSearchParams(own);
  const declared = new Set(params.keys());
  for (const [key, value] of new URLSearchParams(requested)) {
    if (!declared.has(key)) {
      params.append(key, value);
    }
  }
  const text = params.toString();
  return text === "" ? "" : `?${text}`;
}

/**
 * The redirect `app.router.redirects` answers this request with, or `null`.
 *
 * The first rule whose source matches wins, in the order the file lists them.
 * `permanent: true` is a `308` and `false` a `307`, which are the two statuses
 * that keep the method and the body — a `301` would turn a redirected `POST`
 * into a `GET`.
 */
export function redirectFor(rules: ?RoutingRules, request: Request): Response | null {
  const { redirects } = compile(rules);
  if (redirects.length === 0) {
    return null;
  }
  const url = new URL(request.url);
  const document = documentOf(url);
  const parts = segmentsOf(document.pathname);
  for (const rule of redirects) {
    const params = matchSegments(rule.pattern, parts);
    if (params == null) {
      continue;
    }
    const target = parseDestination(rule.destination);
    const path = fill(target.path, params);
    const search = mergedSearch(target.query, url.search);
    // Another origin is left as written: what a browser finds there is not a
    // payload, and `fetchFlight` loads it as a document. This origin's is the
    // target's payload, which a navigating `fetch` follows and lands on.
    const location =
      target.origin !== ""
        ? `${target.origin}${path}${search}${target.hash}`
        : document.payload
          ? `${flightPath(path)}${search}`
          : `${path}${search}${target.hash}`;
    return new Response(null, { status: rule.status, headers: { location } });
  }
  return null;
}

/**
 * The request `app.router.rewrites` hands the application instead, or `null`.
 *
 * Same method, headers and body; another path. The address the visitor asked
 * for is untouched, because a rewrite happens on the server and a redirect is
 * the one that tells the browser.
 */
export function rewriteFor(rules: ?RoutingRules, request: Request): Request | null {
  const { rewrites } = compile(rules);
  if (rewrites.length === 0) {
    return null;
  }
  const url = new URL(request.url);
  const document = documentOf(url);
  const parts = segmentsOf(document.pathname);
  for (const rule of rewrites) {
    const params = matchSegments(rule.pattern, parts);
    if (params == null) {
      continue;
    }
    const target = parseDestination(rule.destination);
    const path = fill(target.path, params);
    const rewritten = new URL(url.href);
    rewritten.pathname = document.payload ? flightPath(path) : path;
    rewritten.search = mergedSearch(target.query, url.search);
    return requestAt(request, rewritten);
  }
  return null;
}

/**
 * The response headers `app.router.headers` puts on this request's answer.
 *
 * Every matching rule, in order, lowercased — so a later rule setting the same
 * name wins, which is what applying them one after another with `set` does.
 */
export function headersFor(
  rules: ?RoutingRules,
  request: Request,
): $ReadOnlyArray<[string, string]> {
  const { headers } = compile(rules);
  if (headers.length === 0) {
    return [];
  }
  const parts = segmentsOf(documentOf(new URL(request.url)).pathname);
  const found: Array<[string, string]> = [];
  for (const rule of headers) {
    if (matchSegments(rule.pattern, parts) != null) {
      found.push(...rule.pairs);
    }
  }
  return found;
}

/**
 * `response` with `pairs` set on it, over whatever it already said.
 *
 * The project's rule wins over the answer's own header of the same name,
 * which is what a rule is for. A `Response` whose headers are immutable — one
 * `fetch` returned, or `Response.redirect` built — is copied first; a `101` is
 * returned as it is, because a switched protocol cannot be rebuilt.
 */
export function withHeaders(response: Response, pairs: $ReadOnlyArray<[string, string]>): Response {
  if (pairs.length === 0 || response.status === 101) {
    return response;
  }
  try {
    for (const [name, value] of pairs) {
      response.headers.set(name, value);
    }
    return response;
  } catch {
    const copy = new Response(response.body, response);
    for (const [name, value] of pairs) {
      copy.headers.set(name, value);
    }
    return copy;
  }
}

/**
 * `request`, at another URL.
 *
 * A `Request` is a valid `RequestInit`: its method, headers, body, signal and
 * `duplex` are read off it, so a streamed upload is handed on rather than
 * read, and the original is left unusable rather than read twice.
 */
export function requestAt(request: Request, url: URL): Request {
  // $FlowFixMe[incompatible-call] - a `Request` is read as the `RequestInit` it satisfies.
  return new Request(url.href, request);
}
