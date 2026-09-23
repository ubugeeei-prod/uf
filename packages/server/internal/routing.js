// @flow
//
// Internal to `@uniflowed/server`: `app.router`'s base path, trailing-slash
// policy, redirects, rewrites and response headers, read one way for every
// front door.
//
// `uf.config.js` declares them, `@uniflowed/vite` writes them into the server
// bundle as `routing`, and this module is what every host asks — `uf dev`,
// `uf preview`, `uf start`, each `--adapter` target and a compiled binary. One
// reading, because a rule that answered differently in the deployment than in
// the preview it was checked with is the failure the front-door comparison in
// `tests/library/deploy.test.js` exists to catch.
//
// # Where each is applied
//
// [`admit`] is the first thing a front door asks about a request, before its
// static files: a request outside the base path is a `404`, a request for the
// spelling of a path the trailing-slash policy does not use is a `308` to the
// one it does, and a redirect rule is answered. Whatever is left continues as
// the application path — the base taken off — which is what the static half
// looks up and what the application is handed. `headersFor` goes on whatever
// answered, a file included.
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
// # Admitted once
//
// `handler.js`'s `fetch` admits a request itself, for a host that hands it
// every request with nothing in front — and a host with a static half has
// already admitted it. Admitting twice would take the base off twice, so an
// admitted request is remembered by identity and the second door lets it
// through. A `WeakSet` rather than a header: a header is something a client can
// send.
//
// # The grammar is a route's
//
// `/blog/:slug` and `/docs/:path*`, which is the grammar the route table
// itself is written in — a literal segment, a `:name` that takes one, and a
// trailing `:name*` that takes the rest. Next.js's regular expressions and
// `:name+`/`:name?` modifiers are refused by `uf_config` when the file is
// read, and matching here is a split and a comparison, never a regular
// expression over the request (docs/security.md rule 5). A source is an
// application path, written without the base.
//
// Segments are compared as they arrived, percent-encoded, and substituted into
// a destination the same way, so a slug is never decoded and encoded again on
// its way through.
//
// # A payload is its document
//
// A browser navigating a React Server Components application fetches
// `/blog/x/__uf.flight` rather than `/blog/x`. Every rule is matched against
// the document path, and what it produces is turned back into a payload URL —
// so a redirect during a client navigation lands on the target's payload, and a
// rewrite renders the destination's, exactly as the document request for the
// same address would. A payload URL is never redirected for its trailing slash.

import { mintCurrentNonce, newNonce } from "./context.js";
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

/** Which spelling of a path is the page; see `app.router.trailingSlash`. */
export type TrailingSlash = "never" | "always" | "ignore";

/** What the server bundle exports as `routing`. */
export type RoutingRules = {|
  readonly redirects?: $ReadOnlyArray<RedirectRule>,
  readonly rewrites?: $ReadOnlyArray<RewriteRule>,
  readonly headers?: $ReadOnlyArray<HeaderRule>,
  readonly basePath?: string,
  readonly trailingSlash?: TrailingSlash,
|};

/** What [`admit`] decided: an answer, or the request the application is handed. */
export type Admission =
  | {| readonly kind: "answer", readonly response: Response |}
  | {| readonly kind: "continue", readonly request: Request |};

type Segment =
  | {| readonly kind: "static", readonly value: string |}
  | {| readonly kind: "param", readonly name: string |}
  | {| readonly kind: "catchAll", readonly name: string |};

type Params = { [name: string]: string | $ReadOnlyArray<string> };

