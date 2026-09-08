# Security

`uf` replaces a toolchain whose parts have shipped real, exploited
vulnerabilities. The goal is not to react to our own CVEs quickly; it is to be
structurally incapable of most of them, and to hold a regression test for every
class we have studied.

This document is the threat model. Every row names a published failure in an
incumbent tool, the structural decision that makes the same bug impossible or
loud in `uf`, and where the regression test lives. A row whose test does not
exist on `main` yet is marked `todo`; that row is a work item, not a claim.
Land the guard, then update the row in the same change.

Every CVE identifier below was verified against the NVD API before being cited.

## Rules

1. **Untrusted input is named.** Source text, file paths, HTTP requests, archive
   entries, registry metadata, and `package.json` fields are attacker-controlled
   in the threat model, even in a "local dev" tool. A developer machine running
   `uf install` on a cloned repository is a remote-code-execution target.
2. **Decide on the canonical form.** Never authorize against a raw request
   string, a pre-normalization path, or a partially decoded URL. Resolve first,
   then check, then use the value you checked — no re-derivation afterwards.
3. **Deny by default, allow by table.** Program names, adapters, hosts, and
   loaders come from fixed `&'static str` tables. Untrusted text never reaches a
   position where it can name a program, a path root, or a host.
4. **No unbounded anything.** Every parse, every cache, every recursion, and
   every read has an explicit bound and a typed error above it.
5. **No regex on untrusted input** unless the engine is proven non-backtracking.
   Hand-written single-pass parsers are cheaper than a ReDoS advisory.
6. **Every guard has a test that fails without it.** A guard with no failing
   test is a comment.

## Dev server and file serving

The dev server is Vite's, run through `@uniflowed/vite` on the project's
JavaScript host. `uf` used to carry a Rust HTTP server with its own request
pipeline and a corpus of the four `server.fs.deny` bypasses Vite shipped in one
year; that server is gone, because a second implementation of Vite's surface
is a second place for the same class of bug, and because the mitigations now
live upstream where every Vite project gets them.

What `uf` keeps is the policy around the server, which is where the structural
decisions are:

