// @flow
//
// Signing in: the contract, and everything that must refuse.
//
// ubugeeei-prod/uf#505 asks for a contract rather than a provider, so the first
// thing this file does is be a provider — twenty lines, no dependency, written
// from nothing but the type. If the seam were bigger than that, this file would
// be the evidence.
//
// The rest is refusals. OAuth is the one place in this repository where a
// mistake is somebody's account, so every case below is a specific published
// way to steal one:
//
// * a callback carrying a `state` that is not the one this browser started with
//   — cross-site request forgery on the sign-in itself, which is how somebody
//   ends up signed into an account that is not theirs;
// * the same `state` twice — a replayed callback, which a store that read
//   without removing would let through;
// * a callback in a browser that never started a flow — the same attack with
//   the cookie missing instead of wrong;
// * `?return=//evil.example` — an open redirect wearing this site's domain;
// * a cross-site `POST` to the endpoints that change something — `Origin`
//   against `Host`, and never `X-Forwarded-Host`;
// * a token in a response body, a cacheable header, or a log line.
//
// # Why there is no network here
//
// `fetch` is replaced for the duration of a case. The alternative is a test
// that needs an OAuth provider to be reachable, which is a test that does not
// run — and the thing under test is uf's half: what it sends, what it stores,
// and what it refuses. What the provider does with the request is the
// provider's.

import { describe, expect, it } from "@uniflowed/test";
import { Temporal } from "@uniflowed/core/temporal";
import { fixedClock, setClock } from "@uniflowed/core/clock";
import { contextFor, runWithContext } from "@uniflowed/server/host";
import { installLogger, recordingLogger } from "@uniflowed/server/log";
import type { OAuthProvider } from "@uniflowed/server/oauth";
import { createAuth, memorySessionStore, sameOrigin } from "@uniflowed/server/oauth";

const SITE = "https://app.example";

/**
 * A provider, in full.
 *
 * This is the deliverable of #505 as a piece of evidence rather than a claim:
 * two URLs, a client id, a scope and one function. Nothing about uf appears in
 * it, and nothing about this provider appears in uf.
 */
const provider: OAuthProvider = {
  authorizationEndpoint: "https://provider.example/oauth/authorize",
  tokenEndpoint: "https://provider.example/oauth/token",
  clientId: "client-1",
  clientSecret: "shhh",
  scope: "openid email",
  async identify(tokens) {
    // The subject is deliberately not derived from the token: several cases
    // below assert that no token text reaches a body, and a subject containing
    // one would make those assertions pass for the wrong reason.
    return {
      subject: tokens.accessToken === "" ? "anonymous" : "user-1",
      claims: { email: "a@b.example" },
    };
  },
};

/** Every request the replaced `fetch` was asked to make. */
type Call = {| url: string, form: URLSearchParams, headers: { [string]: string } |};

/**
 * Run `body` with the token endpoint answering `payload`.
 *
 * Restored in a `finally`, because a global left replaced by a failing case is
 * a failure reported against whichever case ran next.
 */
async function withTokenEndpoint<T>(
  payload: mixed,
  body: (calls: Array<Call>) => Promise<T>,
  status?: number,
): Promise<T> {
  const calls: Array<Call> = [];
  const real = globalThis.fetch;
  (globalThis: $FlowFixMe).fetch = async (url: mixed, init: $FlowFixMe) => {
    const headers: { [string]: string } = {};
    for (const name of Object.keys(init.headers ?? {})) {
      headers[name] = init.headers[name];
    }
    calls.push({ url: String(url), form: new URLSearchParams(String(init.body)), headers });
    return new Response(JSON.stringify(payload), {
      status: status ?? 200,
      headers: { "content-type": "application/json" },
    });
  };
  try {
    return await body(calls);
  } finally {
    (globalThis: $FlowFixMe).fetch = real;
  }
}

/** A configured `auth` and the store behind it. */
function signingIn(options?: { [string]: mixed }) {
  const store = memorySessionStore();
  const auth = createAuth({
    provider,
    store,
    callbackPath: "/auth/callback",
    ...options,
  });
  return { auth, store };
}

/** One `Set-Cookie` value from a response, by cookie name. */
function setCookie(response: Response, name: string): string | null {
  for (const value of response.headers.getSetCookie()) {
    if (value.startsWith(`${name}=`)) return value;
  }
  return null;
}

