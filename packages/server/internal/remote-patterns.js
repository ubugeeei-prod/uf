// @flow
//
// Internal to `@uniflowed/server`: `app.builtins.images.remotePatterns`, read
// and matched.
//
// The allow-list in front of `/__uf/image`, and the only thing that decides
// which hosts the endpoint will fetch from. Next.js's `images.remotePatterns`
// is the model — the same four keys, the same wildcards — because a project
// moving across should be able to copy its list rather than translate it.
//
// # What a pattern means
//
// * `protocol` is `"https"` unless it says `"http"`. A pattern that says
//   nothing does not admit plain HTTP: an image fetched in the clear can be
//   replaced on the way, and the endpoint would re-encode the replacement and
//   serve it from the application's own origin.
// * `hostname` is exact, or `*.` for exactly one more label, or `**.` for any
//   number. Neither wildcard matches the name it is written before:
//   `*.example.com` is not `example.com`.
// * `port` is exact. A pattern that names none admits the protocol's default
//   port and no other, because a URL parser writes the default port as no
//   port at all.
// * `pathname` is split on `/`: a `*` segment is one segment, `**` is any
//   number of them including none, and every other segment is compared as
//   written. Against the URL's *parsed* path, so `..` and `%2e%2e` were
//   resolved before the comparison rather than after it.
//
// # Why this refuses rather than guesses
//
// A pattern this module cannot read the way it is written is a thrown error
// when the endpoint is built, not a pattern that matches nothing — or worse,
// one that matches more than its author meant. `uf_assets::check_remote_pattern`
// refuses the same spellings at the config file; this is the second half of
// that fact for a handler built by hand, and for a config a newer `uf` wrote.
// No regular expression is built from any of it: the matcher is a comparison
// of labels and of segments, and nothing in a request can make it backtrack.

/** One entry in the allow-list, as `uf.config.js` writes it. */
export type RemotePattern = {|
  readonly protocol?: "https" | "http",
  readonly hostname: string,
  readonly port?: string,
  readonly pathname?: string,
|};

/** A pattern, read once. */
export type CompiledPattern = {|
  readonly protocol: "https:" | "http:",
  readonly host: {| readonly kind: "exact" | "one" | "any", readonly name: string |},
  readonly port: string,
  readonly path: $ReadOnlyArray<string> | null,
|};

/**
 * Read every pattern, or throw naming the first that cannot be read.
 *
 * `mixed` in, because the list comes out of a configuration file and a
 * generated handler, and neither is checked by the type that describes it.
 */
export function compileRemotePatterns(patterns: mixed): $ReadOnlyArray<CompiledPattern> {
  if (!Array.isArray(patterns)) {
    throw new TypeError("uf: app.builtins.images.remotePatterns is not a list");
  }
  return patterns.map((pattern, index) => {
    try {
      return compilePattern(pattern);
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      throw new TypeError(`uf: app.builtins.images.remotePatterns[${index}] ${reason}`);
    }
  });
}