| Concern | Decision in `uf` | Where |
| --- | --- | --- |
| The four `server.fs.deny` bypasses ([CVE-2025-30208](https://github.com/advisories/GHSA-x574-m823-4x7w), [CVE-2025-31125](https://nvd.nist.gov/vuln/detail/CVE-2025-31125), [CVE-2025-32395](https://nvd.nist.gov/vuln/detail/CVE-2025-32395), [CVE-2025-62522](https://nvd.nist.gov/vuln/detail/CVE-2025-62522)) | `@uniflowed/vite` depends on a Vite line that contains every fix, and the dependency is what `uf install` resolves — a project cannot end up on an older server by naming one | `packages/vite/package.json` |
| DNS rebinding, and cross-origin requests with side effects | Loopback bind by default. `uf dev --host` refuses to start without a non-empty `dev.allowedHosts`, before any process is spawned, and the list is handed to Vite's `server.allowedHosts` unchanged; `*` is never written for the user | `uf_cli::commands::dev`, `packages/vite/driver.js` |
| A deny list weakened by project configuration | `dev.fs.deny` entries are handed to Vite's `server.fs.deny` on top of its built-in list (`.env`, `.env.*`, `*.{crt,pem}`, `**/.git/**`); there is no configuration that removes a built-in entry | `packages/vite/driver.js` |
| A dev server that outlives the command that started it | The driver's stdin is a pipe `uf` holds open and never writes to; when `uf` exits, for any reason, the pipe closes and the driver exits | `uf_cli::commands::vite` |
| A different `uf` on PATH transforming the project's modules | The driver is told which binary started it (`UF_BINARY`) and every transform goes through that one — including the ones answered from `.uf/cache/transform`, whose entries are keyed by the size and modification time of the binary that wrote them, so a different or rebuilt `uf` misses rather than inherits, and a host that cannot identify its binary caches nothing at all. `uf test` resolves the binary before it starts a worker — the running executable first, so an inherited `UF_BINARY` cannot redirect a run that knows its own path — and refuses to run at all when no route names one, rather than letting a worker fall back to a bare `uf` on `PATH` | `uf_cli::commands::vite`, `uf_cli::commands::test`, `packages/host/transform.js`, `packages/host/internal/node-hooks.js` |
| A rebuilt `uf` reporting the type errors the previous one found | `.uf/cache/check` entries are keyed by the size and modification time of the binary that wrote them, alongside every limit that can change what a check reports and a digest of every signature the file depends on; a rebuilt `uf`, a changed limit, or a moved declaration in a dependency all miss rather than inherit, and a process that cannot identify its own binary caches nothing at all. One record carries an answer per batch the file has been checked in — at most four, so a project checked many ways stays bounded — and each is believed only under the exact dependency digest it was computed with, so a whole-project run and a path-scoped one share a record without ever sharing an answer. A record that is truncated, of the wrong shape, about another file, or over its bound on answers or diagnostics is a miss | `uf_check::cache`, `uf_check::tests::cache` |
| A cache directory growing until the disk is full, and the sweep that stops it | The three answer caches — `.uf/cache/check`, `.uf/cache/transform` and `.uf/cache/task` — are each bounded at 128 MiB and swept to 96 MiB, coldest entry first. The sweep only ever unlinks — it never truncates or rewrites, so a concurrent reader sees a whole entry or none — it never removes a file used in the last minute, which is what keeps it away from a temporary another process is about to rename into place, and it does not follow symbolic links, so a link planted in a cache directory cannot make `uf` delete what it points at | `uf_infra::cache` |
| Unbounded work from a hostile module | Every stage of the transform has a ceiling — source size, tree depth — and a typed error above it; the transform service runs on a thread with a fixed large stack so a pathological input fails with a message | `uf_transform` |
| Inbound headers that steer dispatch — [CVE-2025-29927](https://nvd.nist.gov/vuln/detail/CVE-2025-29927)'s class | Vite's middleware chain is the only dispatch, and `uf` adds one middleware: render a document for a `GET`/`HEAD` whose `Accept` includes `text/html` and whose path has no file extension. It reads nothing else from the request | `packages/vite/index.js` |
| The build's own notes served to whoever guesses the filename | `uf-build-manifest.json`, `uf-rsc-manifest.json` and `uf-bundle-report.json` are written to `.uf/build/meta/` rather than into the output directory. Between them they name every route including the ones that were never prerendered, the source file behind each, and the size of every chunk; nothing reads them to answer a request. This is not a denylist in uf's two servers — the file is not there, so a static host has nothing to serve either | `uf_cli::commands::build`, `uf_cli::tests::vite` |

## Framework, RSC, and server actions

| Past failure | Structural decision in `uf` | Test |
| --- | --- | --- |
| [CVE-2025-29927](https://nvd.nist.gov/vuln/detail/CVE-2025-29927) — spoofing `x-middleware-subrequest` skips middleware, bypassing auth | No inbound request header participates in middleware dispatch: which middleware runs is decided by the request path against the directory each one guards, and by nothing else. Recursion control is internal state, never a header a client can send | `tests/library/middleware.test.js` |
| Server Action endpoint IDs globally disclosed | Action ids are keyed hashes of (module path, export name, build id), so they are neither guessable nor stable across builds; an action not reachable from a client boundary is never registered as an endpoint, and the table the endpoint dials into is built from the manifest's callable set rather than from anything a request carries | `uf_rsc::action`, `tests/library/server-actions.test.js` |
| A deserializer that reconstructs attacker-chosen objects — the class every "RCE through a serialization format" advisory is | A server action's arguments are plain JSON data, under the closed grammar below, plus at most one submitted form written beside them rather than inside one. No tag in a payload names a constructor, a module, a function or a reference; the single constructor the decoder can call is fixed in the source, so there is nothing for a payload to *become* | `tests/library/server-actions.test.js` |
| Prototype pollution through a request body — `__proto__` as an own property, which `JSON.parse` produces and the next spread applies | `__proto__`, `constructor` and `prototype` are refused as keys anywhere in a payload, in the one place a payload is decoded, rather than left for each action to remember | `tests/library/server-actions.test.js` |
| CSRF against a state-changing endpoint: a cross-site page posting an action with the visitor's cookies | Three independent guards, any one of which would do. The call carries `uf-action`, which is not a header a simple request may set, so a cross-origin caller needs a preflight and uf answers none; the content type must be `application/json`, which no `<form>` can produce; and `Origin` must be present and equal `Host`. `Host` alone, never `X-Forwarded-*` — a forwarded header is a string the caller wrote, so a proxy in front of a uf application has to preserve `Host` and one that rewrites it turns every action call into a `403` | `tests/library/server-actions.test.js`, `crates/uf_cli/tests/vite.rs` |
| A form post as a CSRF vehicle: a `<form>` submit is a *simple* cross-origin request, so an endpoint that accepts one accepts it from any page on the internet | `<form action={fn}>` never produces a native form post. React hands the reference a `FormData`, and the reference sends the same `application/json` request with the id in a header, with the form's entries beside the values — so all three guards above hold for a form call unchanged and no multipart parser exists to be reached. What that costs is stated rather than hidden: React writes `action="javascript:throw …"` for a form whose action carries no `$$FORM_ACTION`, so a submit before the page has hydrated throws in the page instead of calling — nothing reaches a server that was not meant to, and nothing happens either | `tests/library/server-actions.test.js`, `crates/uf_cli/tests/vite.rs` |
| An action endpoint used as an enumeration oracle | Every failed lookup is the same `404` with the same body, and the table is scanned whole with a constant-time comparison, so neither the answer nor the time says whether the id existed. The payload is decoded *before* the id is resolved, so a malformed body cannot be used to tell a real id from a guess | `tests/library/server-actions.test.js` |
| An application's internals in an error response — a message, a name, a stack | An action that throws is a `500` with a fixed body; the exception goes to the host's error reporting. The same in development as in production, because `uf dev` and `uf build` have to agree about what this endpoint answers | `tests/library/server-actions.test.js` |
| Unbounded work from a request body: an enormous payload, deep nesting, non-UTF-8 bytes | A byte ceiling counted as the body arrives rather than trusted from `Content-Length`, a depth ceiling, a value-count ceiling, an argument-count ceiling, an entry-count and field-name ceiling for a submitted form, and `TextDecoder(…, { fatal: true })` so invalid UTF-8 is refused rather than replaced. The walk that applies them is iterative, so the sender's hand is not on the stack depth. A file in a form is refused at the call site rather than base64-encoded into a body with no ceiling of its own | `tests/library/server-actions.test.js` |
| Server code reaching the browser through an action module — a database handle, a secret, `node:async_hooks` | A `"use server"` module is replaced, in the client graph only, by one reference per callable export. The RSC graph colours it server for the same reason, so the analysis and the bundle agree about it rather than about each other | `crates/uf_cli/tests/vite.rs`, `uf_rsc::graph` |
| An action's arguments or result that cannot cross a wire at all, arriving as `{}` | `uf prepare` writes the wire grammar into the generated `server-actions.js` as a bound over every action in the project, so an action taking a callback or returning a `Map` is a `uf check` error | `tests/type-tests/server-actions.js` |
| The build's whole module graph published at `/uf-rsc-manifest.json` — module paths, export names, diagnostics | The manifest is written into the output directory and served with it. The action ids in it are the ones already in the client bundle, so it discloses no endpoint that was not disclosed anyway; the module graph beside them is disclosure with no reader | todo |
| RSC cache poisoning when a shared cache does not partition response variants ([CVE-2026-44576](https://nvd.nist.gov/vuln/detail/CVE-2026-44576)) | Route, fetch, action, and data caches are **off by default**. When enabled, the RSC variant is part of the cache key, and the response carries the matching `Vary` | todo |
| Server-code leak: a `"use client"` module importing server-only code | The RSC graph rejects the edge at build time as an error, not a warning | `uf_rsc::graph` |
| Directive parsing bugs — `"use client"` accepted when not the first statement, or built from a template literal | The directive is only recognized as a plain string literal in leading directive position; everything else is a typed diagnostic | `uf_rsc::directive` |
| `"use server"` export that is not an async function | Rejected at build time; React's calling convention makes this a correctness *and* a safety issue | `uf_rsc::graph` |
| SSRF via WebSocket upgrade ([CVE-2026-44578](https://nvd.nist.gov/vuln/detail/CVE-2026-44578)) | uf has no proxying upgrade, and the one it does have cannot become one: `upgradeWebSocket(request)` upgrades the *inbound* connection the host is already answering, takes no address, and reaches no upstream. The host's own upgrader is a value the deployment passes where the server is built, never a name resolved from a request. A proxying upgrade would need the allowlist in its first commit rather than after one | `tests/library/transports.test.js` |
| Image optimizer: unbounded disk cache, CPU exhaustion from remote images, cache deception | Image caching is opt-in, remote sources require an explicit host allowlist, decode work is bounded by pixel budget, and the cache has a size ceiling | todo |
| A draft-mode cookie anybody can set for themselves — a flag rather than a token, so unpublished content is gated by a value you can type | The value is an expiry and an HMAC-SHA256 over it — domain-separated `uf-draft-v1`, length-prefixed, keyed with the deployment's secret, the same construction `crates/uf_rsc/src/action.rs` argues for under a different name — compared in constant time, with the expiry *inside* the signature so a holder cannot extend it and checked against uf's clock as well as by `Max-Age`. `__Host-` prefixed, `Secure`, `HttpOnly`, `SameSite=Lax`, `Path=/`, one name for every scheme so there is no second name a subdomain could plant. The key is `UF_DRAFT_SECRET`, refused below 32 bytes rather than stretched, and a per-process one otherwise with a `warn` saying what that costs | `tests/library/route-handler.test.js` |
| A cookie or a response header set from inside a render, at a moment when the headers may already be on the wire | `headers()` and `cookies()` are read-only, and `draftMode().enable()` — the one case that needs a response — is allowed only where a response is being produced: a route handler or a server action, marked by `asResponder`. Anywhere else, a guard included, it is a named `DraftModeError` rather than a decision nothing writes down | `tests/library/server.test.js`, `tests/library/request-lifecycle.test.js` |
| Draft content served out of a shared cache, or a published document served to somebody who came to see the draft | A request in draft mode never reaches the route cache and is never answered with a prerendered document — from `dist/` under `uf start` and every adapter, and from the embedded copy in a compiled binary. The other direction holds too: reading `draftMode()` counts as reading request state, so a render that consulted it is never stored | `tests/library/cache.test.js`, `tests/library/serve.test.js`, `tests/library/standalone.test.js` |
| XSS via CSP nonce handling and `beforeInteractive` scripts | Nonces are generated per response and never reused across a cached response; script injection points are typed, not string-concatenated | todo |

### The argument boundary

A `"use server"` export is a public HTTP endpoint the moment it exists, and the
one decision that makes it a feature rather than a remote-code-execution
surface is what the bytes on the wire are allowed to become. It is this, and
`packages/router/internal/action-wire.js` is the only place that applies it:

> **A server action's arguments are plain JSON data, plus at most one form.**
> One JSON object, `{"args": [...]}`, of at most 1 MiB of valid UTF-8, holding
> at most 16 values, nested at most 24 deep, with at most 10,000 values in
> total. Each of those is `null`, a boolean, a finite number, a string, an
> array of them, or a plain object whose keys are ordinary strings and are none
> of `__proto__`, `constructor` or `prototype`. One argument may instead be a
> submitted form, written beside the values as `"form": {"at": <index>,
> "entries": [[name, value], …]}` — at most 256 entries, field names of at most
> 128 characters, strings only, and the slot it names must hold `null`. The
> result travels back under exactly the same grammar minus the form, plus
> `undefined` for an action that returns nothing.

Nothing in a payload can name a function, a module, a class, a prototype, a
React element, an id or a reference, and nothing in it is revived into an
object the sender chose. What the decoder adds to `JSON.parse` is the refusal
of everything `JSON.parse` would have let through.

The form is the one exception and it is shaped so that it is not one. It is
*outside* the value tree rather than a tag inside it: `at` names a position and
not a type, the value walk is unchanged and still knows nothing but JSON data,
and the only constructor the decoder ever calls is `FormData` — fixed in the
source, never named by the bytes. A payload cannot say which class to build,
which is the property the rest of this section is about.

Four things are outside it deliberately, each because admitting it would mean
admitting a tag in the payload that says which constructor to call:

- **A reference format.** React's Flight payload carries references to client
  modules, promises and elements. uf has no such payload
  ([#252](https://github.com/ubugeeei-prod/uf/issues/252)), and this grammar is
  not the place to grow one quietly.
- **Class instances, `Map`, `Set`, `Date`, `RegExp`, typed arrays.** An action
  that wants a date takes an ISO string and parses it, where the parse is the
  application's and is checked.
- **Cycles and shared references**, which are a reference format by another
  name.
- **Multipart, a file upload, and a form that submits before hydration.** A
  form call is an ordinary `application/json` request carrying the id in a
  header, so all three of the CSRF guards above hold for it unchanged. Making a
  form work before its JavaScript has arrived would mean accepting a *native*
  form post — `multipart/form-data`, a simple cross-origin request that reaches
  a server with the visitor's cookies and no preflight — and that is a
  multipart parser plus a different CSRF story, not a smaller version of this
  one. So a `File` entry is refused at the call site, and a submit before the
  page has hydrated throws in the page — React's own answer for a form action
  with no `$$FORM_ACTION` — rather than posting anywhere.

The grammar is enforced twice, and the second time is what makes it a
*contract* rather than a runtime check: Flow holds every action's parameters
and return value against it at build time, through the two bounds `uf prepare`
writes into `server-actions.js`, and the endpoint applies it again to whatever
actually arrives. The two bounds are different types for the same reason the
grammar is asymmetric: a `FormData` is something a call passes and never
something a server answers with.

Validating the fields is the application's, and it is the application's *on the
server*: `maxlength` on an `<input>` is a convenience for the person typing,
and the request that reaches an action was not necessarily made by that
document. `crates/uf_cli/tests/fixtures/rsc-split-app` is the shape —
`@uniflowed/validator` between the raw entries and any value the action
believes, with the failure answered as state rather than thrown, because the
endpoint answers a thrown action with a `500` and nothing in it.

### A server action authorizes itself

An action call is a `POST` to the page's own URL carrying the id in a header,
so the middleware guarding that path runs above it exactly as it does above the
page — no reserved path to collide with a project's routes, and no second
spelling of "which guard applies here".

That is a convenience and it is not a boundary, because the URL is the caller's
to choose: a client that wants to skip the guard on `/dashboard` posts the same
id to `/`. **A server action is the unit of authorization**, the way a route
handler is, and a `"use server"` function that relies on a path guard having
run is a function with a hole in it.

## Signing in

`@uniflowed/server/oauth` is the one part of `uf` where a mistake is somebody's
account rather than somebody's afternoon. The decision that shapes the rest is
that `uf` ships **no provider**: the seam is four strings and one function, and
everything a provider has no opinion about — the state parameter, PKCE, the
callback, the session — belongs to `uf`, because those are exactly the parts
that go wrong.

Two of the rules above do most of the work here. Rule 2: the `redirect_uri`
sent to the token endpoint is the one stored when the authorization began, not
one re-derived from the callback's headers — decide on the canonical form, then
use the value you checked. Rule 3: the return path is checked against a closed
shape rather than against a list of hosts.

| Past failure | Structural decision in `uf` | Test |
| --- | --- | --- |
| A `state` parameter that is guessable, absent, or checked against something the attacker also controls — login CSRF, which ends with somebody signed into an account that is not theirs | 256 bits from `crypto.getRandomValues`, held server-side, compared in constant time against the record found through an `HttpOnly` cookie. The parameter alone proves the callback came from the provider; the cookie is what proves it came back to the browser that set out | `tests/library/oauth.test.js` |
| A replayed callback — the same `state` and code spent twice | The pending record is read with `SessionStore.take`, one store operation that reads *and* removes. A `read` followed by a `destroy` has a window between them, and a window is all a replay needs | `tests/library/oauth.test.js` |
| A subdomain planting a flow, or a session, in somebody else's browser | Both cookies are `__Host-` prefixed, `Secure`, `HttpOnly`, `Path=/` and `SameSite=Lax`. `Lax` is required rather than preferred: the provider's redirect back is a cross-site top-level navigation, and `Strict` would withhold the cookie on exactly that one. The name is decided once for the deployment rather than per request, so there is never a second name a reader might fall back to | `tests/library/oauth.test.js` |
| An intercepted authorization code being worth something | PKCE with `S256`, always. There is no configuration for it and `plain` is not offered: a `plain` challenge *is* the verifier, so anything that can read the authorization URL can spend the code. The verifier never leaves the server | `tests/library/oauth.test.js` |
| An open redirect on the way back in — `?return=//evil.example`, which wears this site's own domain | The return path must begin with `/` and not with `//` or `/\`, may hold no byte below `!` and no `DEL`, and is length-capped. Checked when it is stored and again when it is used, because what comes back from a store is what a store had | `tests/library/oauth.test.js` |
| `Host`-header poisoning steering the `redirect_uri`, so the authorization code is delivered somewhere else | The `redirect_uri` sent with the exchange is read out of the stored pending record, so the two requests cannot disagree about it. The origin it was built from is the deployment's configured one where there is one and the request's own `Host` otherwise — never `X-Forwarded-Host` | `tests/library/oauth.test.js` |
| Cross-site `POST` to an endpoint that changes state, authenticated by a cookie the browser attaches whether or not the caller meant it to | `Origin` is compared against `Host` — `new URL(request.url).host`, which is the `Host` header in every host `uf` ships — and against nothing else. A missing `Origin` is refused rather than allowed: every browser sends one on a `POST`, so a request without one is not a browser. No forwarded header is read anywhere in the flow | `tests/library/oauth.test.js` |
| Session fixation: an id fixed before the victim signs in and held afterwards | A sign-in issues a new id and destroys whatever session the request arrived with. A refresh deliberately does *not* rotate — two concurrent refreshes would each write a new id and the second would sign the person out — so rotation happens at the one moment fixation is possible | `tests/library/oauth.test.js` |
| A token in a response body, a shared cache, or a log line | Tokens stay in the store. `currentSession()` answers with the subject and the claims and never the tokens; `tokens()` is a separately named reader, so reaching for a credential is a decision. Every response from the flow carries `Cache-Control: no-store, private`, `Vary: Cookie` and `Referrer-Policy: no-referrer`, and no token, code or provider error text reaches a logger | `tests/library/oauth.test.js` |
| A page that reads who is signed in being served to the next visitor from the route cache | `currentSession()` reads `cookies()`, which counts as a read of request state — the same counter `packages/server/fetch.js` compares across the whole render before storing a document. A page about one person is never stored | `tests/library/oauth.test.js` |
| A client secret sent in the clear, or repeated back in a proxy log | The token endpoint is refused at configuration time unless it is `https`, or `http` on loopback, which is where every one of these is developed. The secret goes in an `Authorization: Basic` header rather than in the request body, which is the method RFC 6749 requires a server to accept | `tests/library/oauth.test.js` |
| An unbounded response from a compromised or hostile token endpoint | The body is read in chunks against a 64 KiB ceiling and the stream is cancelled above it, rather than being buffered whole by `response.json()` | `packages/server/internal/oauth.js` |
| An `expires_in` that is not a lifetime — `NaN`, an infinity, a negative number, or one far enough out to leave the range an instant can hold | The value is bounded before it is added to the clock, and anything outside it reads as "the provider did not say" rather than as a `RangeError` thrown out of the middle of a sign-in. A token endpoint is a third party and its answer is untrusted input, exactly as a browser's request is | `packages/server/oauth.js`, `tests/library/oauth.test.js` |
| Unverified OpenID Connect claims treated as an identity | `uf` does not decode an `id_token`. Verifying one means a JWKS fetch, a cache, an algorithm allow-list and a refusal of `alg: none`; half of that is worse than none of it, so the raw token is handed to the provider's own `identify` and `uf` claims nothing about it | — |
| Unbounded session storage as a memory exhaustion anybody can reach — every authorize request writes a record and nothing makes the browser come back to spend it | The built-in store has an entry ceiling and drops expired entries first, then the oldest. It is documented as one process's memory; the four-method interface is the deliverable and a durable store is an adapter's | `packages/server/internal/oauth.js` |

## Logging

A log is where attacker-chosen text is written and later read as a record of
what happened, and where credentials are published by accident. Both are
answered where the record is built rather than at each call site.

| Concern | Decision in `uf` | Where |
| --- | --- | --- |
| A credential in a log line. A token in the log store is read by more people, kept for longer, and replicated further than the store it came from | A closed table of field names whose value is never printed, matched after normalising case, `-`, `_` and `.`, and applied at every depth. A table of *names* rather than a test of values: recognising "this looks like a JWT" is a regex over untrusted input, which rule 5 forbids, and a heuristic that misses once has published the token | `packages/server/internal/log.js`, `tests/library/log.test.js` |
| Log injection — a newline in a path or a user agent closing its own record and opening a fabricated one | Every string in a record loses its control characters and is cut to a fixed length before it is formatted, the same treatment `uf_pm::progress` gives registry text before drawing it | `packages/server/internal/log.js`, `tests/library/log.test.js` |
| A query string in an access line: a return path, a search term, a signed URL, an OAuth `code` and `state` | The access line carries `url.pathname` and never the search. A host writing that line does not know which route it is logging, so the only rule it can apply is the one that is right for every route | `packages/server/log.js`, `tests/library/log.test.js` |
| A field named `level` or `msg` relabelling a record's own severity | The record's own keys are written after the fields, so a field of the same name cannot displace one | `packages/server/internal/log.js`, `tests/library/log.test.js` |
| An inbound `X-Request-Id` becoming the id `uf` correlates by | `uf` generates the request id and never takes it from the request. A client-chosen id can be identical on a million requests, which defeats the only thing an id is for, and it lands in a log line | `packages/server/internal/context.js` |
| A request id rendered into a document that is then cached, so every later visitor is told they are the first one | `requestId()` counts as a read of request state, exactly as `cookies()` does, so the route cache refuses to store the render. `logger()` deliberately does not: an id that reached a log line has not made the document personal | `packages/server/index.js`, `tests/library/log.test.js` |
| A log line written into `uf`'s own control channel | The default sink writes every level to `console.error`. In the process that runs `uf start` and `uf preview`, stdout is `@uniflowed/vite`'s JSON event channel and `console.info` goes to stdout on Node, so choosing the stream by level would put a log line in the middle of a protocol — intermittently, and only under traffic | `packages/server/internal/log.js` |
| A request Node's own parser refuses, answered `400` by the runtime and recorded nowhere | Every server uf ships — `serve`, and the compiled binary's — attaches a `clientError` handler that writes the same `400` the default one does and reports it at `warn` with the error's `code` and nothing else; Node puts the offending bytes on `error.rawPacket`, and those are whatever the client sent | `tests/library/log.test.js` |
| That same line as an amplifier: a malformed request is two dozen bytes to send, and one line to write is a disk and a retention window somebody else gets to spend | Twenty lines a minute per server, then a count of what the budget hid written once when the next window opens. A flood therefore costs a fixed number of lines and still says how big it was. The `400` is answered whether or not a line was written, because the budget bounds the log and not the protocol | `tests/library/log.test.js` |
| Unbounded work from a field a handler passed to a logger | Values are walked to a fixed depth with a fixed number of keys per object and entries per array, and strings are cut | `packages/server/internal/log.js`, `tests/library/log.test.js` |

## Package manager

`uf install` runs on a freshly cloned, untrusted repository. Everything in
`package.json` and every registry response is hostile input.

| Past failure | Structural decision in `uf` | Test |
| --- | --- | --- |
| [pnpm GHSA-6x96-7vc8-cm3p](https://github.com/pnpm/pnpm/security/advisories/GHSA-6x96-7vc8-cm3p) / CVE-2026-23889 — Windows backslash tarball path traversal | Archive entry names are validated as a closed grammar on all platforms; `\`, `..`, absolute paths, and drive-relative paths are rejected before any join | todo |
| [CVE-2026-82393](https://nvd.nist.gov/vuln/detail/CVE-2026-82393) — scoped path traversal through a tarball manifest `name`, overwriting arbitrary paths **even with `--ignore-scripts`** | The manifest `name` never becomes a filesystem path. Store layout is content-addressed by integrity hash, so the extraction path does not depend on attacker text at all | todo |
| Transitive dependency alias containing traversal segments, used as a link path | Aliases are validated with the same grammar as names, and links are created inside the store root with the root re-checked after resolution | todo |
| Tarballs from `codeload.github.com` not hash-pinned in the lockfile | Every resolved artifact carries an integrity hash in `uf.lock`; a source without one is a hard error, not a warning | todo |
| Binary planting through the `bin` field | `bin` targets are validated as single path segments inside the package, and shims are written only into the store's own bin directory | todo |
| `npx`-style execution of a package the project never installed | `uf exec` runs an installed binary from `node_modules/.bin` without ceremony, and **refuses** to fetch a name that is not in the lockfile unless the caller passes `--yes`. Fetching and running an unpinned package is strictly more dangerous than a `postinstall`, so it asks at least as loudly | `crates/uf_cli/tests/cli.rs` |
| Lifecycle scripts as an RCE vector | npm scripts are **forbidden by default** — `uf install` fails on a manifest that declares them, and `--ignore-scripts` goes to the manager so no *dependency* runs one either. `uf pm approve-builds` is the way back in, one package at a time; see below | `crates/uf_pm`, `uf_pm::builds` |
| Shell injection through the `packageManager` field | Parsed by a hand-written single-pass parser with no regex (ReDoS), and `Invocation.program` comes only from a fixed program table, so no manifest text can name a program or inject an argument | `uf_pm::detect` |
| Prototype-pollution keys in manifest JSON | `__proto__`, `constructor`, and `prototype` are reported and dropped wherever manifest JSON becomes a map | `uf_pm::detect` |
| Terminal escape sequences in a package name, injected into a progress display that steers the cursor | Every name taken out of a manager's output is stripped of control characters and length-capped before it can be drawn, and the redrawn region cuts each row to a fixed width, so no registry text can move the cursor | `uf_pm::progress` |
| Dependency confusion: a private name answered by the public registry | A scope bound in `pm.scopes` resolves from that registry and nowhere else — no fallback, because the fallback is the attack. `uf install` refuses a lockfile whose bound-scope package was resolved elsewhere, or that does not record where it came from at all | `uf_pm::confusion` |
| A tarball that is not the one anybody attested to | `uf install` reads the provenance attestation of every package that arrived or moved and refuses one whose subject digest is not the lockfile's; an attestation copied from another package, or rewritten under a real transparency-log entry, is refused with its own message | `uf_pm::provenance` |
| A registry read that goes to the wrong host because uf had one registry setting | `pm.registry` is the one uf reads from and `publish.registry` is where `uf publish` pushes. The old spelling still resolves, and says once which key to move to | `uf_config::UniflowedConfig::read_registry` |

### Dependency install scripts

A dependency with a `postinstall` script is arbitrary code, run on the machine
of everyone who installs it, before anybody has read a line of it. uf passes
`--ignore-scripts` to every manager by default, so none of them runs.

That is safe and it is not free: `esbuild`, `sharp` and everything else with a
native binary to place will not work until their build runs.
`uf pm approve-builds` lists what is waiting, with the hooks each package declares, and records the
ones you have read:

```
  package  version  runs         approved
  esbuild  0.24.0   postinstall  no
  sharp    0.33.5   postinstall  no

› 2 packages would run code at install time and are not approved, so uf does not let them run
```

The approved set goes in the root `package.json`, in the field the project's own
package manager already reads:

| manager | field |
| --- | --- |
| pnpm | `pnpm.onlyBuiltDependencies` |
| bun | `trustedDependencies` |
| yarn 2+ | `dependenciesMeta.<name>.built` |
| npm, yarn 1 | — |

Not a file of uf's own, and not a mirror of anybody's config schema: uf records
the decision and the manager enforces it, so a project that stops using uf keeps
a working allow-list and one that already had a list is read rather than
overridden. `--ignore-scripts` comes off only when the manager can enforce the
list *and* the list has something in it — an empty `onlyBuiltDependencies` is
pnpm's way of saying "none", and dropping the flag for it would turn that into
"whatever the manager defaults to".

**npm and Yarn 1 cannot do this.** `--ignore-scripts` is every script or none,
and there is no third answer to give it. uf keeps them off and says so, rather
than offering an approval that quietly means "and everything else too". Turning
them all on is `pm.allowLifecycleScripts: true` in `uf.config.js` — a deliberate
act with a deliberate spelling, which approves every dependency you have,
including the ones you have not read. `uf pm approve-builds` will not do it
for you.

Approving does not install. A security command that reached for the install the
moment you made the decision would be a command that runs the script you were
still thinking about.

## Config and plugins

`uf.config.js` is checked into the repository a developer just cloned, so every
value in it is hostile input. A plugin entry is the sharpest one: it decides
what code the toolchain executes.

| Past failure | Structural decision in `uf` | Test |
| --- | --- | --- |
| A config naming a plugin outside the project (`../../evil.js`, `/etc/…`, `~/…`, `file://…`) loads arbitrary code from the developer's machine | Plugin names are a closed grammar checked in one pass with no regex. Only a leading `./` names a file at all; absolute paths, URL schemes, drive letters, `~`, and `..` segments are typed errors, so there is exactly one place a config can reach the filesystem and it is guarded | `uf_plugin::resolve` |
| Windows-only separator handling lets `..\..\evil.js` through a check that only understood `/` | `\` is refused as a path separator on **every** platform, never only on Windows | `uf_plugin::resolve` |
| A symlink inside the project pointing out of it defeats a purely lexical containment check | The joined path is resolved and containment is re-checked against the canonical root, compared as path components rather than string prefixes | `uf_plugin::resolve` |
| Unbounded config text as a denial-of-service or allocation vector | Plugin names have an explicit byte ceiling and control bytes are refused, so no config text reaches a resolver as a NUL- or newline-bearing string | `uf_plugin::resolve` |
| A config plugin shadowing a built-in stage, silently replacing part of the toolchain | The `uf:` prefix is reserved, and two plugins with one name is a typed error that names both positions rather than a silent override | `uf_plugin::resolve` |

## Permissions the toolchain enforces

`uf test`, `uf transform`, `uf fmt`'s non-Flow delegation and the Vite driver
all execute JavaScript uf did not write — a test body, a plugin, a config file —
on a host that hands it the whole machine. A test that reads `~/.ssh` should
have to say so.

Deno's contribution to this class of tool was never the runtime; it was that a
program declares what it may reach and gets nothing it did not ask for. Node has
`--permission`, Bun has nothing, and uf runs on all three — so **uf owns the
model** and translates it, one `permissions` block in `uf.config.js` per
project. `docs/hosts.md` is the per-host table; this row is the decision behind
it.

| Concern | Decision in `uf` | Test |
| --- | --- | --- |
| A test body, plugin or config file reading anything the developer can — `~/.ssh`, `~/.aws`, `/etc` — because the toolchain starts its host with no restrictions at all | `permissions` in `uf.config.js` is a deny-by-default set that `uf test` puts in force on every worker. Declared entries are *added to* what uf itself needs to load and transform the project, so what the set denies is the rest of the machine rather than the project's own files — which is written down where it is implemented, because a reader who expected the other meaning would be surprised in the direction that matters | `crates/uf_cli/tests/permissions.rs` runs a real Flow test under Node's permission model and asserts the *body* saw `ERR_ACCESS_DENIED`, with a control run that asserts the same read succeeds without the block |
| A permission a host cannot enforce, accepted and quietly meaning something weaker — the failure mode this document's standard exists to prevent | Refused, naming the categories and a host that can. Node has no network or environment dimension at all and `--allow-child-process` is every program or none, so `net`, `env` and `run` stop the run there rather than being dropped; Bun has no model, so any declared set stops the run. Deno enforces all five | `uf_runtime::tests::permissions`, `crates/uf_cli/tests/permissions.rs` |
| A silent all-access grant surviving a declaration — Deno's `-A`, which uf passed unconditionally and which no later flag takes back | `HostCommand::with_permissions` removes `-A` rather than appending to it. Without that the run would read as sandboxed in `uf explain` and be wide open in fact, which is worse than the unsandboxed run it replaced | `uf_test::host::tests` |
| A typo in the block — `permissions: { nett: [...] }` — parsing to a set with no network at all, in a project that believes it declared one | `deny_unknown_fields` on this block alone. Every other section of `uf.config.js` ignores an unknown key, which is right where that means an option from a newer uf; here it means a permission nobody granted and nobody was told about | `uf_config::tests`, `crates/uf_cli/tests/permissions.rs` |
| An entry that a host's own argument syntax would split into a grant nobody wrote — `/tmp/a,/etc` under Deno's comma-separated `--allow-read` | Refused with the entry quoted. There is no quoting to reach for and dropping it would narrow the set without saying so, so the only answer that cannot widen it is to stop | `uf_runtime::tests::permissions` |
| The grants uf makes for itself being invisible, so nobody can check them | `uf explain test` prints, per permission, how many entries the project declared, how many uf added, and which flag enforces it. On Node the two additions are `--allow-worker` (the module hooks run on a loader thread) and `--allow-child-process` (every module is transformed by a `uf transform` child), neither of which Node can scope — so a test can still start a program, and `docs/hosts.md` says so in as many words rather than leaving it to be discovered | `crates/uf_cli/tests/permissions.rs` |

The install-script half of the same question is `pm.allowLifecycleScripts` and
`--ignore-scripts`, under [Dependency install scripts](#dependency-install-scripts).

**Only `uf test` is wired so far**, and that is the sharpest of the four — a
test body is code uf did not write, running on a host uf started, in a
repository somebody has just cloned. `uf transform`, `uf fmt`'s non-Flow
delegation and the Vite driver are not, and neither is the application at run
time (ubugeeei-prod/uf#535).

## Environment variables

A `.env` file holds the credentials a project's own developers put there, and it
is read by a tool that also produces a bundle anybody can download. Two
questions decide whether that is safe: which values cross into browser code, and
what a file in a repository you just cloned can make uf do.

| Concern | Decision in `uf` | Where |
| --- | --- | --- |
| A secret in a `.env` file inlined into the client bundle, where every visitor can read it | Only a name starting with the client prefix — `VITE_`, or Vite's own `envPrefix` when a project sets one — is substituted into browser code, and uf never widens that: it hands Vite the values through the process environment and Vite's `loadEnv` selects the prefixed subset. An empty prefix is refused, as Vite refuses it | `uf_config::env_files`, `crates/uf_cli/tests/vite.rs` asserts a non-prefixed value is absent from every file in `dist/` |
| A `.env` served over HTTP by the dev server | Vite's built-in `server.fs.deny` covers `.env` and `.env.*` and no project configuration removes a built-in entry | `packages/vite/driver.js` |
| A `.env` in a cloned repository steering the toolchain — `UF_BINARY` names the binary every module is transformed through | The project's values are applied to a child process *before* uf's own variables, so a file that names one is overwritten rather than obeyed | `uf_cli::commands::vite`, `uf_test::host` |
| A `.env` file outside the project — `env.files: ["~/.aws/credentials"]` in a cloned repository, or a committed symlink at `.env` — putting somebody's keys into every process uf starts, and any name in them behind the client prefix into the bundle | An `env.files` entry is a closed grammar checked before the filesystem is touched: no absolute path, no `..`, no `~`, no drive letter, and `\` refused on every platform. Every file uf actually opens is then resolved and checked against the canonical project root, by path component rather than string prefix, which is what a symlink defeats | `uf_config::env_files::check_entry`, `uf_config::env_files::contained` |
| A profile or mode that escapes the project — `uf env use ../../etc`, `uf build --mode ../secrets` | A mode is the end of a file name and is checked against a closed character set before anything is read or written; `local` is refused because `.env.local` already means something else | `uf_config::env_files::check_mode` |
| Unbounded file text as an allocation vector | Every file has a byte ceiling and a typed error above it, and the parser is one hand-written pass with no regex | `uf_config::env_files` |
| A credential printed into a log by a diagnostic | `uf inspect` reports the mode, the file names and the variable *names*; the banners report the mode and the files; a parse error names the line and the text before the `=`. No command prints a value | `uf_cli::commands::inspect`, `uf_config::env_files` |

## Parser, formatter, linter, and test runner

These read attacker-authored source text. The failure mode is denial of service
on a CI machine, or a formatter silently changing program meaning.

| Risk | Structural decision in `uf` | Test |
| --- | --- | --- |
| A hosted JavaScript engine budgeting its stack from wherever it was created, so parsing inside a work-stealing pool exhausts it on ordinary files — found in `uf lint` at a few hundred files, reported to the user as a syntax error in their own code | No hosted engine: Flow's Rust port parses on the calling stack | `uf_flow`, `uf_lint` |
| Stack overflow on deeply nested input | Depth is tracked on an explicit stack, never the call stack, and bounded | todo |
| Quadratic or exponential scanning | Single-pass lexing with byte scanning; no backtracking regex on source text | todo |
| A formatter that changes the token stream | Formatting is verified token-preserving: the lexer output of input and output must match, ignoring trivia | todo |
| Unbounded memory on a hostile file | File size caps with typed errors | `uf_rsc::scan`, `uf_pm::detect`, `uf_bundle::size`, `uf_transform::estree` |
| Non-UTF-8 and lone-surrogate input | Rejected at the boundary with a typed error; never sliced blindly | todo |

`uf` used to reach Flow's grammar through a QuickJS-hosted build of Flow's
JavaScript parser — an embedded C engine with its own internal limits, reached
through `libquickjs-sys`, and the default on stable toolchains. It is gone.
Meta's Flow Rust port removed the C dependency, the engine-internal failure
class above, and the source rewriting `uf` performed to feed a parser that
predated `component` syntax — which had put every diagnostic in a rewritten
file at the wrong location.

## Image and font transformation

`uf assets` decodes image and font files at build time. They arrive from the
project *and from its dependencies*, so every one of them is attacker-authored
input to a decoder, and a decoder is the classic place for a memory-safety bug.

| Risk | Structural decision in `uf` | Test |
| --- | --- | --- |
| A memory-safety bug in an image codec | Every decoder and encoder is pure Rust with no `unsafe` of uf's own and no C library linked: `png`, `zune-jpeg` and `image-webp` by way of `image`, chosen with `default-features = false` so no codec uf does not emit is compiled in at all | `uf_assets::image` |
| A decompression bomb — a small file declaring an enormous image | Dimensions are read from the header and checked against `MAX_SOURCE_PIXELS` *before* the buffer is allocated, and the file itself against `MAX_SOURCE_BYTES` | `uf_assets::image` |
| A font declaring a table directory or a table stream larger than the machine | `MAX_FONT_BYTES` and `MAX_TABLES`, and every decompression is `take`-bounded rather than trusted | `uf_assets::font` |
| A malformed WOFF2 length moving the reader off the entry boundary | `UIntBase128` refuses a leading zero, a value over 32 bits and a run longer than five bytes, exactly as the specification requires | `uf_assets::font` |
| A font family or file name breaking out of the generated CSS | Family names are emitted as escaped CSS strings; emitted file names are reduced to `[A-Za-z0-9_-]` and are one path segment by construction | `uf_assets::font`, `uf_assets::name` |
| A dev-server request reading outside the asset cache | The middleware serves one path segment and refuses any name containing a separator or `..`, rather than resolving a path and then checking where it landed | `@uniflowed/vite` |

**There is no request-time resize endpoint, and that is a decision rather than
an omission.** An endpoint that resizes whatever URL or dimensions a query
string names is a denial-of-service amplifier — a handful of requests can pin
every core on the machine — which is why Next.js pairs its optimizer with
`remotePatterns` as an allow-list. uf transforms only files a module actually
imports, at widths the project declared, at build time. A request-time path is
worth having for user-supplied and remote images, and when it is added it needs
the allow-list in the first commit rather than after one.

## Supply chain of `uf` itself

- Every GitHub Action is pinned to a commit SHA whose version comment resolves
  to a real Git ref, and `zizmor` runs at `pedantic` with `min-severity: low` on
  every pull request.
- `persist-credentials: false` on every checkout, so a compromised build step
  cannot reuse the workflow token.
- Publishing is tokenless OIDC trusted publishing on a `uf@*` tag push; the
  first publish is local and manual. So is moving the `latest` dist-tag after a
  prerelease release: npm exchanges the workflow's id-token for `npm publish`
  and nothing else, and putting a token in this repository's secrets to widen
  that would give away the property. See `tools/release/promote-latest.sh`.
- Every release archive is signed with keyless Sigstore from the release
  workflow's own OIDC identity, so there is no signing key to store or lose,
  and `install.sh` verifies the bundle against that identity before it unpacks
  anything. `UF_VERIFY_ORIGIN=require` in the release smoke job means a release
  whose signature does not verify never reaches a user.
- `upstream/flow` is pinned to a specific commit and is subject to the same
  review as any other dependency bump.
- `cargo-fuzz` builds on every pull request that touches the workspace, and
  `tools/legal` tracks the license of every built-in dependency.

## Checked against what actually happened

The tables above are structural decisions. This section is the other direction:
the published failures of the two frameworks uf is closest to, each one asked of
uf's own code, with the answer and where to verify it. A row here is a claim
about a specific file, not a posture.

| Their failure | uf's answer | Where |
| --- | --- | --- |
| **[CVE-2025-29927](https://nvd.nist.gov/vuln/detail/CVE-2025-29927)** — Next.js middleware bypassed by a request header (`x-middleware-subrequest`) the framework used to talk to itself | No request header steers control flow anywhere in the request path. The only one read at all is `Host`, and it sets the URL's authority rather than skipping a layer. There is no internal channel to forge because uf passes none | `packages/server/standalone.js`, `packages/server/fetch.js` |
| **[CVE-2024-34351](https://nvd.nist.gov/vuln/detail/CVE-2024-34351)** — Next.js server-action SSRF through a forged `Host` | A server action needs three things a cross-site caller cannot have: `POST` with a `uf-action` header (not a simple request, so it needs a preflight nothing answers), `content-type: application/json` (which no `<form>` can produce), and `Origin` equal to `Host`. `Origin: null` is refused rather than matched. `X-Forwarded-Host` is never consulted — a proxy that rewrites `Host` turns actions into `403`s, which is the right way to find out | `packages/router/internal/action-endpoint.js` |
| **[CVE-2023-46298](https://nvd.nist.gov/vuln/detail/CVE-2023-46298)** — Next.js cached a personalised SSR response and served it to everybody | The route cache stores nothing when the render read the request, nothing when the response carries `Set-Cookie`, and nothing for a status that is not `200`. The "read the request" test is a comparison of request-state reads across the render, so a component inside a `<Suspense>` boundary that reads a cookie counts as much as the shell | `packages/server/fetch.js`, `tests/library/cache.test.js` |
| **[CVE-2025-32421](https://nvd.nist.gov/vuln/detail/CVE-2025-32421)** — Next.js cache confusion between a page and its data route | uf has no parallel data route to confuse a page with, and the key is the method, the path and the search string together | `packages/server/internal/cache-key.js` |
| Absolute URLs built from a forged `Host` and then cached, so a poisoned entry advertises the attacker's origin in `canonical` and `og:url` | `metadataBase` is a site-wide setting, not a per-request value: a route module cannot see the host it is served from, so there is nothing per-request to poison | `packages/router/internal/runtime.js` |
| **[CVE-2021-37699](https://nvd.nist.gov/vuln/detail/CVE-2021-37699)** — open redirect from a path the framework normalised | A redirect is a status and a `Location` the application wrote; uf synthesises none from user input | — |
| **[CVE-2018-6341](https://nvd.nist.gov/vuln/detail/CVE-2018-6341)** — React DOM server-side attribute injection | The renderer is React's own, and uf adds no attribute path around it. `Metadata` values reach `<meta content>` as React children, which React escapes | `packages/router/internal/runtime.js` |
| Path traversal in static file serving | Decoded, NUL refused, `path.resolve`, and then required to be the root or under it — the check after normalisation rather than before | `packages/server/node.js` |
| **Symlink escape** from a served directory | The file that is opened is realpath'd and required to be inside the realpath'd root, so a link that reads as inside and points outside is a 404 — indistinguishable from a file that is not there. A link that stays inside still works | `packages/server/node.js`, `tests/library/serve.test.js` |

## Supply chain

Where the code comes from, asked with the same directness.

| Attack | uf's answer | Where |
| --- | --- | --- |
| A dependency runs code at install time | `--ignore-scripts` to every manager, always, by default. Getting one package back is `uf pm approve-builds <name>`, recorded in the field the project's own manager reads; the flag comes off only when the manager can enforce a list *and* the list has something in it | `crates/uf_pm/src/builds.rs` |
| A package's own manifest declares scripts | `uf install` refuses the workspace before anything is fetched. Project automation is `uf.config.js` tasks | `crates/uf_pm` |
| A tarball's bytes are not the bytes that were resolved | Every artefact carries an integrity hash in `uf.lock`; a source without one is a hard error. SHA-1 is refused outright — a check that can be forged is a check in name only | `crates/uf_env/src/archive.rs` |
| Fetching and running a package the project never installed | `uf exec` refuses a name that is not in the lockfile without `--yes`. It is strictly more dangerous than a `postinstall`, so it asks at least as loudly | `crates/uf_cli/tests/cli.rs` |
| A registry read leaks credentials | Packument reads are HTTPS only — `http://` refused, loopback included — a URL with an authority is refused outright, and curl is given `--proto =https --proto-redir =https` so a `301` cannot undo either | `crates/uf_pm/src/registry.rs` |
| uf's own npm publish uses a long-lived token | It does not have one. Publishing is OIDC trusted publishing, bound to `publish.yml`; there is no npm token in the repository or its secrets | `.github/workflows/publish.yml` |
| A compromised GitHub Action | Every action is pinned to a commit SHA and `zizmor` gates the workflows in CI | `.github/workflows/` |
| A compromised release host | The release workflow signs every archive with keyless Sigstore, and `install.sh` verifies the bundle against the certificate identity of `release.yml` in this repository and GitHub's OIDC issuer — a root that is not on the release host. A bundle that verifies proves **origin**; the SHA-256 beside the archive proves **transit** and never proved more. A signature that is present and does not verify refuses the install and prints cosign's own reason, because a signature by somebody else and a `cosign` too old to read the bundle exit alike and are not alike. Where `cosign` is not installed, `UF_CHECKSUM_BASE` gives a second opinion from a host that did not serve the archive; the release's own smoke job runs `UF_VERIFY_ORIGIN=require`, so a uf release cannot ship without an origin anybody can check | `infra/cloudflare/setup-assets/install.sh`, `tools/release/test-install.sh` |
| A package published by a taken-over account | `uf install` reads the npm provenance attestation of every package it brought in or moved, and **refuses** one that is not about the tarball being installed. `uf pm approve-builds` shows the same in an `attested` column, at the moment a package is about to be allowed to run code. Absence is reported, never refused — most of npm publishes none. What this proves and does not prove is below, and the signature itself is still unverified: [#552](https://github.com/ubugeeei-prod/uf/issues/552) tracks the rest | `uf_pm::provenance` |
| Dependency confusion | `pm.scopes` binds a scope to a registry, and a name in a bound scope is resolved from that registry **and nowhere else** — there is no fallback to the public one, because the fallback *is* the attack. `uf install` refuses a lockfile that resolves a bound scope from anywhere else, before the manager runs and again on what it wrote, naming the package, the bound registry and the one that answered | `uf_pm::confusion`, `uf_pm::registry` |

A row that says "not answered yet" is the point of both tables: a list where
every row says "handled" is a list nobody checked. The three that used to be
here are closed by the section below, which is careful to say how far.

### What a signature and an attestation actually prove

Two of the answers above are partial, and a partial answer stated as a whole
one is worse than the gap it papers over.

**`install.sh`, the archive.** The SHA-256 beside the archive proves *transit*:
the bytes on the machine are the bytes that host advertised, which catches a
truncated download, a corrupting proxy or a cache serving half a file. It
proves nothing about *origin*, and never did — the archive and the checksum
come from the same host, so whoever can replace one can replace the other. The
Sigstore signature is what proves origin, verified against a certificate
identity and an OIDC issuer that the release host does not control. Where
`cosign` is absent the second opinion is weaker and is described as such: two
operators agreeing about the bytes, not who built them.

One thing is deliberately not yet true: the installer's default is
`UF_VERIFY_ORIGIN=auto`, which **reports** an install nothing vouched for rather
than refusing it. Every release published before signing existed carries no
bundle, and a default of `require` would make each of them uninstallable by the
current script — so the strict default waits until every installable release is
signed, and until then the release smoke job is what holds the line.

**`uf install`, a package.** uf checks that an attestation *binds to the
artefact in front of it*: the DSSE envelope is an in-toto statement and carries
a signature at all; the subject is this package at this version, compared as a
decoded purl rather than as a string; the subject's SHA-512 is the SHA-512 the
lockfile pins; and where the bundle carries a Rekor entry, the payload hash
that entry records is the SHA-256 of the statement actually served. Between
them those refuse an attestation copied from another package, one whose digest
was edited to match a swapped tarball, and a statement rewritten underneath a
genuine transparency-log entry.

**uf does not verify the Sigstore signature on an attestation.** That needs an
ECDSA verification against a Fulcio certificate, its chain, and its OIDC
identity extensions — a cryptographic stack uf does not link, and will not grow
inside a package-manager module by accident. Until it does, a registry that is
*itself* hostile can serve a self-consistent attestation naming any repository
it likes and the checks above will pass. What is closed today is the attacker
who can replace an artefact but not rewrite the whole attestation around it,
and the reporting that makes an absent attestation visible at all — which is
the signal a taken-over publishing account produces: a package that had
provenance and stops having it. [#552](https://github.com/ubugeeei-prod/uf/issues/552)
stays open for the signature.

**The confusion check reads the lockfile, not the wire.** It proves every
package in a bound scope was resolved from the registry the project said that
scope lives on. It does not prove the bound registry is honest, and it says
nothing about a scope nobody bound. A lockfile format uf does not parse as a
tree — pnpm's YAML, yarn's own syntax, bun's binary format — yields no
findings, which is an absence of evidence rather than evidence of absence.

## Reporting

Security reports go to the repository's private vulnerability reporting. Do not
open a public issue for a suspected vulnerability.