/** The value of a cookie a response set, or `""` when it cleared it. */
function cookieValue(response: Response, name: string): string {
  const header = setCookie(response, name) ?? "";
  return header.slice(name.length + 1).split(";")[0];
}

/** A request carrying `cookies`, addressed to this site. */
function get(path: string, cookies?: { readonly [string]: string }): Request {
  return new Request(`${SITE}${path}`, { headers: headersFor(cookies) });
}

function headersFor(cookies?: { readonly [string]: string }): { [string]: string } {
  if (cookies == null) return {};
  const jar = Object.keys(cookies)
    .map((name) => `${name}=${cookies[name]}`)
    .join("; ");
  return { cookie: jar };
}

/** Begin a flow and hand back everything the callback will need. */
async function begun(auth: $FlowFixMe, returnTo?: string) {
  const started = await auth.authorize(get(`/auth/authorize${returnTo ?? ""}`));
  const target = new URL(started.headers.get("location") ?? "");
  return {
    started,
    state: target.searchParams.get("state") ?? "",
    challenge: target.searchParams.get("code_challenge") ?? "",
    pending: cookieValue(started, "__Host-uf.session.pending"),
  };
}

/** The tokens a provider would answer an exchange with. */
const TOKENS = {
  access_token: "at-1",
  refresh_token: "rt-1",
  token_type: "Bearer",
  expires_in: 3600,
};

describe("beginning a flow", () => {
  it("sends the browser to the provider with a state and an S256 challenge", async () => {
    const { auth } = signingIn();

    const response = await auth.authorize(get("/auth/authorize"));
    const target = new URL(response.headers.get("location") ?? "");

    expect(response.status).toBe(302);
    expect(target.origin + target.pathname).toBe("https://provider.example/oauth/authorize");
    expect(target.searchParams.get("response_type")).toBe("code");
    expect(target.searchParams.get("client_id")).toBe("client-1");
    expect(target.searchParams.get("redirect_uri")).toBe(`${SITE}/auth/callback`);
    expect(target.searchParams.get("code_challenge_method")).toBe("S256");
    expect((target.searchParams.get("state") ?? "").length).toBeGreaterThan(30);
    expect((target.searchParams.get("code_challenge") ?? "").length).toBeGreaterThan(30);
  });

  it("never puts the PKCE verifier in the authorization request", async () => {
    // The whole of what PKCE buys. A verifier that travelled with the challenge
    // would mean an intercepted authorization URL is an intercepted sign-in,
    // which is the `plain` method uf refuses to offer.
    const { auth, store } = signingIn();

    const { started, challenge, pending } = await begun(auth);
    const record = await store.read(`uf.pending:${pending}`);
    const url = started.headers.get("location") ?? "";

    expect(typeof record?.verifier).toBe("string");
    expect(url).not.toContain(String(record?.verifier));
    expect(challenge).not.toBe(record?.verifier);
  });

  it("gives two flows two different states", async () => {
    // A state that repeated would be a state an attacker could reuse, which is
    // the whole reason it comes from `crypto.getRandomValues`.
    const { auth } = signingIn();

    const first = await begun(auth);
    const second = await begun(auth);

    expect(first.state).not.toBe(second.state);
  });

  it("marks the pending cookie HttpOnly, SameSite=Lax, Secure and host-only", async () => {
    // Each attribute closes something. `HttpOnly` keeps one XSS from being
    // every sign-in; `Lax` is required rather than preferred, because the
    // provider's redirect back is a cross-site top-level navigation and
    // `Strict` would withhold the cookie on exactly that one; `__Host-` stops a
    // subdomain planting a flow in somebody else's browser.
    const { auth } = signingIn();

    const response = await auth.authorize(get("/auth/authorize"));
    const header = setCookie(response, "__Host-uf.session.pending") ?? "";

    expect(header).toContain("HttpOnly");
    expect(header).toContain("SameSite=Lax");
    expect(header).toContain("Secure");
    expect(header).toContain("Path=/");
  });
});

