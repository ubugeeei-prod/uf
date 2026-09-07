// @flow
//
// `@uniflowed/server/oauth`: signing in, as a contract rather than a provider.
//
// uf does not ship a Google client, and will not. `docs/red-lines.md` puts it
// as "keep provider SDKs and proprietary services out of portable application
// and framework contracts", and the reason is the one the whole document is
// about: a toolchain that integrates with a company becomes the place every
// change to that company's API has to be released through. What is portable
// about signing in is the *flow* — a redirect, a state parameter, a token
// exchange, a session — and the flow is the same whoever is at the other end.
// What is not portable is one function: which URLs to talk to and how to turn
// the tokens into a person. That is the seam, and it is deliberately small
// enough to write from the provider's own documentation without a package in
// between.
//
// # The whole of a provider
//
//     // app/providers/example.js
//     // @flow
//     import type { OAuthProvider } from "@uniflowed/server/oauth";
//
//     export const example: OAuthProvider = {
//       authorizationEndpoint: "https://example.com/oauth/authorize",
//       tokenEndpoint: "https://example.com/oauth/token",
//       clientId: process.env.EXAMPLE_CLIENT_ID ?? "",
//       clientSecret: process.env.EXAMPLE_CLIENT_SECRET,
//       scope: "openid email",
//       async identify(tokens) {
//         const answer = await fetch("https://example.com/userinfo", {
//           headers: { authorization: `Bearer ${tokens.accessToken}` },
//         });
//         if (!answer.ok) {
//           throw new Error(`userinfo answered ${answer.status}`);
//         }
//         const user = await answer.json();
//         return { subject: String(user.id), claims: { email: user.email } };
//       },
//     };
//
// Twenty lines, no dependency, and nothing in it that uf had to anticipate.
//
// # And the whole of using one
//
//     // app/auth.js
//     import { createAuth, memorySessionStore } from "@uniflowed/server/oauth";
//     import { example } from "./providers/example.js";
//
//     export const auth = createAuth({
//       provider: example,
//       store: memorySessionStore(),
//       callbackPath: "/auth/callback",
//     });
//
//     // app/auth/authorize/_uf.route.js
//     export const GET = auth.authorize;
//
//     // app/auth/callback/_uf.route.js
//     export const GET = auth.callback;
//
//     // app/auth/refresh/_uf.route.js
//     export const POST = auth.refresh;
//
//     // app/auth/session/_uf.route.js
//     export const GET = auth.session;
//     export const DELETE = auth.session;
//
// and, in a loader or a server component, `await auth.currentSession()`.
//
// # What uf owns
//
// Everything a provider has no opinion about, which turns out to be everything
// that goes wrong:
//
// * **The `state` parameter.** 256 bits from `crypto.getRandomValues`, kept
//   server-side, compared in constant time, and *taken* rather than read — one
//   store operation that removes it, so the second callback carrying the same
//   value finds nothing and is refused. See `SessionStore.take`.
// * **Binding the flow to the browser that started it.** The `state` alone
//   proves the callback came from the provider; it does not prove it came back
//   to the same person. The pending record is found through an `HttpOnly`,
//   `SameSite=Lax`, `__Host-`-prefixed cookie, so a callback replayed in
//   somebody else's browser has no record to find.
// * **PKCE, always.** S256, no configuration, no `plain`. An intercepted
//   authorization code is worth nothing without the verifier, which never
//   leaves this process.
// * **The redirect back.** Only a path on this site, checked against every
//   spelling a browser treats as absolute.
// * **The session.** An opaque 256-bit id in a cookie, the record behind it in
//   a store, and a new id every time somebody signs in.
// * **Where the tokens are not.** They are in the store. They are not in the
//   session an application reads, not in any response body, and not in any log
//   line. `tokens()` exists for the application that genuinely needs to call
//   the provider's API, and it is named so that using it is a decision.
//
// # What is deliberately not here
//
// **No provider registry, and no `providers/google.js`.** One would be four
// URLs and a `identify`, which is the example above; a directory of them is a
// list uf has to keep current, and red line 5's chokepoint in miniature.
//
// **No OpenID Connect `id_token` verification.** Verifying one means fetching a
// JWKS, caching it, choosing which algorithms are acceptable and refusing
// `alg: none` — a real amount of security-critical code, and a lie if it were
// half done. `identify` receives the raw `idToken` and a provider that wants to
// use it can; what uf must not do is decode one without checking the signature
// and call the result an identity.
//
// **No durable store.** `memorySessionStore` is one process's memory and says
// so at every opportunity. The interface is the deliverable; an implementation
// against a database is an adapter's, and it is four methods.