function compilePattern(pattern: mixed): CompiledPattern {
  if (pattern == null || typeof pattern !== "object") {
    throw new TypeError("is not an object");
  }
  const { protocol, hostname, port, pathname } = pattern;
  if (protocol != null && protocol !== "https" && protocol !== "http") {
    throw new TypeError(`has protocol ${quoted(protocol)}, not "https" or "http"`);
  }
  if (typeof hostname !== "string" || hostname === "") {
    throw new TypeError("has no hostname");
  }
  let kind: "exact" | "one" | "any" = "exact";
  let name = hostname;
  if (hostname.startsWith("**.")) {
    kind = "any";
    name = hostname.slice(3);
  } else if (hostname.startsWith("*.")) {
    kind = "one";
    name = hostname.slice(2);
  }
  if (name === "" || name.includes("*")) {
    throw new TypeError(
      `has hostname ${JSON.stringify(hostname)}, with a wildcard somewhere other than a ` +
        "leading `*.` or `**.`; an allow-list that admits every host is not an allow-list",
    );
  }
  if (!name.split(".").every(isLabel)) {
    throw new TypeError(
      `has hostname ${JSON.stringify(hostname)}, which is not a lowercase host name`,
    );
  }
  if (port != null && (typeof port !== "string" || !/^[0-9]+$/.test(port))) {
    throw new TypeError(`has port ${quoted(port)}, which is not a port number`);
  }
  let path = null;
  if (pathname != null) {
    if (typeof pathname !== "string" || !pathname.startsWith("/")) {
      throw new TypeError(`has pathname ${quoted(pathname)}, which does not start with /`);
    }
    path = pathname.slice(1).split("/");
    if (path.some((segment) => segment.includes("*") && segment !== "*" && segment !== "**")) {
      throw new TypeError(
        `has pathname ${JSON.stringify(pathname)}; a wildcard is a whole segment, \`*\` or \`**\``,
      );
    }
  }
  return {
    protocol: protocol === "http" ? "http:" : "https:",
    host: { kind, name },
    port: port ?? "",
    path,
  };
}

/** A value from the configuration, as it would be written there. */
function quoted(value: mixed): string {
  return JSON.stringify(value) ?? String(value);
}

/** A label of a host name as a URL parser writes one. */
function isLabel(label: string): boolean {
  if (label === "") return false;
  for (let index = 0; index < label.length; index += 1) {
    const code = label.charCodeAt(index);
    const lower = code >= 0x61 && code <= 0x7a;
    const digit = code >= 0x30 && code <= 0x39;
    if (!lower && !digit && code !== 0x2d) return false;
  }
  return true;
}

/** Whether `url` is one the list admits. */
export function admits(patterns: $ReadOnlyArray<CompiledPattern>, url: URL): boolean {
  // Credentials in a URL are never an image address. They are the classic way
  // to make a URL read as one host to a person and as another to a parser.
  if (url.username !== "" || url.password !== "") return false;
  return patterns.some((pattern) => matches(pattern, url));
}

function matches(pattern: CompiledPattern, url: URL): boolean {
  if (url.protocol !== pattern.protocol) return false;
  if (url.port !== pattern.port) return false;
  if (!matchesHost(pattern.host, url.hostname)) return false;
  return pattern.path == null || matchesPath(pattern.path, url.pathname.slice(1).split("/"));
}

function matchesHost(
  host: {| readonly kind: "exact" | "one" | "any", readonly name: string |},
  hostname: string,
): boolean {
  if (host.kind === "exact") return hostname === host.name;
  const suffix = `.${host.name}`;
  if (!hostname.endsWith(suffix)) return false;
  const prefix = hostname.slice(0, -suffix.length);
  if (prefix === "") return false;
  return host.kind === "any" || !prefix.includes(".");
}

/**
 * Whether `segments` match `pattern`, `**` taking any number of segments.
 *
 * Iterative, with the one backtracking point a single `**` needs remembered
 * rather than recursed into, so the work is bounded by the product of the two
 * lengths and a request cannot choose a deep stack.
 */
function matchesPath(pattern: $ReadOnlyArray<string>, segments: $ReadOnlyArray<string>): boolean {
  let p = 0;
  let s = 0;
  let starAt = -1;
  let resumeAt = 0;
  while (s < segments.length) {
    if (p < pattern.length && pattern[p] === "**") {
      starAt = p;
      resumeAt = s;
      p += 1;
    } else if (
      p < pattern.length &&
      (pattern[p] === segments[s] || (pattern[p] === "*" && segments[s] !== ""))
    ) {
      p += 1;
      s += 1;
    } else if (starAt !== -1) {
      p = starAt + 1;
      resumeAt += 1;
      s = resumeAt;
    } else {
      return false;
    }
  }
  while (p < pattern.length && pattern[p] === "**") p += 1;
  return p === pattern.length;
}