describe("the callback", () => {
  it("refuses a callback whose state is not the one this browser started with", async () => {
    // The case #505 names, and the one that matters most: without it, an
    // attacker sends a victim a callback URL carrying the attacker's own
    // authorization code, and the victim's browser establishes a session that
    // belongs to the attacker's account.
    const { auth } = signingIn();
    const { pending } = await begun(auth);

    const response = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(
        get("/auth/callback?code=c&state=not-the-one", { "__Host-uf.session.pending": pending }),
      ),
    );

    expect(response.status).toBe(400);
    expect(setCookie(response, "__Host-uf.session")).toBe(null);
  });

  it("refuses a state that has already been spent", async () => {
    // Single use, and the reason `SessionStore` has `take` rather than a `read`
    // followed by a `destroy`: a replayed callback must find nothing, and two
    // arriving at once must not both find the record.
    const { auth } = signingIn();
    const { state, pending } = await begun(auth);
    const url = `/auth/callback?code=c&state=${state}`;

    const first = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(get(url, { "__Host-uf.session.pending": pending })),
    );
    const second = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(get(url, { "__Host-uf.session.pending": pending })),
    );

    expect(first.status).toBe(303);
    expect(second.status).toBe(400);
  });

  it("refuses a callback in a browser that never started a flow", async () => {
    // The state parameter proves the callback came from the provider. Only the
    // cookie proves it came back to the person who set out.
    const { auth } = signingIn();
    const { state } = await begun(auth);

    const response = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(get(`/auth/callback?code=c&state=${state}`)),
    );

    expect(response.status).toBe(400);
  });

  it("refuses a callback the provider sent an error with, without echoing it", async () => {
    // `?error=` is text the provider — or anybody who can make the browser
    // visit this URL — chose. It is not repeated into the body and not into the
    // log line; both are places somebody else reads.
    const { auth } = signingIn();
    const { pending } = await begun(auth);
    const { logger, records } = recordingLogger();
    installLogger(logger);
    let response;
    try {
      response = await auth.callback(
        get("/auth/callback?error=access_denied%3Cscript%3E", {
          "__Host-uf.session.pending": pending,
        }),
      );
    } finally {
      installLogger(null);
    }

    expect(response.status).toBe(400);
    expect(await response.text()).not.toContain("script");
    expect(JSON.stringify(records)).not.toContain("access_denied");
  });

  it("sends the stored redirect_uri to the token endpoint, not one built from this request", async () => {
    // Two requests, two `Host` headers, one of them chosen by whoever is
    // sending it. RFC 6749 requires the exchange to repeat the authorization's
    // `redirect_uri`, and storing it means a callback arriving with a forged
    // host cannot talk uf into exchanging against a different URL.
    const { auth } = signingIn();
    const { state, pending } = await begun(auth);

    const calls = await withTokenEndpoint(TOKENS, async (made) => {
      await auth.callback(
        new Request(`https://evil.example/auth/callback?code=c&state=${state}`, {
          headers: headersFor({ "__Host-uf.session.pending": pending }),
        }),
      );
      return made;
    });

    expect(calls[0].form.get("redirect_uri")).toBe(`${SITE}/auth/callback`);
  });

  it("sends the verifier the challenge was made from", async () => {
    const { auth, store } = signingIn();
    const { state, pending } = await begun(auth);
    const record = await store.read(`uf.pending:${pending}`);

    const calls = await withTokenEndpoint(TOKENS, async (made) => {
      await auth.callback(
        get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
      );
      return made;
    });

    expect(calls[0].form.get("code_verifier")).toBe(record?.verifier);
    expect(calls[0].form.get("grant_type")).toBe("authorization_code");
  });

  it("sends the client secret in a header rather than in the body", async () => {
    // A request body is far more likely to reach a proxy log than an
    // `Authorization` header everybody already knows not to print, and Basic is
    // the method RFC 6749 requires every server to accept.
    const { auth } = signingIn();
    const { state, pending } = await begun(auth);

    const calls = await withTokenEndpoint(TOKENS, async (made) => {
      await auth.callback(
        get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
      );
      return made;
    });

    expect(calls[0].headers.authorization).toBe(`Basic ${btoa("client-1:shhh")}`);
    expect(calls[0].form.get("client_secret")).toBe(null);
  });

  it("establishes a session and sends the browser back to a path on this site", async () => {
    const { auth, store } = signingIn();
    const { state, pending } = await begun(auth, "?return=/orders");

    const response = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(
        get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
      ),
    );
    const id = cookieValue(response, "__Host-uf.session");

    expect(response.status).toBe(303);
    expect(response.headers.get("location")).toBe("/orders");
    expect((await store.read(`uf.session:${id}`))?.subject).toBe("user-1");
  });

  it("gives a second sign-in a new session id and forgets the first", async () => {
    // Session fixation. An attacker who can set the session cookie could
    // otherwise fix an id before the victim signs in and hold a session that
    // becomes the victim's afterwards.
    const { auth, store } = signingIn();

    const first = await begun(auth);
    const one = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(
        get(`/auth/callback?code=c&state=${first.state}`, {
          "__Host-uf.session.pending": first.pending,
        }),
      ),
    );
    const firstId = cookieValue(one, "__Host-uf.session");

    const second = await begun(auth);
    const two = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(
        get(`/auth/callback?code=c&state=${second.state}`, {
          "__Host-uf.session.pending": second.pending,
          "__Host-uf.session": firstId,
        }),
      ),
    );
    const secondId = cookieValue(two, "__Host-uf.session");

    expect(secondId).not.toBe(firstId);
    expect(await store.read(`uf.session:${firstId}`)).toBe(null);
  });

  it("answers 502 and stores nothing when the token endpoint refuses", async () => {
    const { auth } = signingIn();
    const { state, pending } = await begun(auth);

    const response = await withTokenEndpoint(
      { error: "invalid_grant" },
      async () =>
        auth.callback(
          get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
        ),
      400,
    );

    expect(response.status).toBe(502);
    expect(setCookie(response, "__Host-uf.session")).toBe(null);
  });
});