import type { Instant } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";
import { cookies, logger } from "./index.js";
import { parseCookies } from "./internal/context.js";
import type { CookieAttributes, SessionStore, StoredValue } from "./internal/oauth.js";
import {
  constantTimeEquals,
  cookieHeader,
  cookieNameFor,
  localPath,
  memorySessionStore,
  pkceChallenge,
  randomToken,
  readBoundedJson,
  requireSecureEndpoint,
  sameOrigin,
} from "./internal/oauth.js";

export type { CookieAttributes, SessionStore, StoredValue } from "./internal/oauth.js";
export { memorySessionStore, sameOrigin } from "./internal/oauth.js";

/** What a token endpoint answered with, in uf's spelling rather than OAuth's. */
export type TokenSet = {|
  readonly accessToken: string,
  readonly tokenType: string,
  /** Absent for a provider that does not issue one, or a flow that did not ask. */
  readonly refreshToken: string | null,
  /** The OpenID Connect identity token, unverified; see the module header. */
  readonly idToken: string | null,
  readonly scope: string | null,
  /**
   * When the access token stops working, or `null` when the provider did not
   * say.
   *
   * A `Temporal.Instant` rather than a number, because an expiry is a point in
   * time and this is the value an application compares against another one.
   * `@uniflowed/core/temporal` supplies it on every host uf runs on, so this
   * type means the same thing in a worker as it does under `uf start`.
   */
  readonly expiresAt: Instant | null,
|};

/** Who the tokens turned out to be about. */
export type OAuthIdentity = {|
  /**
   * The provider's own stable id for this person.
   *
   * Not an email address. An email address is a thing people change and a thing
   * some providers let anybody claim without proving it, and an application
   * that keyed its accounts on one has an account takeover waiting for it. Put
   * the email in `claims` and key on this.
   */
  readonly subject: string,
  /** Anything else worth keeping about them. Readable by the application. */
  readonly claims?: { +[string]: mixed },
|};

/**
 * The one thing uf cannot write for you.
 *
 * Four strings and a function. Everything else about signing in is the same
 * whoever the provider is, which is why this type is the entire seam and why it
 * has no `name`, no icon, no button and no `type: "oauth2" | "oidc"` — those
 * are an application's business, and a framework that collected them would be
 * collecting them forever.
 */
export type OAuthProvider = {|
  /** Where the browser is sent to ask the person. Must be `https`. */
  readonly authorizationEndpoint: string,
  /** Where uf exchanges a code for tokens. Must be `https`. */
  readonly tokenEndpoint: string,
  readonly clientId: string,
  /**
   * The client secret, for a provider that issues one.
   *
   * Optional, because a public client with PKCE does not have one and should
   * not pretend to. When it is present uf sends it with HTTP Basic, which is
   * the method RFC 6749 requires a server to support and the one that keeps it
   * out of the request body.
   */
  readonly clientSecret?: string,
  /** The scopes to ask for, space-separated, exactly as the provider spells them. */
  readonly scope?: string,
  /**
   * Who these tokens are about.
   *
   * The only provider-specific code in the whole flow. Called once, on the
   * callback, with the tokens the exchange produced; whatever it returns is
   * what the application reads from `currentSession()` afterwards. Throwing is
   * how it says the tokens are not good enough — uf answers `502` and stores
   * nothing.
   */
  readonly identify: (tokens: TokenSet) => Promise<OAuthIdentity>,
|};

