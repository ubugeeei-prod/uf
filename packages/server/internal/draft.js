// @flow
//
// Internal to `@uniflowed/server`: the cookie that makes draft mode a feature.
//
// `draftMode()` was a flag on the request context that nothing wrote to a
// response and nothing read from one, so `enable()` mutated an object that was
// discarded when the response was sent and `isEnabled` was `false` in every
// request that had not called `enable()` earlier in its own stack — which is
// every request. See ubugeeei-prod/uf#282. Persistence is the whole feature: a
// CMS previews unpublished content by linking to a route handler that enables
// draft mode and redirects, and the reader then browses the live site seeing
// drafts.
//
// # The cookie is an authorization token, so it is signed
//
// Whoever holds it sees unpublished content and bypasses the route cache. A
// cookie whose value is `1` would be a feature anybody can turn on for
// themselves, which is worse than the feature being absent — an operator who
// believes drafts are gated would be wrong and would have no way to find out.
//
// The construction is the one `crates/uf_rsc/src/action.rs` already argues for
// under a different name: HMAC-SHA256 over a length-prefixed,
// domain-separated message, compared in constant time.
//
//     value   = <expiry> "." base64url(mac)
//     mac     = HMAC-SHA256(key, "uf-draft-v1" || u32be(len(expiry)) || expiry)
//     expiry  = decimal epoch milliseconds, as ASCII
//
// The expiry is in the clear because it is not a secret — it is *inside* the
// signature so that a holder cannot extend it, which is the only property that
// matters. The length prefix and the domain string are there for the reason
// `action.rs` gives: no two different messages may produce the same signed
// bytes, and a digest computed here must not be mistakable for one computed
// anywhere else with the same key.
//
// # The key, and what happens when a deployment does not supply one
//
// `UF_DRAFT_SECRET`, at least 32 bytes, is the deployment's key. It is the only
// way a draft cookie survives a restart or is accepted by a second instance,
// which is what any deployment with more than one process needs. A secret
// shorter than 32 bytes is refused rather than accepted and stretched: a key
// that is a word is not a key, and silently accepting one would mean an
// application whose drafts are gated by something guessable.
//
// Without it, uf generates 32 bytes per process. That is deliberately not
// "draft mode does not work" and deliberately not "draft mode is unsigned": a
// single-process `uf dev` or `uf start` gets the whole feature with no setup,
// and what it costs is stated in a `warn` line the first time a cookie is
// issued — the cookies stop working when the process restarts, and a second
// instance behind a load balancer never accepts them.
//
// # There is no per-request name
//
// One name, `__Host-uf.draft`, always `Secure`. `internal/oauth.js`'s
// `cookieNameFor` derives a name from the scheme and this deliberately does
// not, for the reason #585 flagged about the session cookie: two possible names
// mean a reader that has to try both, and a reader that falls back to an
// unprefixed name can be handed one a subdomain planted. Browsers treat
// `http://localhost` as a secure context and accept a `Secure` cookie there, so
// one name costs `uf dev` nothing.

import { Temporal } from "@uniflowed/core/temporal";
import { constantTimeEquals, cookieHeader } from "./oauth.js";

/**
 * The cookie name, fixed for every deployment and every scheme.
 *
 * `__Host-` makes it host-only: `evil.example.com` cannot set a cookie that
 * `example.com` reads, so a subdomain cannot turn draft mode on in somebody
 * else's browser. The prefix requires `Secure`, `Path=/` and no `Domain`, all
 * of which `cookieHeader` writes.
 */
export const DRAFT_COOKIE: string = "__Host-uf.draft";

/**
 * How long a draft cookie is good for, in seconds.
 *
 * An hour. The cookie is an authorization token, so it has to expire, and the
 * cost of it expiring is that the editor clicks the CMS's preview link again —
 * one redirect. The alternative, a session cookie with no expiry, is a token
 * that lives as long as the browser profile does and is worth stealing for
 * that long.
 *
 * Both halves are written: `Max-Age` so a browser drops it, and the signed
 * expiry so a browser that does not is refused anyway. A client controls
 * whether it sends a cookie; it does not control whether uf accepts one.
 */
