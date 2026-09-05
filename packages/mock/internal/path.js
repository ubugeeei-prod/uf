// @flow
//
// Internal to `@uniflowed/mock`: the path grammar a handler is written in.
//
// Its own module because it is the only part of this package that is a
// *grammar*, and a grammar is a pile of edge cases — a pattern that names an
// origin against one that does not, a parameter next to a literal, a trailing
// slash, a percent-encoded segment — none of which need a `fetch`, a handler
// or a registry to be decided. Keeping it here means those decisions are made
// in one place and can be read without following an interception path.
//
// The syntax is the one every router in this repository uses, so a path copied
// out of `@uniflowed/router` matches the same URLs here:
//
//   /users/:id          one segment, captured as `params.id`
//   /files/*            the rest of the path, captured as `params["*"]`
//   *                   everything
//   https://api.test/v1/users/:id   the same, pinned to one origin
//
// Two deliberate limits, both because the alternative is a matcher that is
// harder to predict than the thing it matches. A `*` that is not the last
// segment matches exactly one segment and captures nothing — `/*/users` is a
// wildcard *segment*, not a second catch-all. And a query string in a pattern
// is ignored rather than matched: `/search?q=uf` is the same pattern as
// `/search`, because a handler that silently stopped matching when a caller
// added a tracking parameter would be the worst kind of test failure.

/** Parameters captured from a path, by name. */
export type PathParams = { readonly [string]: string };

/** One piece of a compiled pattern, between two slashes. */
type Segment =
  | {| readonly kind: "literal", readonly value: string |}
  | {| readonly kind: "param", readonly name: string |}
  | {| readonly kind: "wildcard" |};

/**
 * A pattern, parsed once when the handler is declared.
 *
 * Compiled at declaration time rather than per request: a suite installs its
 * handlers once and then makes requests against them, so splitting the same
 * string on every `fetch` is work nobody asked for.
 */
export type PathPattern = {|
  /** The string the handler was written with, for a failure message. */
  readonly source: string,
  /** The origin the pattern pinned, or `null` when it matches any. */
  readonly origin: string | null,
  readonly segments: $ReadOnlyArray<Segment>,
|};

/** Whether `source` begins with a scheme, and so names an origin. */
function isAbsolute(source: string): boolean {
  return /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(source);
}

/** Split a path into its non-empty segments. */
function segmentsOf(path: string): Array<string> {
  return path.split("/").filter((segment) => segment !== "");
}

/**
 * Parse a handler's path into something that can be matched against a URL.
 *
 * A pattern with no scheme matches any origin, which is what makes a suite's
 * handlers survive a change of base URL. A pattern with one matches only that
 * origin, which is how a test says "the analytics host, not ours".
 */
export function compilePattern(source: string): PathPattern {
  // The pattern's own query and fragment are dropped here rather than at match
  // time, so `matchPattern` never has to know they were possible.
  const withoutQuery = source.split(/[?#]/)[0];

  let origin = null;
  let path = withoutQuery;
  if (isAbsolute(withoutQuery)) {
    const url = new URL(withoutQuery);
    origin = url.origin;
    path = url.pathname;
  }

  // The callback is annotated, not just the `const`: without it Flow widens
  // each `kind` to `string` and the literals stop matching the union.
  const segments = segmentsOf(path).map((segment): Segment => {
    if (segment === "*") {
      return { kind: "wildcard" };
    }
    if (segment.startsWith(":") && segment.length > 1) {
      return { kind: "param", name: segment.slice(1) };
    }
    return { kind: "literal", value: segment };
  });

  return { source, origin, segments };
}

/**
 * Percent-decode a captured segment, or hand back what was there.
 *
 * `decodeURIComponent` throws on a lone `%`, and a URL that a caller really did
 * send is not the place to raise: the handler should see the raw segment and
 * decide, rather than the matcher turning a strange request into a crash.
 */
function decode(value: string): string {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

/**
 * Match a compiled pattern against a URL, returning what it captured.
 *
 * `null` for a miss rather than an empty object, so a pattern with no
 * parameters is still distinguishable from one that did not match.
 *
 * A trailing slash is not significant on either side — `/users` and `/users/`
 * are the same path — because empty segments are dropped before the walk. That
 * is the behaviour a test wants: nobody means to write two handlers there.
 */
export function matchPattern(pattern: PathPattern, url: URL): PathParams | null {
  if (pattern.origin != null && pattern.origin !== url.origin) {
    return null;
  }

  const given = segmentsOf(url.pathname);
  const params: { [string]: string } = {};

  for (let index = 0; index < pattern.segments.length; index += 1) {
    const segment = pattern.segments[index];

    if (segment.kind === "wildcard" && index === pattern.segments.length - 1) {
      const rest = given.slice(index).join("/");
      params["*"] = decode(rest);
      // MSW's path syntax names an unnamed wildcard by its position, so a
      // handler ported from there reads `params[0]`. Both spellings are set:
      // one is what the pattern says, the other is what the ported code types.
      params["0"] = params["*"];
      return params;
    }

    if (index >= given.length) {
      return null;
    }

    if (segment.kind === "param") {
      params[segment.name] = decode(given[index]);
      continue;
    }

    if (segment.kind === "wildcard") {
      continue;
    }

    if (segment.value !== given[index]) {
      return null;
    }
  }

  return pattern.segments.length === given.length ? params : null;
}