describe("where a browser is sent afterwards", () => {
  it("refuses every spelling of somewhere that is not this site", async () => {
    // A sign-in that lands wherever a query parameter says is the classic
    // phishing amplifier: the link is genuinely on this domain and the page it
    // ends on is not. `//` and `/\` are the two spellings every naive
    // `startsWith("/")` check lets through, and the last entry is a `Location`
    // header carrying a second header.
    const hostile = [
      "https://evil.example/",
      "//evil.example/",
      "/\\evil.example",
      "javascript:alert(1)",
      "/ok\nLocation: https://evil.example",
    ];

    for (const target of hostile) {
      const { auth } = signingIn();
      const { state, pending } = await begun(auth, `?return=${encodeURIComponent(target)}`);

      const response = await withTokenEndpoint(TOKENS, async () =>
        auth.callback(
          get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
        ),
      );

      expect([target, response.headers.get("location")]).toEqual([target, "/"]);
    }
  });

  it("comes back to a path on this site when it is given one", async () => {
    const { auth } = signingIn();
    const { state, pending } = await begun(auth, "?return=%2Forders%2F8813%3Ftab%3Dopen");

    const response = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(
        get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
      ),
    );

    expect(response.headers.get("location")).toBe("/orders/8813?tab=open");
  });
});

describe("a cross-site request to something that changes state", () => {
  it("refuses a refresh whose Origin is not this site's Host", async () => {
    const { auth } = signingIn();

    const response = await auth.refresh(
      new Request(`${SITE}/auth/refresh`, {
        method: "POST",
        headers: { origin: "https://evil.example" },
      }),
    );

    expect(response.status).toBe(403);
  });

  it("is not persuaded by an X-Forwarded-Host that agrees with the attacker", async () => {
    // The rule `docs/security.md` states: `Origin` is compared against `Host`,
    // and a forwarded header is a header — something the client sent, and so
    // something that cannot decide whether the client may do what it is asking.
    const { auth } = signingIn();

    const response = await auth.refresh(
      new Request(`${SITE}/auth/refresh`, {
        method: "POST",
        headers: { origin: "https://evil.example", "x-forwarded-host": "evil.example" },
      }),
    );

    expect(response.status).toBe(403);
  });

  it("refuses a request that sends no Origin at all", async () => {
    // Every browser sends one on a `POST`, so a request without one is not a
    // browser — and a non-browser authenticating with a cookie is a request
    // that should not be answered, because the cookie is attached whether or
    // not the caller meant it to be.
    const { auth } = signingIn();

    const response = await auth.refresh(new Request(`${SITE}/auth/refresh`, { method: "POST" }));

    expect(response.status).toBe(403);
  });

  it("accepts one whose Origin is this site", async () => {
    const { auth } = signingIn();

    const response = await auth.refresh(
      new Request(`${SITE}/auth/refresh`, { method: "POST", headers: { origin: SITE } }),
    );

    // 401 rather than 403: the request was allowed, and there is no session.
    expect(response.status).toBe(401);
  });

  it("answers the same way for a sign-out", async () => {
    const { auth } = signingIn();

    const response = await auth.session(
      new Request(`${SITE}/auth/session`, {
        method: "DELETE",
        headers: { origin: "https://evil.example" },
      }),
    );

    expect(response.status).toBe(403);
  });
});