type Compiled = {|
  readonly base: string,
  readonly slash: TrailingSlash,
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

const NONE: Compiled = { base: "", slash: "ignore", redirects: [], rewrites: [], headers: [] };

/**
 * Rules already compiled, by the object the bundle exported.
 *
 * The object is a module constant, so its identity is the build's and a
 * request pays a map lookup rather than a parse of every pattern.
 */
const compiledRules: WeakMap<RoutingRules, Compiled> = new WeakMap();

/** Requests a front door has already admitted; see "Admitted once" above. */
const admittedRequests: WeakSet<Request> = new WeakSet();

function compile(rules: ?RoutingRules): Compiled {
  if (rules == null) {
    return NONE;
  }
  const cached = compiledRules.get(rules);
  if (cached != null) {
    return cached;
  }
  const compiled: Compiled = {
    base: withoutTrailingSlashes(rules.basePath ?? ""),
    slash: rules.trailingSlash ?? "ignore",
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

/** `value` with every trailing `/` counted off, never matched with a pattern. */
function withoutTrailingSlashes(value: string): string {
  let end = value.length;
  while (end > 0 && value.charCodeAt(end - 1) === 47) {
    end -= 1;
  }
  return value.slice(0, end);
}

/**
 * The application path an address's pathname names, or `null` outside the
 * base. `/docs/guide` is `/guide` under `/docs`; `/docs` and `/docs/` are both
 * `/`; `/docsx` is outside it, because a base is whole segments.
 */
function applicationPathOf(base: string, pathname: string): string | null {
  if (base === "") {
    return pathname;
  }
  if (pathname === base) {
    return "/";
  }
  return pathname.startsWith(`${base}/`) ? pathname.slice(base.length) : null;
}

/**
 * `path` — an application path — in `policy`'s spelling.
 *
 * The same rule `@uniflowed/router`'s `internal/base-path.js` writes links
 * with, spelled twice because the router cannot import this package's
 * internals into a browser bundle; `../routing.test.js` holds the two to one
 * answer. The root of an application at the root is `/`; the root under a base
 * is the base itself, `""` here, unless the policy is `"always"`. A path whose
 * last segment looks like a file keeps what it was written with.
 */
export function spellPath(path: string, policy: TrailingSlash, underBase: boolean): string {
  const trimmed = withoutTrailingSlashes(path);
  if (trimmed === "") {
    return policy === "always" || !underBase ? "/" : "";
  }
  if (policy === "ignore" || looksLikeAFile(trimmed)) {
    return path;
  }
  return policy === "always" ? `${trimmed}/` : trimmed;
}

function looksLikeAFile(path: string): boolean {
  return path.slice(path.lastIndexOf("/") + 1).includes(".");
}

/** The path the rules are matched against, and whether the request was for a payload. */
function documentOf(pathname: string): {| readonly pathname: string, readonly payload: boolean |} {
  const document = flightDocumentPath(pathname);
  return document == null ? { pathname, payload: false } : { pathname: document, payload: true };
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
 * What a front door does with a request before anything else answers it.
 *
 * In order: a request outside `basePath` is a `404`; a `GET` or `HEAD` for the
 * spelling of a path `trailingSlash` does not use is a `308` to the one it
 * does; a matching redirect rule is answered. Anything else continues as the
 * same request at its application path, and is remembered as admitted.
 *
 * The `404` is plain rather than the project's not-found page, because a page
 * is rendered for an address inside the application and this one is not.
 */
export function admit(rules: ?RoutingRules, request: Request): Admission {
  if (admittedRequests.has(request)) {
    return { kind: "continue", request };
  }
  const compiled = compile(rules);
  const url = new URL(request.url);
  const application = applicationPathOf(compiled.base, url.pathname);
  if (application == null) {
    return {
      kind: "answer",
      response: new Response("404 Not Found\n", {
        status: 404,
        headers: { "content-type": "text/plain; charset=utf-8" },
      }),
    };
  }

  const method = request.method.toUpperCase();
  // Not for `/__uf/`, which is uf's rather than the application's: the image
  // endpoint's URLs are written by `Image` and would otherwise be redirected
  // to a spelling the endpoint does not answer, once per image, under
  // `trailingSlash: "always"`.
  if (
    compiled.slash !== "ignore" &&
    (method === "GET" || method === "HEAD") &&
    flightDocumentPath(application) == null &&
    !application.startsWith("/__uf/")
  ) {
    const address = `${compiled.base}${spellPath(application, compiled.slash, compiled.base !== "")}`;
    if (address !== url.pathname) {
      return {
        kind: "answer",
        response: new Response(null, {
          status: 308,
          headers: { location: `${address}${url.search}` },
        }),
      };
    }
  }

  const moved = redirectFor(compiled, application, url.search);
  if (moved != null) {
    return { kind: "answer", response: moved };
  }

  let admitted = request;
  if (compiled.base !== "") {
    const inside = new URL(url.href);
    inside.pathname = application;
    admitted = requestAt(request, inside);
  }
  admittedRequests.add(admitted);
  return { kind: "continue", request: admitted };
}

/** Whether a front door has already admitted this request object. */
export function wasAdmitted(request: Request): boolean {
  return admittedRequests.has(request);
}

/**
 * The redirect `app.router.redirects` answers an application path with, or
 * `null`.
 *
 * The first rule whose source matches wins, in the order the file lists them.
 * `permanent: true` is a `308` and `false` a `307`, which are the two statuses
 * that keep the method and the body — a `301` would turn a redirected `POST`
 * into a `GET`. A destination on this application gets the base in front and
 * the trailing-slash policy's spelling, so following it is not a second
 * redirect.
 */
function redirectFor(compiled: Compiled, pathname: string, search: string): Response | null {
  if (compiled.redirects.length === 0) {
    return null;
  }
  const document = documentOf(pathname);
  const parts = segmentsOf(document.pathname);
  for (const rule of compiled.redirects) {
    const params = matchSegments(rule.pattern, parts);
    if (params == null) {
      continue;
    }
    const target = parseDestination(rule.destination);
    const path = fill(target.path, params);
    const query = mergedSearch(target.query, search);
    // Another origin is left as written: what a browser finds there is not a
    // payload, and `fetchFlight` loads it as a document. This origin's is the
    // target's payload, which a navigating `fetch` follows and lands on.
    let location;
    if (target.origin !== "") {
      location = `${target.origin}${path}${query}${target.hash}`;
    } else if (document.payload) {
      location = `${compiled.base}${flightPath(path)}${query}`;
    } else {
      const spelled = spellPath(path, compiled.slash, compiled.base !== "");
      location = `${compiled.base}${spelled === "" ? "" : spelled}${query}${target.hash}`;
    }
    return new Response(null, { status: rule.status, headers: { location } });
  }
  return null;
}

/**
 * The request `app.router.rewrites` hands the application instead, or `null`.
 *
 * Asked of an admitted request, whose path is already the application's. Same
 * method, headers and body; another path. The address the visitor asked for is
 * untouched, because a rewrite happens on the server and a redirect is the one
 * that tells the browser.
 */
export function rewriteFor(rules: ?RoutingRules, request: Request): Request | null {
  const { rewrites } = compile(rules);
  if (rewrites.length === 0) {
    return null;
  }
  const url = new URL(request.url);
  const document = documentOf(url.pathname);
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
    const next = requestAt(request, rewritten);
    if (admittedRequests.has(request)) {
      admittedRequests.add(next);
    }
    return next;
  }
  return null;
}

/**
 * The response headers `app.router.headers` puts on this request's answer.
 *
 * Asked of the request as it arrived: the base is taken off here, and a
 * request outside it gets none. Every matching rule, in order, lowercased — so
 * a later rule setting the same name wins, which is what applying them one
 * after another with `set` does.
 */
export function headersFor(
  rules: ?RoutingRules,
  request: Request,
): $ReadOnlyArray<[string, string]> {
  const compiled = compile(rules);
  if (compiled.headers.length === 0) {
    return [];
  }
  const url = new URL(request.url);
  const application = admittedRequests.has(request)
    ? url.pathname
    : applicationPathOf(compiled.base, url.pathname);
  if (application == null) {
    return [];
  }
  const parts = segmentsOf(documentOf(application).pathname);
  const found: Array<[string, string]> = [];
  for (const rule of compiled.headers) {
    if (matchSegments(rule.pattern, parts) != null) {
      found.push(...rule.pairs);
    }
  }
  return withNonce(found);
}

/** The token an `app.router.headers` value writes where the nonce goes. */
const NONCE_TOKEN = "{uf.nonce}";

/**
 * `pairs`, with `{uf.nonce}` replaced by this request's nonce.
 *
 * The short way to set a nonce policy: one rule in `uf.config.js` says
 *
 *     { source: "/:path*", headers: { "content-security-policy":
 *         "script-src 'nonce-{uf.nonce}' 'strict-dynamic'; object-src 'none'" } }
 *
 * and the header and the markup are then the same value read twice rather than
 * two values generated separately. Reading it here is what *mints* it, which
 * is what tells the renderer this response's documents carry one — see
 * `./context.js`'s `nonce` field.
 *
 * A rule that names no nonce is returned untouched and costs one `indexOf` per
 * matching rule, so a project that has never heard of this pays nothing.
 *
 * # Outside a request it is still substituted, with a nonce of its own
 *
 * A response answered before the request was established — a redirect decided
 * by `admit`, which `uf start` reaches before `beginRequest` and a Worker
 * reaches after — has no request nonce to read. The token must not survive
 * either way: a literal `{uf.nonce}` in a policy names a nonce no script has,
 * and an empty `'nonce-'` admits nothing; both block every script on the page.
 *
 * So a standalone nonce is generated for it. That costs 16 bytes on a response
 * which, having no document, has no script to admit — and it buys the property
 * `tests/library/deploy.test.js` checks: **every front door substitutes**.
 * Dropping the header instead made the doors disagree, because which of them
 * answers a redirect inside a request is an ordering difference between hosts
 * rather than a decision about nonces, and a policy that appears on four doors
 * and not the fifth is the class of drift that file exists to catch.
 */
function withNonce(pairs: Array<[string, string]>): $ReadOnlyArray<[string, string]> {
  if (!pairs.some(([, value]) => value.includes(NONCE_TOKEN))) {
    return pairs;
  }
  const nonce = mintCurrentNonce() ?? newNonce();
  return pairs.map(([name, value]) =>
    value.includes(NONCE_TOKEN) ? [name, value.split(NONCE_TOKEN).join(nonce)] : [name, value],
  );
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
    const copy = new Response(response.body, {
      headers: response.headers,
      status: response.status,
      statusText: response.statusText,
    });
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
  // A `Request` is read as the `RequestInit` it satisfies, which Flow's library
  // definition cannot say: `RequestOptions` is an object type, so a class
  // instance is not one, and it has no `duplex`. Spelling the init out instead
  // would be wrong rather than merely long: a navigation request's `mode` is
  // `navigate`, which the constructor refuses from an init and accepts from a
  // `Request`. The codes are the three errors that one fact produces; they
  // read `incompatible-call` before Flow renamed it.
  // $FlowFixMe[class-object-subtyping]
  // $FlowFixMe[incompatible-variance]
  // $FlowFixMe[incompatible-type]
  return new Request(url.href, request);
}