/** What an application sees of somebody who is signed in. */
export type Session = {|
  readonly subject: string,
  readonly claims: { +[string]: mixed },
  /**
   * The session is gone after this.
   *
   * A `Temporal.Instant`, so "is this about to run out" is
   * `Temporal.Instant.compare` and not arithmetic on two numbers whose units a
   * reader has to take on trust. `JSON.stringify` spells it as the same ISO
   * string the `session` handler answers with, so the value a server component
   * holds and the value the browser is told are one thing written twice.
   */
  readonly expiresAt: Instant,
|};

/** How long a sign-in lasts, and how long an unfinished one is remembered. */
const DEFAULT_SESSION_SECONDS = 60 * 60 * 24 * 7;
const DEFAULT_AUTHORIZATION_SECONDS = 60 * 10;

/** What `createAuth` may be told. */
export type AuthOptions = {|
  readonly provider: OAuthProvider,
  readonly store: SessionStore,
  /**
   * Where `callback` is mounted, as a path on this site.
   *
   * uf builds the `redirect_uri` from it and the request's origin, and the
   * provider must have the same string registered. It is required rather than
   * guessed: a framework that assumed `/auth/callback` would be a framework
   * whose convention has to match a value in somebody else's dashboard.
   */
  readonly callbackPath: string,
  /** Where a finished sign-in goes when the request did not say. `/` by default. */
  readonly defaultReturnPath?: string,
  /**
   * The absolute origin to build the `redirect_uri` from.
   *
   * Optional, and worth setting. Without it uf uses the request's own origin,
   * which comes from the `Host` header — a value the client chose. That is
   * survivable because the provider only accepts a `redirect_uri` that has been
   * registered with it, so a forged `Host` produces a refused authorization
   * rather than a code delivered somewhere else; but "survivable because
   * somebody else checks" is not where a security-relevant absolute URL should
   * come from, and a deployment that knows its own name should say it.
   *
   * It is not used for the CSRF check. That one compares `Origin` against
   * `Host` and nothing else; see `sameOrigin`.
   */
  readonly origin?: string,
  /** The session cookie's name, before any `__Host-` prefix. */
  readonly cookieName?: string,
  /**
   * Whether uf's cookies are `Secure` and carry the `__Host-` prefix. On by
   * default.
   *
   * One deployment-wide boolean rather than something derived per request from
   * the scheme, and that is the point. A cookie whose *name* depended on how a
   * request arrived would have two names, and a reader looking for the session
   * would have to try both — at which point a subdomain can set the unprefixed
   * one and be preferred whenever the prefixed one happens to be absent, which
   * is the exact fixation `__Host-` exists to stop. One name, decided once.
   *
   * On by default because the wrong default here is somebody's session. It
   * costs nothing under `uf dev`: browsers treat `http://localhost` as a secure
   * context and accept `Secure` and `__Host-` cookies from it. Turning it off
   * is for the one case that genuinely cannot be secure — a shared development
   * host reached by name over plain HTTP — and it is spelled out so that it is
   * a decision somebody made rather than a default they inherited.
   */
  readonly secureCookies?: boolean,
  readonly sessionSeconds?: number,
  readonly authorizationSeconds?: number,
|};

/**
 * The four handlers, and the two ways to read what they established.
 *
 * Each handler is a plain `Request` → `Response`, which is what a
 * `_uf.route.js` exports and what runs unchanged on Node, Bun, Deno and a
 * worker. They are values rather than methods so that
 * `export const GET = auth.authorize` is the whole of mounting one.
 */
export type Auth = {|
  /** `GET`: begin. Redirects to the provider. `?return=/somewhere` comes back. */
  readonly authorize: (request: Request) => Promise<Response>,
  /** `GET`: the provider sends the browser here. Establishes the session. */
  readonly callback: (request: Request) => Promise<Response>,
  /** `POST`: exchange the stored refresh token for a new access token. */
  readonly refresh: (request: Request) => Promise<Response>,
  /** `GET`: who is signed in. `DELETE`: sign out. */
  readonly session: (request: Request) => Promise<Response>,
  /** The session behind this request's cookie, for a loader or a component. */
  readonly currentSession: () => Promise<Session | null>,
  /**
   * This request's provider tokens, for calling the provider's own API.
   *
   * Named apart from `currentSession` so that reaching for a credential is a
   * thing somebody chose to do rather than a field they happened to destructure.
   * What comes back must not be rendered, returned from a loader, or put in a
   * response: a loader's value is embedded in the document uf sends to the
   * browser, so a token that reaches one has been published.
   */
  readonly tokens: () => Promise<TokenSet | null>,
|};