describe("the session endpoint", () => {
  /** Sign in, and hand back the session cookie that came out of it. */
  async function signedIn() {
    const { auth, store } = signingIn();
    const { state, pending } = await begun(auth);
    const response = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(
        get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
      ),
    );
    return { auth, store, id: cookieValue(response, "__Host-uf.session") };
  }

  it("says who is signed in without saying what their tokens are", async () => {
    // A response body is a thing a browser extension, a shared computer and an
    // over-eager cache all get to see. The access token is in the store, and it
    // stays there.
    const { auth, id } = await signedIn();

    const response = await auth.session(get("/auth/session", { "__Host-uf.session": id }));
    const body = await response.text();

    expect(response.status).toBe(200);
    expect(JSON.parse(body).subject).toBe("user-1");
    expect(body).not.toContain("at-1");
    expect(body).not.toContain("rt-1");
  });

  it("answers 401 for a browser that is not signed in", async () => {
    const { auth } = signingIn();

    expect((await auth.session(get("/auth/session"))).status).toBe(401);
  });

  it("signs out by destroying the record, not only by clearing the cookie", async () => {
    // A cookie the browser was told to drop is a cookie a copy of the request
    // still has. Signing out has to end the session at the end that holds it.
    const { auth, store, id } = await signedIn();

    const response = await auth.session(
      new Request(`${SITE}/auth/session`, {
        method: "DELETE",
        headers: { ...headersFor({ "__Host-uf.session": id }), origin: SITE },
      }),
    );

    expect(response.status).toBe(204);
    expect(cookieValue(response, "__Host-uf.session")).toBe("");
    expect(await store.read(`uf.session:${id}`)).toBe(null);
  });

  it("answers 405 with an Allow header for a method it does not have", async () => {
    const { auth } = signingIn();

    const response = await auth.session(
      new Request(`${SITE}/auth/session`, { method: "PUT", headers: { origin: SITE } }),
    );

    expect(response.status).toBe(405);
    expect(response.headers.get("allow")).toBe("GET, HEAD, DELETE");
  });
});

describe("what none of these responses may become", () => {
  it("marks every answer no-store, so nothing personal reaches a shared cache", async () => {
    // `docs/security.md`: nothing personalised may become cacheable. Each of
    // these responses is about exactly one person — a redirect carrying a
    // state, a `Set-Cookie` holding a session, a body naming who is signed in.
    const { auth } = signingIn();
    const { state, pending } = await begun(auth);

    const responses = [
      await auth.authorize(get("/auth/authorize")),
      await withTokenEndpoint(TOKENS, async () =>
        auth.callback(
          get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
        ),
      ),
      await auth.session(get("/auth/session")),
      await auth.refresh(new Request(`${SITE}/auth/refresh`, { method: "POST" })),
    ];

    for (const response of responses) {
      expect(response.headers.get("cache-control")).toBe("no-store, private");
      expect(response.headers.get("vary")).toBe("Cookie");
    }
  });

  it("keeps the authorization code out of the log when an exchange fails", async () => {
    // The one object in this package that certainly holds a credential is the
    // token endpoint's answer, and the one place uf could accidentally publish
    // it is a diagnostic about a failure.
    const { auth } = signingIn();
    const { state, pending } = await begun(auth);
    const { logger, records } = recordingLogger();
    installLogger(logger);
    try {
      await withTokenEndpoint(
        { error: "invalid_grant", access_token: "leaked-token" },
        async () =>
          auth.callback(
            get(`/auth/callback?code=super-secret-code&state=${state}`, {
              "__Host-uf.session.pending": pending,
            }),
          ),
        400,
      );
    } finally {
      installLogger(null);
    }

    const written = JSON.stringify(records);
    expect(written).not.toContain("super-secret-code");
    expect(written).not.toContain("leaked-token");
  });
});

