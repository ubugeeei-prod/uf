// @flow
//
// Internal to `@uniflowed/server`: the parts of signing in that are not a flow.
//
// `../oauth.js` is the four handlers and the order they happen in. This is
// everything underneath them that has an answer of its own worth arguing for:
// where randomness comes from, how PKCE is computed, how two secrets are
// compared, what makes a cookie safe, which redirect targets are allowed, and
// what a store has to be able to do.
//
// They are here rather than there because a flow reads badly when every third
// line is a base64 loop, and because each of these is a place a mistake is
// somebody's account rather than somebody's afternoon. A reader auditing the
// flow should be able to read the flow.
//
// # Web Crypto, not `node:crypto`
//
// Every primitive below comes from the `crypto` global. `@uniflowed/server` is
// imported by `../edge.js` for runtimes that have no Node built-ins at all, and
// a module that reached for `node:crypto` would be a module that cannot be
// bundled for a worker — which would make signing in the one thing a uf
// application cannot do on half the hosts uf targets.

/** The pieces of a session or a half-finished authorization, as stored. */
export type StoredValue = { +[string]: mixed };

/**
 * Where uf keeps what it may not put in a cookie.
 *
 * Four methods, one value type, and uf owns what goes in — so implementing this
 * against Redis, a table, or a worker's KV is a few lines that know nothing
 * about OAuth. `expiresAt` is milliseconds since the epoch and is passed
 * separately rather than left inside the value, because a store that can expire
 * entries itself should be told when to, and one that cannot still has to know
 * when to answer `null`.
 *
 * # `take` is not `read` then `destroy`
 *
 * It is the one method that could not be composed from the others, and it is
 * the whole reason a `state` parameter is single-use. Two callbacks carrying
 * the same `state` arriving at once must not both find the record: whichever
 * store this is has to make the read and the removal one step, so the second
 * one gets `null` and is refused. Written as two calls there is a window
 * between them, and a window is all a replay needs.
 *
 * A `Map` closes it by being single-threaded, which is what
 * [`memorySessionStore`] relies on and says so. A durable store closes it with
 * whatever it has — `GETDEL`, a delete that returns the row, a transaction —
 * and an implementation that cannot is an implementation that must not be used
 * for this.
 *
 * # Everything is async
 *
 * Including on the in-memory store, where nothing needs to be. A contract whose
 * default implementation is synchronous is a contract every caller quietly
 * starts assuming is synchronous, and the first durable store to be dropped in
 * behind it finds a dozen places that never awaited.
 */
export type SessionStore = {|
  /** The value under `key`, or `null` when there is none or it has expired. */
  readonly read: (key: string) => Promise<StoredValue | null>,
  /** Put `value` under `key` until `expiresAt`, replacing whatever was there. */
  readonly write: (key: string, value: StoredValue, expiresAt: number) => Promise<void>,
  /** The value under `key`, removed in the same step; see above. */
  readonly take: (key: string) => Promise<StoredValue | null>,
  /** Remove `key`, whether or not it was there. */
  readonly destroy: (key: string) => Promise<void>,
|};

/**
 * How many entries the in-memory store holds before it starts dropping some.
 *
 * `docs/security.md` rule 4: no unbounded anything. An unbounded map here is a
 * memory exhaustion that anybody can reach — every hit on the authorize handler
 * writes a record, and nothing makes the browser come back to spend it.
 */
const MEMORY_STORE_CAPACITY = 10000;

/**
 * A store in one process's memory.
 *
 * The implementation uf ships, and the honest description of it is that it is
 * for one process: four instances behind a load balancer hold four different
 * sets of sessions, a restart signs everybody out, and a half-finished sign-in
 * that lands on a different instance is refused as a replay. That is not a
 * defect to be configured around — it is what "in memory" means — and it is
 * spelled out because a default that quietly does not work in production is
 * worse than no default at all.
 *
 * # What it does when it is full
 *
 * Expired entries first, and then the oldest surviving one. Both choices are
 * uncomfortable and the second is the lesser: refusing the write instead would
 * mean an attacker who fills the map can stop everybody else signing in, which
 * is a denial of service reachable by anyone, whereas evicting the oldest costs
 * a session that has to be established again. Neither is a reason to run this
 * in production, which is the actual answer.
 */