export const DRAFT_MAX_AGE_SECONDS: number = 60 * 60;

/** Shortest accepted `UF_DRAFT_SECRET`, in bytes. */
export const MIN_DRAFT_SECRET_BYTES: number = 32;

/** Domain separation, so this digest is not mistakable for an action id's. */
const DRAFT_DOMAIN = "uf-draft-v1";

/** The environment variable a deployment sets to key the cookie. */
const SECRET_VARIABLE = "UF_DRAFT_SECRET";

/**
 * Raised when `enable()` or `disable()` is called where no response is owned.
 *
 * Named rather than generic because the fix is a move, not an edit: the call
 * belongs in a route handler or a server action, and the message says so. The
 * same refusal Next makes, for the same reason — a render has no defined moment
 * at which a response header takes effect, because the headers may already be
 * on the wire by the time a component six levels down runs.
 */
export class DraftModeError extends Error {
  /** `enable` or `disable`. */
  operation: string;

  constructor(operation: string, reason: string) {
    super(
      `@uniflowed/server: draftMode().${operation}() ${reason}. ` +
        "It writes a cookie, so it has to be called where a response is being produced: " +
        'a route handler (`_uf.route.js`) or a `"use server"` action. A middleware that ' +
        "wants to turn draft mode on should answer with a redirect to one of those.",
    );
    this.name = "DraftModeError";
    this.operation = operation;
  }
}

/** What a request decided about draft mode, for a responder to write. */
export type DraftChange = "enable" | "disable";

/**
 * Whether a `Cookie` header names the draft cookie at all.
 *
 * A *name* scan and not a value check, and the two callers are the reason it is
 * separate from verification. `../node.js`'s static handler and
 * `../standalone.js`'s embedded lookup both answer a request from bytes on disk
 * before any application code runs, and both must not hand a draft request a
 * prerendered document — but neither can reach the request context, because the
 * context lives in the *application bundle's* copy of `./context.js` and those
 * two modules are the host's copy. What they can read is the request's own
 * header, which needs no key and no storage.
 *
 * What a forged cookie buys is therefore a live render of a page that would
 * otherwise have come off disk, and nothing else: whether draft mode is
 * actually *on* is decided by [`verifyDraftCookie`] below, under the key. That
 * is a bypass of a cache rather than of a gate, and it is bounded by the fact
 * that requesting any path with no prerendered document already forces a render
 * — so the amplification is one request in, one render out, which is what an
 * unprerendered route costs anyway.
 *
 * Written out rather than calling `parseCookies`: this module must not import
 * `./context.js`, which imports it.
 */
export function carriesDraftCookie(header: string | null): boolean {
  if (header == null || header === "") {
    return false;
  }
  for (const pair of header.split(";")) {
    const at = pair.indexOf("=");
    if (at > 0 && pair.slice(0, at).trim() === DRAFT_COOKIE) {
      return true;
    }
  }
  return false;
}

/**
 * Whether a prerendered document may answer this request.
 *
 * **This is the reason, and it is written here once.** Four front doors answer
 * a request from bytes that were written before it arrived — `uf start`'s
 * static handler in `../node.js`, the compiled binary's embedded index in
 * `../standalone.js`, the worker's assets binding in `../edge.js`, and
 * `uf preview`, where the file server is Vite's own and runs in front of
 * everything uf mounts. Each of them asks this, and none of them argues it
 * again. `../lambda.js` is not a fifth: its `staticDir` is served through
 * `../node.js`'s `createStaticHandler`, so it is the first door under another
 * name.
 *
 * A file in `dist/` is what the site said *before* the draft existed. Handing
 * one to an editor who came to look at the draft answers a different question
 * from the one they asked, and it would make draft mode a feature that works
 * everywhere except on the pages a build was able to prerender — which are
 * exactly the pages a CMS produces. So a request that carries the draft cookie
 * is rendered, on every door, or draft mode is a property of which command
 * somebody happened to run.
 *
 * **Documents only.** A stylesheet and a chunk are the same bytes in draft mode
 * as out of it, and skipping those would leave the page unstyled and
 * unhydrated for no gain. Deciding *which* bytes are a document is each door's
 * own — a file extension here, an embedded key there, a `content-type` at the
 * edge — because that is the one part of the question that depends on where
 * the bytes are kept.
 *
 * The cookie's **name** decides it and not its signature, for the reason in
 * [`carriesDraftCookie`]: these doors run before any application code and
 * cannot reach the request context where the verified answer lives. What a
 * forged cookie buys is a live render of a page that is public anyway.
 *
 * See ubugeeei-prod/uf#282, #615 and #620.
 */