describe("reading the session from a loader or a component", () => {
  it("answers about the request it is inside, with no request handed to it", async () => {
    const { auth, store } = signingIn();
    const { state, pending } = await begun(auth);
    const response = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(
        get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
      ),
    );
    const id = cookieValue(response, "__Host-uf.session");

    const context = contextFor(get("/orders", { "__Host-uf.session": id }));
    const session = await runWithContext(context, () => auth.currentSession());

    expect(session?.subject).toBe("user-1");
    expect(session?.claims.email).toBe("a@b.example");
    expect(await store.read(`uf.session:${id}`)).not.toBe(null);
  });

  it("makes the page that asked uncacheable, because it is a page about one person", async () => {
    // Not a side effect to work around: it is the correct answer, and it is the
    // same rule that stops a cached document carrying somebody's `Set-Cookie`.
    const { auth } = signingIn();
    const context = contextFor(get("/orders"));

    await runWithContext(context, () => auth.currentSession());

    expect(context.requestStateReads).toBeGreaterThan(0);
  });

  it("hands back tokens only through the reader that is named for it", async () => {
    // `currentSession` is the safe default and `tokens` is the decision. A
    // loader's value is embedded in the document uf sends to the browser, so a
    // token that reached one would have been published.
    const { auth } = signingIn();
    const { state, pending } = await begun(auth);
    const response = await withTokenEndpoint(TOKENS, async () =>
      auth.callback(
        get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
      ),
    );
    const id = cookieValue(response, "__Host-uf.session");
    const context = contextFor(get("/orders", { "__Host-uf.session": id }));

    const [session, tokens] = await runWithContext(context, async () => [
      await auth.currentSession(),
      await auth.tokens(),
    ]);

    expect(JSON.stringify(session)).not.toContain("at-1");
    expect(tokens?.accessToken).toBe("at-1");
  });
});