/**
 * Wire a provider, a store and a mount point into the four handlers.
 *
 * The endpoints are checked here rather than on the first request, so a
 * provider configured with an `http://` token endpoint fails when the
 * application is built rather than the first time somebody signs in.
 */
export function createAuth(options: AuthOptions): Auth {
  const { provider, store } = options;
  requireSecureEndpoint(provider.authorizationEndpoint, "authorizationEndpoint");
  requireSecureEndpoint(provider.tokenEndpoint, "tokenEndpoint");

  const callbackPath = options.callbackPath;
  const defaultReturnPath = options.defaultReturnPath ?? "/";
  const secure = options.secureCookies ?? true;
  const sessionName = cookieNameFor(options.cookieName ?? "uf.session", secure);
  const pendingName = cookieNameFor(`${options.cookieName ?? "uf.session"}.pending`, secure);
  const sessionSeconds = options.sessionSeconds ?? DEFAULT_SESSION_SECONDS;
  const authorizationSeconds = options.authorizationSeconds ?? DEFAULT_AUTHORIZATION_SECONDS;

  /** The store key for a session id, kept apart from a pending one by construction. */
  const sessionKey = (id: string) => `uf.session:${id}`;
  const pendingKey = (id: string) => `uf.pending:${id}`;

  /**
   * One cookie of this request's.
   *
   * Read off the `Request` rather than through `cookies()`, so the four
   * handlers are functions of their argument: a caller can drive one without a
   * host having established a request, which is what makes the flow testable
   * end to end. `currentSession` is the one that cannot do this, because a
   * server component has no request to be handed.
   */
  const readCookie = (request: Request, name: string): string | null => {
    const jar = parseCookies(request.headers.get("cookie"));
    return Object.hasOwn(jar, name) ? jar[name] : null;
  };

  const attributes = (maxAge: number): CookieAttributes => ({ maxAge, secure });

  async function authorize(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const redirectUri = new URL(callbackPath, options.origin ?? url.origin).toString();
    const returnTo = localPath(url.searchParams.get("return"), defaultReturnPath);

    const state = randomToken(32);
    const verifier = randomToken(32);
    const pendingId = randomToken(32);
    // The `redirect_uri` is stored, not recomputed on the callback. RFC 6749
    // requires the exchange to send the same one the authorization used, and
    // the two arrive on two different requests with two different `Host`
    // headers available to whoever is sending them. Storing it means the
    // callback cannot be talked into exchanging against a different URL than
    // the one the code was issued for.
    await store.write(
      pendingKey(pendingId),
      { state, verifier, returnTo, redirectUri },
      expiryAfter(authorizationSeconds),
    );

    const target = new URL(provider.authorizationEndpoint);
    target.searchParams.set("response_type", "code");
    target.searchParams.set("client_id", provider.clientId);
    target.searchParams.set("redirect_uri", redirectUri);
    target.searchParams.set("state", state);
    target.searchParams.set("code_challenge", await pkceChallenge(verifier));
    target.searchParams.set("code_challenge_method", "S256");
    if (provider.scope != null && provider.scope !== "") {
      target.searchParams.set("scope", provider.scope);
    }

    const headers = privateHeaders();
    headers.set("location", target.toString());
    headers.append(
      "set-cookie",
      cookieHeader(pendingName, pendingId, attributes(authorizationSeconds)),
    );
    return new Response(null, { status: 302, headers });
  }

  async function callback(request: Request): Promise<Response> {
    const url = new URL(request.url);
    // Cleared on every path out of this handler, including the refusals. A
    // pending cookie that outlives its record is a cookie the browser sends
    // for nothing on every subsequent request.
    const clearPending = cookieHeader(pendingName, "", attributes(0));
    const refuse = (reason: string) => {
      // The reason is a constant from this function and never anything the
      // request carried. `@uniflowed/server/log` explains why a value a client
      // chose does not belong in a log line, and the `error` a provider sends
      // back in the query string is exactly such a value.
      //
      // `logger()` rather than the process logger, so a refusal carries the id
      // of the request that was refused. A sign-in that failed is the thing
      // somebody writes in and asks about, and the alternative is an operator
      // with four identical lines and no way to tell which one is theirs.
      logger().warn("oauth callback refused", { reason });
      const headers = privateHeaders();
      headers.append("set-cookie", clearPending);
      return new Response("sign-in failed\n", { status: 400, headers });
    };

    const pendingId = readCookie(request, pendingName);
    if (pendingId == null) {
      return refuse("no authorization is in progress in this browser");
    }
    // Taken, so a second callback carrying the same `state` finds nothing. This
    // one call is what makes the parameter single-use; see `SessionStore.take`.
    const pending = await store.take(pendingKey(pendingId));
    if (pending == null) {
      return refuse("the authorization has expired or was already completed");
    }

    if (url.searchParams.get("error") != null) {
      return refuse("the provider refused the authorization");
    }
    const code = url.searchParams.get("code");
    const state = url.searchParams.get("state");
    if (code == null || state == null) {
      return refuse("the callback carried no code or no state");
    }
    if (!constantTimeEquals(state, text(pending.state))) {
      return refuse("the state parameter did not match");
    }

    let tokens: TokenSet;
    try {
      tokens = await exchange(provider, {
        grant_type: "authorization_code",
        code,
        redirect_uri: text(pending.redirectUri),
        code_verifier: text(pending.verifier),
      });
    } catch (error) {
      logger().warn("oauth token exchange failed", { error });
      const headers = privateHeaders();
      headers.append("set-cookie", clearPending);
      return new Response("sign-in failed\n", { status: 502, headers });
    }

    let identity: OAuthIdentity;
    try {
      identity = await provider.identify(tokens);
    } catch (error) {
      logger().warn("oauth identify failed", { error });
      const headers = privateHeaders();
      headers.append("set-cookie", clearPending);
      return new Response("sign-in failed\n", { status: 502, headers });
    }

    // Session fixation: whoever was signed in here before is signed out, and
    // the new session gets an id nobody has seen. Without this, an attacker who
    // can set the session cookie — a subdomain on a deployment without
    // `__Host-`, a network position on plain HTTP — could fix the id before the
    // victim signs in and hold a session that becomes theirs afterwards.
    const previous = readCookie(request, sessionName);
    if (previous != null) {
      await store.destroy(sessionKey(previous));
    }

    const sessionId = randomToken(32);
    const expiresAt = expiryAfter(sessionSeconds);
    // `expiresAt` is in the record as well as being handed to the store. The
    // store's copy decides when the entry stops existing; this one is what an
    // application reads out of `currentSession()`, and a store that expires
    // entries itself would otherwise be the only thing that knew.
    await store.write(
      sessionKey(sessionId),
      {
        subject: identity.subject,
        claims: identity.claims ?? {},
        expiresAt,
        tokens: storedTokens(tokens),
      },
      expiresAt,
    );

    const headers = privateHeaders();
    // 303 rather than 302, so the browser is unambiguously told to `GET` the
    // page it lands on. The callback was a `GET` already, so no client actually
    // gets this wrong — and "unambiguous" is cheaper than "nobody has reported
    // it".
    //
    // Checked again on the way out even though `authorize` checked it on the
    // way in. What comes back from a store is what a store had, and a store is
    // a database somebody else can reach; the guard is four comparisons and the
    // failure it prevents is an open redirect with this site's name on it.
    headers.set("location", localPath(text(pending.returnTo), defaultReturnPath));
    headers.append("set-cookie", clearPending);
    headers.append("set-cookie", cookieHeader(sessionName, sessionId, attributes(sessionSeconds)));
    return new Response(null, { status: 303, headers });
  }

  async function refresh(request: Request): Promise<Response> {
    if (!sameOrigin(request)) {
      return new Response("cross-site request refused\n", {
        status: 403,
        headers: privateHeaders(),
      });
    }
    const id = readCookie(request, sessionName);
    const record = id == null ? null : await store.read(sessionKey(id));
    if (id == null || record == null) {
      return new Response("not signed in\n", { status: 401, headers: privateHeaders() });
    }
    const held = tokensOf(record);
    if (held == null || held.refreshToken == null) {
      return new Response("this session has no refresh token\n", {
        status: 409,
        headers: privateHeaders(),
      });
    }

    let renewed: TokenSet;
    try {
      renewed = await exchange(provider, {
        grant_type: "refresh_token",
        refresh_token: held.refreshToken,
      });
    } catch (error) {
      logger().warn("oauth refresh failed", { error });
      return new Response("refresh failed\n", { status: 502, headers: privateHeaders() });
    }

    // The session id is deliberately not rotated here. Rotating it on every
    // refresh reads as good hygiene and is a bug: two requests refreshing at
    // once would each write a new id and each set a cookie, and whichever
    // arrived second would sign the person out. The moment fixation matters is
    // sign-in, and that is where the id is new.
    //
    // A provider that rotates its own refresh token is honoured: the new one
    // replaces the old, and a provider that returned none keeps the old one,
    // which is what RFC 6749 says an omitted `refresh_token` means.
    const expiresAt = expiryAfter(sessionSeconds);
    await store.write(
      sessionKey(id),
      {
        subject: record.subject,
        claims: record.claims ?? {},
        expiresAt,
        tokens: storedTokens({
          ...renewed,
          refreshToken: renewed.refreshToken ?? held.refreshToken,
        }),
      },
      expiresAt,
    );
    // No body. A token is what this endpoint just obtained and the browser is
    // the one place it must not go: the cookie is `HttpOnly` precisely so that
    // script cannot read the credential, and answering with the token would
    // hand it over through the front door instead.
    return new Response(null, { status: 204, headers: privateHeaders() });
  }

  async function session(request: Request): Promise<Response> {
    const method = request.method.toUpperCase();
    const id = readCookie(request, sessionName);

    if (method === "GET" || method === "HEAD") {
      const record = id == null ? null : await store.read(sessionKey(id));
      if (record == null) {
        return new Response(null, { status: 401, headers: privateHeaders() });
      }
      const headers = privateHeaders();
      headers.set("content-type", "application/json; charset=utf-8");
      // The subject and the claims. Never the tokens: this body is a response,
      // and a response is a thing a browser extension, a shared computer and an
      // over-eager cache all get to see. The expiry goes out as the ISO string
      // an `Instant` serializes to, which is what `Session` carries — so a
      // browser and a server component asking the same question are told the
      // same answer spelled the same way.
      const body = JSON.stringify({
        subject: record.subject,
        claims: record.claims ?? {},
        expiresAt: instantOf(record.expiresAt),
      });
      return new Response(method === "HEAD" ? null : body, { status: 200, headers });
    }

    if (method === "DELETE") {
      // Signing out changes state, so it is checked like every other state
      // change here. A sign-out a hostile page can trigger is only a nuisance,
      // and a nuisance that costs one line to refuse is a nuisance uf refuses.
      if (!sameOrigin(request)) {
        return new Response("cross-site request refused\n", {
          status: 403,
          headers: privateHeaders(),
        });
      }
      if (id != null) {
        await store.destroy(sessionKey(id));
      }
      const headers = privateHeaders();
      headers.append("set-cookie", cookieHeader(sessionName, "", attributes(0)));
      return new Response(null, { status: 204, headers });
    }

    const headers = privateHeaders();
    headers.set("allow", "GET, HEAD, DELETE");
    return new Response(null, { status: 405, headers });
  }

  /** The record behind this request's session cookie, or `null`. */
  async function currentRecord(): Promise<StoredValue | null> {
    // `cookies()` rather than a `Request`, because a loader and a server
    // component have no request in hand — which is the whole point of
    // `@uniflowed/server`. It also counts as a read of request state, so a page
    // that asks who is signed in is a page uf will not put in the route cache.
    // That is not a side effect to work around; it is the correct answer, and
    // it is the same rule that stops a cached document carrying somebody's
    // cookie.
    const id = cookies().get(sessionName);
    return id == null ? null : await store.read(sessionKey(id));
  }

  async function currentSession(): Promise<Session | null> {
    const record = await currentRecord();
    if (record == null) {
      return null;
    }
    return {
      subject: text(record.subject),
      claims: record.claims == null ? {} : (record.claims as $FlowFixMe),
      expiresAt: instantOf(record.expiresAt),
    };
  }

  async function tokens(): Promise<TokenSet | null> {
    const record = await currentRecord();
    return record == null ? null : tokensOf(record);
  }

  return { authorize, callback, refresh, session, currentSession, tokens };
}