export function prerenderedMayAnswer(cookieHeader: string | null): boolean {
  return !carriesDraftCookie(cookieHeader);
}

/**
 * Whether `value` is a draft cookie this deployment issued and still honours.
 *
 * `false` for every way of not being one — absent, the wrong shape, a signature
 * that does not verify, an expiry that has passed — and deliberately without
 * saying which. A caller has nothing to do differently with the distinction and
 * an attacker would have something to learn from it.
 */
export async function verifyDraftCookie(value: string | null): Promise<boolean> {
  if (value == null) {
    return false;
  }
  const at = value.lastIndexOf(".");
  if (at <= 0 || at === value.length - 1) {
    return false;
  }
  const expiry = value.slice(0, at);
  // Bounded and shaped before anything is hashed: the expiry is a decimal
  // number of milliseconds, so thirteen digits today and never more than
  // twenty, and a megabyte of digits is not a near miss. `docs/security.md`
  // rule 4 for the ceiling, and rule 5 for the scan being a loop rather than
  // the obvious `/^[0-9]+$/` — a pattern that cannot backtrack is still a
  // pattern applied to untrusted input, and the rule is about the class.
  if (!isDigits(expiry)) {
    return false;
  }
  let expected: string;
  try {
    expected = await signExpiry(expiry);
  } catch {
    // The deployment's secret was refused; see [`secretBytes`]. Nothing a
    // client holds can then be a cookie this deployment issued, so the honest
    // answer here is no — and the failure is raised where somebody can act on
    // it, at the moment `enable()` tries to issue one. Throwing here instead
    // would fail every request that arrives carrying a stale cookie, which is
    // a misconfiguration turning into an outage.
    return false;
  }
  if (!constantTimeEquals(value.slice(at + 1), expected)) {
    return false;
  }
  return Number(expiry) > nowMillis();
}

/** Longest expiry uf will look at: twenty digits is past any clock it has. */
const MAX_EXPIRY_DIGITS = 20;

/** Whether `value` is one to twenty ASCII digits and nothing else. */
function isDigits(value: string): boolean {
  if (value.length === 0 || value.length > MAX_EXPIRY_DIGITS) {
    return false;
  }
  for (let at = 0; at < value.length; at += 1) {
    const code = value.charCodeAt(at);
    if (code < 0x30 || code > 0x39) {
      return false;
    }
  }
  return true;
}

/**
 * Now, in epoch milliseconds.
 *
 * uf's clock rather than `Date.now()`, so a test that installs one moves the
 * expiry with it — the seam `@uniflowed/core/temporal` exists to be, and the
 * same call `../oauth.js` makes for a token's lifetime. Milliseconds and not an
 * `Instant` because the value goes into a cookie, and a cookie is a wire: an
 * epoch millisecond count is a primitive that serializes as itself.
 */
function nowMillis(): number {
  return Temporal.Now.instant().epochMilliseconds;
}

/**
 * The `Set-Cookie` value that turns draft mode on, or off.
 *
 * Turning it off is the same cookie with `Max-Age=0` and an empty value, which
 * is how a cookie is deleted: the browser has to be told to drop the one it
 * holds, and answering without a `Set-Cookie` would leave it in place.
 */
export async function draftSetCookie(change: DraftChange): Promise<string> {
  if (change === "disable") {
    return cookieHeader(DRAFT_COOKIE, "", { maxAge: 0, secure: true });
  }
  const expiry = String(
    Temporal.Now.instant().add({ seconds: DRAFT_MAX_AGE_SECONDS }).epochMilliseconds,
  );
  return cookieHeader(DRAFT_COOKIE, `${expiry}.${await signExpiry(expiry)}`, {
    maxAge: DRAFT_MAX_AGE_SECONDS,
    secure: true,
  });
}