export function memorySessionStore(options?: {| readonly capacity?: number |}): SessionStore {
  const capacity = options?.capacity ?? MEMORY_STORE_CAPACITY;
  const entries: Map<string, {| value: StoredValue, expiresAt: number |}> = new Map();

  /** The entry under `key` if it is still good, having dropped it if it is not. */
  const live = (key: string) => {
    const found = entries.get(key);
    if (found == null) {
      return null;
    }
    if (found.expiresAt <= Date.now()) {
      entries.delete(key);
      return null;
    }
    return found.value;
  };

  return {
    read: async (key) => live(key),
    take: async (key) => {
      // One step in the sense that matters: nothing runs between these two
      // lines, because JavaScript on one thread cannot interleave them. A
      // durable store has to earn this with an operation of its own.
      const value = live(key);
      entries.delete(key);
      return value;
    },
    write: async (key, value, expiresAt) => {
      entries.set(key, { value, expiresAt });
      if (entries.size <= capacity) {
        return;
      }
      const now = Date.now();
      for (const [name, entry] of entries) {
        if (entry.expiresAt <= now) {
          entries.delete(name);
        }
      }
      // `Map` iterates in insertion order, so the first key is the oldest
      // write — and `set` above re-inserts, so an entry that was refreshed is
      // treated as new rather than as old.
      while (entries.size > capacity) {
        const oldest = entries.keys().next();
        if (oldest.done === true) break;
        entries.delete(oldest.value);
      }
    },
    destroy: async (key) => {
      entries.delete(key);
    },
  };
}

/**
 * `count` bytes of cryptographic randomness, as base64url text.
 *
 * `crypto.getRandomValues` and not `Math.random`, which is the difference
 * between a `state` parameter that cannot be guessed and one that can be
 * predicted from a handful of earlier ones. 32 bytes is 256 bits, which is what
 * every caller here asks for and more than the OAuth security guidance
 * requires.
 *
 * base64url rather than hex so it fits a cookie and a query string without
 * encoding, and rather than base64 because `+`, `/` and `=` all mean something
 * in one of those two places.
 */
export function randomToken(count: number): string {
  return base64url(crypto.getRandomValues(new Uint8Array(count)));
}

/**
 * The PKCE code challenge for `verifier`: base64url of its SHA-256.
 *
 * S256 and never `plain`. A `plain` challenge is the verifier itself, so an
 * attacker who can read the authorization request — a browser extension, a
 * proxy, a log of the URL — has everything needed to spend the code they
 * intercepted, and the whole extension is decoration. RFC 7636 allows `plain`
 * for clients that cannot compute a digest; every runtime this package runs on
 * can, so uf never offers it and there is no option to turn it off.
 */
export async function pkceChallenge(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  return base64url(new Uint8Array(digest));
}

/** Bytes as base64url text, with the padding gone. */
function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

/**
 * Whether two secrets are the same, without saying how far they agreed.
 *
 * `a === b` on strings stops at the first difference, and the time it took is a
 * measurement of how much of the secret the caller got right — enough, over
 * many attempts, to find the rest a character at a time. Both values compared
 * here are ours and the same length, so the early length test leaks nothing
 * that was not already public.
 *
 * A JavaScript engine is free to optimise this and there is no portable way to
 * stop it; `node:crypto`'s `timingSafeEqual` is not reachable from a worker.
 * What this buys is that the *algorithm* is not the leak, which is the half
 * that is in our hands.
 */
export function constantTimeEquals(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  let difference = 0;
  for (let at = 0; at < a.length; at += 1) {
    difference |= a.charCodeAt(at) ^ b.charCodeAt(at);
  }
  return difference === 0;
}