/**
 * The headers every response from this module carries.
 *
 * `no-store` on all of them, and it is the rule `docs/security.md` states as
 * "nothing personalised may become cacheable" rather than a precaution. Each of
 * these responses is about exactly one person: a redirect carrying a `state`, a
 * `Set-Cookie` holding a session, a body naming who is signed in. A shared
 * cache that kept any of them would hand one person's sign-in to the next
 * visitor, which is the shape of the RSC cache-poisoning failure in the same
 * document with a session in place of a page.
 *
 * `Vary: Cookie` is there so that a cache which ignores `no-store` — and they
 * exist — at least does not answer one person from another's entry.
 *
 * `Referrer-Policy: no-referrer` because the callback's own URL contains the
 * authorization code. Nothing renders here, so no `Referer` should be produced
 * at all; saying so costs a header and closes the case where something later
 * does.
 */
function privateHeaders(): Headers {
  return new Headers({
    "cache-control": "no-store, private",
    vary: "Cookie",
    "referrer-policy": "no-referrer",
  });
}

/**
 * Ask the token endpoint for tokens.
 *
 * One function for both grants, because they differ by two form fields and
 * nothing else that matters. What is worth reading here is where the client
 * secret goes: HTTP Basic when there is one, which RFC 6749 requires every
 * server to accept and which keeps the secret out of the body — a body that is
 * far more likely to end up in a proxy log than a header everybody already
 * knows not to print. A public client sends `client_id` in the body instead and
 * relies on PKCE, which is what PKCE is for.
 */