/** The signature over one expiry, as base64url text. */
async function signExpiry(expiry: string): Promise<string> {
  const bytes = new TextEncoder().encode(expiry);
  const message = new Uint8Array(DRAFT_DOMAIN.length + 4 + bytes.length);
  message.set(new TextEncoder().encode(DRAFT_DOMAIN), 0);
  new DataView(message.buffer).setUint32(DRAFT_DOMAIN.length, bytes.length, false);
  message.set(bytes, DRAFT_DOMAIN.length + 4);
  const mac = await crypto.subtle.sign("HMAC", await signingKey(), message);
  return base64url(new Uint8Array(mac));
}

/** Bytes as base64url text, with the padding gone. */
function base64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

/** The imported key, and the secret it was imported from. */
let imported: {| readonly secret: string | null, readonly key: Promise<CryptoKey> |} | null = null;

/**
 * The HMAC key, imported once per secret.
 *
 * Cached because importing is the expensive half and two requests arriving
 * together should import it once between them rather than once each — and
 * keyed on the secret rather than held forever, because "the key is whatever
 * `UF_DRAFT_SECRET` says" is the honest contract: a deployment that rotates it
 * has rotated it, and every cookie signed with the old one stops being
 * accepted, which is what rotating a signing key means.
 *
 * A secret that is refused throws out of `secretBytes` before the assignment,
 * so a rejected import is never what the cache holds.
 */
function signingKey(): Promise<CryptoKey> {
  const secret = environmentSecret();
  if (imported == null || imported.secret !== secret) {
    imported = {
      secret,
      key: crypto.subtle.importKey("raw", secretBytes(), { name: "HMAC", hash: "SHA-256" }, false, [
        "sign",
      ]),
    };
  }
  return imported.key;
}

/** The generated key, when the deployment supplied none. */
let generated: Uint8Array | null = null;

/**
 * The key bytes: the deployment's, or this process's own.
 *
 * The refusal is deliberate and it is not a validation formality. A secret
 * shorter than [`MIN_DRAFT_SECRET_BYTES`] is a word somebody typed, and an
 * application whose unpublished content is gated by a word is one whose
 * unpublished content is not gated. Refusing here means the failure appears the
 * first time somebody enables draft mode, with the variable named, rather than
 * never.
 */
function secretBytes(): Uint8Array {
  const supplied = environmentSecret();
  if (supplied != null) {
    const bytes = new TextEncoder().encode(supplied);
    if (bytes.length < MIN_DRAFT_SECRET_BYTES) {
      throw new Error(
        `@uniflowed/server: ${SECRET_VARIABLE} is ${String(bytes.length)} bytes and must be at ` +
          `least ${String(MIN_DRAFT_SECRET_BYTES)}. It is the key that signs the draft-mode ` +
          "cookie, and a short one is not a key.",
      );
    }
    return bytes;
  }
  generated ??= crypto.getRandomValues(new Uint8Array(MIN_DRAFT_SECRET_BYTES));
  return generated;
}

/**
 * Whether the key is this process's own rather than the deployment's.
 *
 * Read by `./context.js` so that the warning is written when a cookie is
 * *issued* rather than when this module is loaded: a deployment that never uses
 * draft mode has nothing to be told.
 */
export function draftKeyIsPerProcess(): boolean {
  return environmentSecret() == null;
}

/**
 * `UF_DRAFT_SECRET`, or `null`.
 *
 * Read through `globalThis` rather than as a bare `process`, because this
 * module is bundled for worker runtimes by `../edge.js` and some of them have
 * no `process` at all — the same walk, and the same reason, as `./log.js`'s
 * `environment`, which is not exported and is one variable rather than this
 * one. An empty value is `null`: a variable set to nothing is how a shell says
 * "unset" more often than it is a deliberate empty key.
 */
function environmentSecret(): string | null {
  const runtime = globalThis.process;
  if (runtime == null || typeof runtime !== "object") {
    return null;
  }
  const environment = runtime.env;
  if (environment == null || typeof environment !== "object") {
    return null;
  }
  const value = environment[SECRET_VARIABLE];
  return typeof value === "string" && value !== "" ? value : null;
}