/**
 * Whether this request was made from the site it is addressed to.
 *
 * The rule `docs/security.md` states and this is the only implementation of it:
 * **`Origin` is compared against `Host`, and never against `X-Forwarded-Host`.**
 * `new URL(request.url).host` is the `Host` header in every host uf ships —
 * `../node.js`'s `toRequest` builds the URL from it, and a worker's `Request`
 * carries the real one — and no forwarded header is read here or anywhere near
 * here. A forwarded header is a header: something a client can send, and
 * therefore something that cannot decide whether a client is allowed to do
 * what it is asking to do. Trusting one is how a Next.js deployment behind a
 * proxy came to be steerable by whoever could set it.
 *
 * A missing `Origin` is refused rather than allowed. Every browser sends it on
 * a `POST` or a `DELETE`, so a request without one is not a browser — and a
 * non-browser client authenticating with a cookie is a request that should not
 * be answered anyway, because a cookie is exactly the credential the browser
 * attaches whether or not the caller meant it to.
 *
 * A deployment whose proxy rewrites `Host` will see these refused, and the fix
 * is the proxy: preserving `Host` is something every proxy can do, and the
 * alternative is uf believing a header instead.
 */
export function sameOrigin(request: Request): boolean {
  const origin = request.headers.get("origin");
  if (origin == null || origin === "" || origin === "null") {
    return false;
  }
  let sent: URL;
  try {
    sent = new URL(origin);
  } catch {
    return false;
  }
  return sent.host === new URL(request.url).host;
}

/** The longest path uf will send a browser back to after signing in. */
const MAX_RETURN_PATH = 512;

/**
 * `value` if it is a path on this site, and `fallback` if it is anything else.
 *
 * The open-redirect guard, and the reason it is a whitelist of one shape rather
 * than a list of hosts: a sign-in flow that sends the browser wherever a query
 * parameter says is the classic phishing amplifier — the link is genuinely on
 * your domain, the sign-in is genuinely yours, and the page the user lands on
 * afterwards is not.
 *
 * What is refused, and why each spelling is here rather than implied by the one
 * above it:
 *
 * * anything not starting with `/`, which covers `https://evil.example` and
 *   `javascript:` alike;
 * * `//evil.example`, which a browser reads as a protocol-relative *absolute*
 *   URL and every naive `startsWith("/")` check lets through;
 * * `/\evil.example`, which browsers accept as the same thing, because they
 *   normalise a backslash to a slash in the authority position;
 * * anything holding a byte below `!` or a `DEL` — a newline in a `Location`
 *   header is a response-splitting attack, and a space is simply not a URL.
 *
 * The result is used verbatim in a `Location` header. It is a path, so it is
 * resolved against the site the browser is already on, and there is no
 * arrangement of the characters that survive this check that names another one.
 */
export function localPath(value: string | null, fallback: string): string {
  if (value == null || value.length === 0 || value.length > MAX_RETURN_PATH) {
    return fallback;
  }
  if (value.charCodeAt(0) !== 0x2f) {
    return fallback;
  }
  const second = value.charCodeAt(1);
  if (second === 0x2f || second === 0x5c) {
    return fallback;
  }
  for (let at = 0; at < value.length; at += 1) {
    const code = value.charCodeAt(at);
    if (code <= 0x20 || code === 0x7f) {
      return fallback;
    }
  }
  return value;
}

/** What a cookie uf sets says about itself. */
export type CookieAttributes = {|
  /** Seconds. `0` expires it now, which is how uf clears one. */
  readonly maxAge: number,
  readonly secure: boolean,
|};

/**
 * One `Set-Cookie` header value.
 *
 * The attributes are not options, because each of them is load-bearing and a
 * project that turned one off would be a project with a different vulnerability
 * rather than a different preference:
 *
 * * **`HttpOnly`** — script cannot read it, so one XSS is not every session.
 * * **`Path=/`** — the session is the site's, not a directory's, and `__Host-`
 *   requires it.
 * * **`SameSite=Lax`, and deliberately not `Strict`.** The provider redirects
 *   the browser back to the callback, which is a cross-site top-level
 *   navigation; `Strict` withholds the cookie on exactly that navigation, so
 *   the pending record could never be found and no sign-in would ever complete.
 *   `Lax` sends it on a top-level `GET` and withholds it from the cross-site
 *   `POST` that a CSRF needs, which is the shape of this flow precisely.
 * * **`Secure`** wherever the request is `https`, together with the `__Host-`
 *   name prefix. The prefix is the part worth explaining: it makes the cookie
 *   host-only, so `evil.example.com` cannot set a cookie that `example.com`
 *   will read. Without it a subdomain — a legacy app, a customer's page on a
 *   shared domain, a stale CNAME somebody else now controls — can plant a
 *   pending authorization in the victim's browser and sign them into an account
 *   that is not theirs.
 */