async function exchange(
  provider: OAuthProvider,
  form: { readonly [string]: string },
): Promise<TokenSet> {
  const body = new URLSearchParams(form);
  const headers: { [string]: string } = {
    "content-type": "application/x-www-form-urlencoded",
    accept: "application/json",
  };
  const secret = provider.clientSecret;
  if (secret != null && secret !== "") {
    headers.authorization = `Basic ${btoa(`${encodeURIComponent(provider.clientId)}:${encodeURIComponent(secret)}`)}`;
  } else {
    body.set("client_id", provider.clientId);
  }

  const response = await fetch(provider.tokenEndpoint, {
    method: "POST",
    headers,
    body,
    // A token response must not be answered from a cache, by uf's own fetch
    // cache or by anything between here and the provider.
    cache: "no-store",
  });
  // The status and nothing else. The body of a failed token response routinely
  // repeats back what was sent, which is the code and sometimes the secret.
  if (!response.ok) {
    throw new Error(`the token endpoint answered ${String(response.status)}`);
  }
  const payload = await readBoundedJson(response);
  if (payload == null || typeof payload !== "object") {
    throw new Error("the token endpoint answered with something that is not an object");
  }
  const accessToken = payload.access_token;
  if (typeof accessToken !== "string" || accessToken === "") {
    throw new Error("the token endpoint answered without an access token");
  }
  return {
    accessToken,
    tokenType: typeof payload.token_type === "string" ? payload.token_type : "Bearer",
    refreshToken: typeof payload.refresh_token === "string" ? payload.refresh_token : null,
    idToken: typeof payload.id_token === "string" ? payload.id_token : null,
    scope: typeof payload.scope === "string" ? payload.scope : null,
    expiresAt: expiryFrom(payload.expires_in),
  };
}

