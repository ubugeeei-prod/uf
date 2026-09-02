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

The Vite dev server has been bypassed four separate ways in one year, all
variations on "the deny check ran against a string that was not the path that
was eventually opened."

| Past failure | Structural decision in `uf` | Test |
| --- | --- | --- |
| [CVE-2025-30208](https://github.com/advisories/GHSA-x574-m823-4x7w) — `?raw??` and `?import&raw??` bypass `server.fs.deny` | Query suffixes are stripped and the request is canonicalized to an absolute path *before* any access decision; the decision is made on the resolved path only | `uf_devserver::target`, `uf_devserver::resolve`, `attack_corpus` |
| [CVE-2025-31125](https://nvd.nist.gov/vuln/detail/CVE-2025-31125) — `?import` / `?inline` / `?raw` traversal | Loader selection is a table lookup over a closed enum, not a string suffix match | `uf_devserver::target`, `attack_corpus` |
| [CVE-2025-32395](https://nvd.nist.gov/vuln/detail/CVE-2025-32395) — invalid `request-target` bypass | Requests whose target is not a valid origin-form path are rejected before routing, not normalized into one | `uf_devserver::target`, `attack_corpus` |
| [CVE-2025-62522](https://nvd.nist.gov/vuln/detail/CVE-2025-62522) — Windows trailing backslash bypass | `\` is treated as a path separator on every platform when deciding access, never only on Windows | `uf_devserver::resolve`, `attack_corpus` |
| Traversal, double encoding, poisoned `%00`, and symlinks out of the project root | One pipeline: percent-decode once (a still-encoded result is rejected, not decoded again), normalize lexically, resolve symlinks, decide, then open. `ResolvedFile` carries the open handle and exposes the approved path only as a non-path `CheckedPath`, so a caller has nothing to re-derive | `uf_devserver::resolve`, `attack_corpus` |
| Vite's `/@fs/` absolute-path prefix, the entry point for the four rows above | There is no such prefix. A request whose first normalized segment is `@fs` is a typed refusal, so the absence is asserted rather than incidental | `uf_devserver::resolve`, `attack_corpus` |
| A deny list weakened by project configuration | `dev.fs.deny` entries are *added* to a built-in list (`.env*`, `**/.git/**`, `*.pem`, `*.key`, `*.crt`, `**/.uf/**`) and cannot remove one. Patterns are matched by a two-pointer globber with a single backtrack point — no regex, no exponential blow-up | `uf_devserver::policy`, `attack_corpus` |
| DNS rebinding, and cross-origin requests with side effects | Loopback bind by default; `--host` fails to start without a non-empty `dev.allowedHosts`, with exposure read from the *bound socket* rather than the config string. A `Host` outside the list is refused, and anything that is not a simple `GET`/`HEAD` needs an `Origin` in `dev.allowedOrigins`. `*` is rejected in either list | `uf_devserver::network`, `uf_devserver::server`, `attack_corpus`, `uf_cli` |
| Inbound headers that steer dispatch — [CVE-2025-29927](https://nvd.nist.gov/vuln/detail/CVE-2025-29927)'s class, at the dev server's own surface | `RequestHead` retains the method, the request target, `Host` and `Origin`, and nothing else: there is no header map for a handler to consult. The two retained headers can only refuse a request, never select a root, loader, handler, or path | `uf_devserver::http`, `attack_corpus` |
| `Last-Event-ID`, the resume cursor the server-sent-events specification defines, letting a client choose what the update stream sends — the same bug class, at the surface most likely to reproduce it | The hot-reload stream is served by the same listener behind the same `Host`/`Origin` allowlists. Its response head is a `&'static [u8]` with no substitution point and no `Access-Control-Allow-Origin`, and a subscriber's cursor is assigned by the server at `subscribe`. `no_inbound_header_changes_what_the_update_stream_sends` drives `Last-Event-ID`, `x-middleware-subrequest` and friends and asserts every response is byte-identical | `uf_devserver::hmr::channel`, `uf_devserver::server` |
| Hot module replacement as a second way to reach a file — an update payload naming a path the request pipeline would refuse | An update carries origin-form *request targets*, built by `update_target` from already-normalized module paths and re-parsed under the same grammar an inbound target is parsed with; a path it cannot spell becomes a full reload, not a target. Fetching is `fetch_update`, which is `resolve_with_policy` behind a name. `an_hmr_fetch_is_refused_exactly_like_a_plain_request` drives the whole corpus, `../../.env` included, down both paths and asserts the refusals are equal | `uf_devserver::hmr::update`, `attack_corpus` |
| A file watcher announcing a path the server would never serve, turning the update channel into a disclosure of what exists | The poll watcher shares the dev server's `FsPolicy` deny list, watches `.js` only, skips dot-directories and a fixed table of build directories, and refuses to follow a symlink. Editing `.env` produces no update at all | `uf_devserver::hmr::watch` |
| Unbounded work from a hostile project tree: a directory nested to the filesystem's limit, an import cycle, a module graph with no ceiling | Every walk is an explicit worklist with a per-side seen set — no recursion in the graph, the invalidation, or the watcher — and each has a typed bound: `MAX_MODULES`, `MAX_MODULE_DEPTH`, `MAX_MODULE_IMPORTS`, `MAX_MODULE_BYTES`, `MAX_WATCH_DEPTH`, `MAX_WATCHED_FILES`, `MAX_SUBSCRIBERS`, `MAX_BUFFERED_UPDATES`. Exceeding the invalidation depth degrades to a reported full reload rather than a hang | `uf_devserver::hmr::graph`, `uf_devserver::hmr::invalidate`, `uf_devserver::hmr::watch`, `uf_devserver::hmr::channel` |

`attack_corpus` is `crates/uf_devserver/tests/attack_corpus.rs`: a table of
request targets that must never produce a file, each row naming the trick it
plays and the status the server must answer with. Every new bypass anyone
thinks of becomes a row there before it becomes a fix.

The dev server binds loopback by default. Exposing it requires `uf dev --host`
*and* a non-empty `dev.allowedHosts`; a cross-origin request with side effects
additionally requires an `Origin` in `dev.allowedOrigins`. Neither list has a
default, neither accepts `*`, and a server that cannot name its allowed hosts
does not start.

What the dev server does today is serve static files under the project root,
answer `/__uf/health`, and stream hot-module-replacement updates on
`/__uf/hmr`. The loaders it names (`?raw`, `?inline`, `?url`, `?worker`) are
selected from the closed enum and reported on the response, but do not yet
transform the body; when they do, the transform must consume the `ResolvedFile`
rather than re-open anything.

The update stream is a second *surface*, not a second *server*: same listener,
same port, same `Host` and `Origin` allowlists, and the modules an update names
are fetched back through `resolve_with_policy` like any other request. The only
new refusal it introduces is a subscriber ceiling, answered with `503`.

## Framework, RSC, and server actions

| Past failure | Structural decision in `uf` | Test |
| --- | --- | --- |
| [CVE-2025-29927](https://nvd.nist.gov/vuln/detail/CVE-2025-29927) — spoofing `x-middleware-subrequest` skips middleware, bypassing auth | No inbound request header participates in middleware dispatch. Recursion control is internal state, never a header a client can send | todo |
| Server Action endpoint IDs globally disclosed | Action ids are keyed hashes of (module path, export name, build id), so they are neither guessable nor stable across builds; an action not reachable from a client boundary is never registered as an endpoint | `uf_rsc::action` |
| RSC cache poisoning when a shared cache does not partition response variants ([CVE-2026-44576](https://nvd.nist.gov/vuln/detail/CVE-2026-44576)) | Route, fetch, action, and data caches are **off by default**. When enabled, the RSC variant is part of the cache key, and the response carries the matching `Vary` | todo |
| Server-code leak: a `"use client"` module importing server-only code | The RSC graph rejects the edge at build time as an error, not a warning | `uf_rsc::graph` |
| Directive parsing bugs — `"use client"` accepted when not the first statement, or built from a template literal | The directive is only recognized as a plain string literal in leading directive position; everything else is a typed diagnostic | `uf_rsc::directive` |
| `"use server"` export that is not an async function | Rejected at build time; React's calling convention makes this a correctness *and* a safety issue | `uf_rsc::graph` |
| SSRF via WebSocket upgrade ([CVE-2026-44578](https://nvd.nist.gov/vuln/detail/CVE-2026-44578)) | Upgrade targets are resolved against an allowlist; no request-derived value selects an upstream host | todo |
| Image optimizer: unbounded disk cache, CPU exhaustion from remote images, cache deception | Image caching is opt-in, remote sources require an explicit host allowlist, decode work is bounded by pixel budget, and the cache has a size ceiling | todo |
| XSS via CSP nonce handling and `beforeInteractive` scripts | Nonces are generated per response and never reused across a cached response; script injection points are typed, not string-concatenated | todo |

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
| Lifecycle scripts as an RCE vector | npm scripts are **forbidden by default** — `uf install` fails on a manifest that declares them. Project automation lives in `uf.config.js` tasks | `crates/uf_pm` |
| Shell injection through the `packageManager` field | Parsed by a hand-written single-pass parser with no regex (ReDoS), and `Invocation.program` comes only from a fixed program table, so no manifest text can name a program or inject an argument | `uf_pm::detect` |
| Prototype-pollution keys in manifest JSON | `__proto__`, `constructor`, and `prototype` are reported and dropped wherever manifest JSON becomes a map | `uf_pm::detect` |

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

## Parser, formatter, linter, and test runner

These read attacker-authored source text. The failure mode is denial of service
on a CI machine, or a formatter silently changing program meaning.

| Risk | Structural decision in `uf` | Test |
| --- | --- | --- |
| A hosted JavaScript engine budgeting its stack from wherever it was created, so parsing inside a work-stealing pool exhausts it on ordinary files — found in `uf lint` at a few hundred files, reported to the user as a syntax error in their own code | `uf_flow::prepare_thread` creates each worker's engine from a shallow frame before any nested work begins | `uf_lint` |
| Stack overflow on deeply nested input | Depth is tracked on an explicit stack, never the call stack, and bounded | todo |
| Quadratic or exponential scanning | Single-pass lexing with byte scanning; no backtracking regex on source text | todo |
| A formatter that changes the token stream | Formatting is verified token-preserving: the lexer output of input and output must match, ignoring trivia | todo |
| Unbounded memory on a hostile file | File size caps with typed errors | `uf_rsc::scan`, `uf_pm::detect`, `uf_bundle::size`, `uf_devserver::hmr::graph` |
| Non-UTF-8 and lone-surrogate input | Rejected at the boundary with a typed error; never sliced blindly | todo |

The QuickJS-hosted Flow parser is a temporary backend and a liability in this
section: it is an embedded C engine with its own internal limits, reached through
`libquickjs-sys`. Meta's Flow Rust port removes both the C dependency and that
class of engine-internal failure, which is a security argument for the switch
independent of it being ~376x faster.

## Supply chain of `uf` itself

- Every GitHub Action is pinned to a commit SHA whose version comment resolves
  to a real Git ref, and `zizmor` runs at `pedantic` with `min-severity: low` on
  every pull request.
- `persist-credentials: false` on every checkout, so a compromised build step
  cannot reuse the workflow token.
- Publishing is tokenless OIDC trusted publishing on a `uf@*` tag push; the
  first publish is local and manual.
- `upstream/flow` is pinned to a specific commit and is subject to the same
  review as any other dependency bump.
- `cargo-fuzz` builds on every pull request that touches the workspace, and
  `tools/legal` tracks the license of every built-in dependency.

## Reporting

Security reports go to the repository's private vulnerability reporting. Do not
open a public issue for a suspected vulnerability.