export function cookieHeader(name: string, value: string, attributes: CookieAttributes): string {
  const parts = [`${name}=${value}`, "Path=/", "HttpOnly", "SameSite=Lax"];
  parts.push(`Max-Age=${String(attributes.maxAge)}`);
  if (attributes.secure) {
    parts.push("Secure");
  }
  return parts.join("; ");
}

/**
 * The cookie name to use over this scheme.
 *
 * `__Host-` only where the connection is secure, because a browser ignores a
 * `__Host-` cookie that is not `Secure` and `Secure` over plain HTTP is not
 * sent at all — so applying the prefix everywhere would make sign-in silently
 * impossible under `uf dev` on `http://localhost`. The name a request looks for
 * is derived the same way from the same request, so the two halves cannot
 * disagree.
 */
export function cookieNameFor(base: string, secure: boolean): string {
  return secure ? `__Host-${base}` : base;
}

/**
 * The most a token endpoint's answer may be before uf stops reading it.
 *
 * A token response is a few hundred bytes. The endpoint is named by the
 * project's own configuration rather than by a request, so it is not hostile in
 * the way a request is — but `docs/security.md` rule 1 says a remote response is
 * untrusted input regardless of who chose the URL, and rule 4 says nothing is
 * unbounded. 64 KiB is four hundred times a real answer and a thousandth of
 * what a compromised endpoint could send.
 */
const MAX_TOKEN_RESPONSE = 64 * 1024;

/**
 * A response body as parsed JSON, refusing to read more than uf will accept.
 *
 * `response.json()` would read the whole body first and then parse it, which is
 * the unbounded read this exists to avoid. The stream is read chunk by chunk
 * and abandoned the moment the total passes the ceiling — `cancel` rather than
 * a `break`, so the connection is released instead of being left for the
 * garbage collector.
 *
 * Nothing that comes out of here is ever logged. It is the one object in this
 * package that certainly contains a credential.
 */
export async function readBoundedJson(response: Response): Promise<mixed> {
  const body = response.body;
  if (body == null) {
    return null;
  }
  const reader = body.getReader();
  const chunks: Array<Uint8Array> = [];
  let total = 0;
  for (;;) {
    const step = await reader.read();
    if (step.done === true) break;
    const chunk = step.value;
    if (chunk == null) continue;
    total += chunk.byteLength;
    if (total > MAX_TOKEN_RESPONSE) {
      await reader.cancel();
      throw new Error("the token endpoint answered with more than uf will read");
    }
    chunks.push(chunk);
  }
  const joined = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    joined.set(chunk, at);
    at += chunk.byteLength;
  }
  return JSON.parse(new TextDecoder().decode(joined));
}

/**
 * `url`, if it is somewhere uf is willing to send a client secret.
 *
 * `https`, or `http` on a loopback name. The exception is not a convenience: an
 * OAuth provider running on `localhost` is how every one of these is developed
 * against, and a rule with no exception is a rule that gets turned off. A
 * loopback name is not on a network, so there is nothing between the two ends
 * to intercept.
 *
 * Anything else — `http://` to a real host, a `file:` URL, a scheme nobody has
 * heard of — is a typed error at configuration time rather than a token sent in
 * the clear at request time.
 */
export function requireSecureEndpoint(url: string, what: string): URL {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    throw new Error(`@uniflowed/server: the provider's ${what} is not a URL: ${url}`);
  }
  const loopback =
    parsed.hostname === "localhost" ||
    parsed.hostname === "127.0.0.1" ||
    parsed.hostname === "[::1]";
  if (parsed.protocol === "https:" || (parsed.protocol === "http:" && loopback)) {
    return parsed;
  }
  throw new Error(
    `@uniflowed/server: the provider's ${what} must be https (or http on loopback, for ` +
      `development), and it is ${parsed.protocol}//${parsed.host}`,
  );
}