/**
 * The longest `expires_in` uf will believe, in seconds.
 *
 * A century, which is longer than any access token has ever been issued for
 * and short enough that adding it to now stays inside the range Temporal
 * represents. `docs/security.md` rule 4 — no unbounded anything — applies to a
 * provider's response exactly as it applies to a browser's request: a token
 * endpoint is a third party, and a third party that answers `1e308` must not
 * be able to throw a `RangeError` out of the middle of a sign-in.
 */
const MAX_EXPIRES_IN_SECONDS = 60 * 60 * 24 * 365 * 100;

/**
 * `expires_in` from a token response, as the instant the token stops working.
 *
 * `null` for a provider that did not say, and for one that said something that
 * is not a number of seconds — `NaN`, an infinity, a negative lifetime, or one
 * past [`MAX_EXPIRES_IN_SECONDS`]. Deny by default (rule 3): "the provider did
 * not tell us when this expires" is a state the rest of the flow already
 * handles, and it is the honest reading of a value that cannot be one.
 */
function expiryFrom(seconds: mixed): Instant | null {
  if (typeof seconds !== "number" || !Number.isFinite(seconds)) {
    return null;
  }
  if (seconds < 0 || seconds > MAX_EXPIRES_IN_SECONDS) {
    return null;
  }
  return Temporal.Now.instant().add({ seconds });
}

