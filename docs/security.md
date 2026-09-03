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
| A different `uf` on PATH transforming the project's modules | The driver is told which binary started it (`UF_BINARY`) and every transform goes through that one | `uf_cli::commands::vite`, `packages/vite/internal/transform.js` |
| Unbounded work from a hostile module | Every stage of the transform has a ceiling — source size, tree depth — and a typed error above it; the transform service runs on a thread with a fixed large stack so a pathological input fails with a message | `uf_transform` |
| Inbound headers that steer dispatch — [CVE-2025-29927](https://nvd.nist.gov/vuln/detail/CVE-2025-29927)'s class | Vite's middleware chain is the only dispatch, and `uf` adds one middleware: render a document for a `GET`/`HEAD` whose `Accept` includes `text/html` and whose path has no file extension. It reads nothing else from the request | `packages/vite/index.js` |

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