describe("when things expire", () => {
  /** Sign in with the clock stopped at `at`, and hand back the session. */
  async function signedInAt(at: string, options?: { [string]: mixed }) {
    const restore = setClock(fixedClock(Temporal.Instant.from(at).epochMilliseconds));
    try {
      const { auth, store } = signingIn(options);
      const { state, pending } = await begun(auth);
      const response = await withTokenEndpoint(TOKENS, async () =>
        auth.callback(
          get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
        ),
      );
      const id = cookieValue(response, "__Host-uf.session");
      const context = contextFor(get("/orders", { "__Host-uf.session": id }));
      const [session, tokens] = await runWithContext(context, async () => [
        await auth.currentSession(),
        await auth.tokens(),
      ]);
      return { auth, store, id, session, tokens };
    } finally {
      restore();
    }
  }

  it("dates the session from uf's clock, a configured lifetime later", async () => {
    // The expiry is `Instant.add({ seconds })` on the clock uf reads, not
    // `Date.now() + seconds * 1000`. Freezing the clock is what turns "it is
    // roughly an hour from now" into an equality a test can hold.
    const { session } = await signedInAt("2026-01-02T03:04:05Z", { sessionSeconds: 3600 });

    expect(session?.expiresAt.toString()).toBe("2026-01-02T04:04:05Z");
  });

  it("reads a token's own expiry back as the instant, not as a number", async () => {
    // `TokenSet.expiresAt` crosses the store as epoch milliseconds and comes
    // back an `Instant`, which is the pair `storedTokens` and `tokensOf` exist
    // to keep honest: a field that stopped being converted would arrive here as
    // a number and this case would say so.
    const { tokens } = await signedInAt("2026-01-02T03:04:05Z");

    expect(tokens?.expiresAt?.toString()).toBe("2026-01-02T04:04:05Z");
  });

  it("says a session it cannot date has already run out", async () => {
    // A record written by an older version, or one somebody has been at. The
    // safe reading of "this does not say when it expires" is "it has", and the
    // epoch is how that is spelled.
    const { auth, store } = signingIn();
    await store.write(
      "uf.session:tampered",
      { subject: "user-1", claims: {} },
      Temporal.Now.instant().add({ seconds: 60 }).epochMilliseconds,
    );
    const context = contextFor(get("/orders", { "__Host-uf.session": "tampered" }));

    const session = await runWithContext(context, () => auth.currentSession());

    expect(session?.expiresAt.epochMilliseconds).toBe(0);
  });

  it("refuses to believe an `expires_in` that is not a number of seconds", async () => {
    // A token endpoint is a third party and its answer is untrusted input.
    // `1e308` seconds is outside the range an instant can hold, so adding it
    // would throw a `RangeError` out of the middle of a sign-in rather than
    // producing the `502` that says the provider answered badly. `null` — "the
    // provider did not say" — is a state the rest of the flow already handles.
    const { auth } = signingIn();
    const { state, pending } = await begun(auth);

    const response = await withTokenEndpoint({ ...TOKENS, expires_in: 1e308 }, async () =>
      auth.callback(
        get(`/auth/callback?code=c&state=${state}`, { "__Host-uf.session.pending": pending }),
      ),
    );
    const id = cookieValue(response, "__Host-uf.session");
    const context = contextFor(get("/orders", { "__Host-uf.session": id }));
    const tokens = await runWithContext(context, () => auth.tokens());

    expect(response.status).toBe(303);
    expect(tokens?.expiresAt).toBe(null);
  });

  it("drops a pending record once the clock has passed it", async () => {
    // The store's own expiry, driven rather than waited for: a `state` that
    // outlived its ten minutes is a `state` a replay could still spend.
    const at = Temporal.Instant.from("2026-01-02T03:04:05Z");
    const { auth, store } = signingIn({ authorizationSeconds: 600 });
    const restoreStart = setClock(fixedClock(at.epochMilliseconds));
    let pending;
    try {
      ({ pending } = await begun(auth));
    } finally {
      restoreStart();
    }

    const later = at.add({ seconds: 601 });
    const restoreLater = setClock(fixedClock(later.epochMilliseconds));
    try {
      expect(await store.read(`uf.pending:${pending}`)).toBe(null);
    } finally {
      restoreLater();
    }
  });
});

describe("configuration uf refuses", () => {
  it("will not send a client secret to an http endpoint", async () => {
    // A rule with no exception is a rule somebody turns off, so loopback is
    // allowed and a real host over plain HTTP is not.
    expect(() =>
      createAuth({
        provider: { ...provider, tokenEndpoint: "http://provider.example/oauth/token" },
        store: memorySessionStore(),
        callbackPath: "/auth/callback",
      }),
    ).toThrow();
  });

  it("allows loopback over http, because that is where every one of these is developed", () => {
    expect(() =>
      createAuth({
        provider: {
          ...provider,
          authorizationEndpoint: "http://localhost:9000/authorize",
          tokenEndpoint: "http://localhost:9000/token",
        },
        store: memorySessionStore(),
        callbackPath: "/auth/callback",
      }),
    ).not.toThrow();
  });

  it("builds the redirect_uri from the configured origin rather than from the Host header", async () => {
    // The `Host` header is a value the client chose. A deployment that knows
    // its own name should say it, and then a forged host changes nothing.
    const { auth } = signingIn({ origin: "https://real.example" });

    const response = await auth.authorize(
      new Request("https://forged.example/auth/authorize", { headers: {} }),
    );
    const target = new URL(response.headers.get("location") ?? "");

    expect(target.searchParams.get("redirect_uri")).toBe("https://real.example/auth/callback");
  });
});

describe("sameOrigin", () => {
  it("compares the Origin header against the Host and nothing else", () => {
    const same = new Request(`${SITE}/x`, { method: "POST", headers: { origin: SITE } });
    const other = new Request(`${SITE}/x`, {
      method: "POST",
      headers: { origin: "https://evil.example", "x-forwarded-host": "app.example" },
    });

    expect(sameOrigin(same)).toBe(true);
    expect(sameOrigin(other)).toBe(false);
  });

  it("refuses the literal `null` origin a sandboxed document sends", () => {
    const opaque = new Request(`${SITE}/x`, { method: "POST", headers: { origin: "null" } });

    expect(sameOrigin(opaque)).toBe(false);
  });
});