/** The tokens inside a stored session record, if it is shaped like one. */
function tokensOf(record: StoredValue): TokenSet | null {
  const held = record.tokens;
  if (held == null || typeof held !== "object") {
    return null;
  }
  const accessToken = held.accessToken;
  if (typeof accessToken !== "string") {
    return null;
  }
  return {
    accessToken,
    tokenType: typeof held.tokenType === "string" ? held.tokenType : "Bearer",
    refreshToken: typeof held.refreshToken === "string" ? held.refreshToken : null,
    idToken: typeof held.idToken === "string" ? held.idToken : null,
    scope: typeof held.scope === "string" ? held.scope : null,
    expiresAt: typeof held.expiresAt === "number" ? instantOf(held.expiresAt) : null,
  };
}

/**
 * A [`TokenSet`] as the store holds it: the same fields, with the expiry back
 * on the wire as epoch milliseconds.
 *
 * Written out rather than spread, because the pair with [`tokensOf`] is what
 * keeps the two sides of the store honest — a field added to `TokenSet` that
 * nothing here converts is a field that goes into a database as an object and
 * comes back as one nobody reads. See `./internal/oauth.js`'s header for why
 * the wire is a number at all.
 */
function storedTokens(tokens: TokenSet): StoredValue {
  return {
    accessToken: tokens.accessToken,
    tokenType: tokens.tokenType,
    refreshToken: tokens.refreshToken,
    idToken: tokens.idToken,
    scope: tokens.scope,
    expiresAt: tokens.expiresAt == null ? null : tokens.expiresAt.epochMilliseconds,
  };
}

/**
 * `seconds` from now, as the epoch milliseconds [`SessionStore.write`] takes.
 *
 * The addition is Temporal's — `Instant.add({ seconds })` — rather than
 * `now + seconds * 1000`, so the unit is in the call instead of in a constant a
 * reader has to check; `.epochMilliseconds` at the end is the seam, and
 * `./internal/oauth.js`'s header says why it stops being an instant there.
 */
function expiryAfter(seconds: number): number {
  return Temporal.Now.instant().add({ seconds }).epochMilliseconds;
}

/**
 * An expiry read back out of a store, as an instant.
 *
 * The epoch for anything that is not a number, which is a record that has been
 * tampered with or written by an older version of this package. Already expired
 * is the safe reading of "this record does not say when it expires": the caller
 * treats it as a session that has run out rather than as one that never does.
 */
function instantOf(value: mixed): Instant {
  return Temporal.Instant.fromEpochMilliseconds(typeof value === "number" ? value : 0);
}

/**
 * One field of a stored record as a string.
 *
 * A store hands back whatever it was given, and what it was given came out of
 * somebody else's database. Narrowing it here means the flow above reads as the
 * flow rather than as a sequence of type tests, and a record that has been
 * tampered with produces an empty string rather than a value of the wrong shape
 * halfway down a token exchange.
 */
function text(value: mixed, fallback?: string): string {
  return typeof value === "string" ? value : (fallback ?? "");
}
