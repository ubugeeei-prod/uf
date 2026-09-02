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
| [CVE-2025-30208](https://github.com/advisories/GHSA-x574-m823-4x7w) — `?raw??` and `?import&raw??` bypass `server.fs.deny` | Query suffixes are stripped and the request is canonicalized to an absolute path *before* any access decision; the decision is made on the resolved path only | todo |
| [CVE-2025-31125](https://nvd.nist.gov/vuln/detail/CVE-2025-31125) — `?import` / `?inline` / `?raw` traversal | Loader selection is a table lookup over a closed enum, not a string suffix match | todo |
| [CVE-2025-32395](https://nvd.nist.gov/vuln/detail/CVE-2025-32395) — invalid `request-target` bypass | Requests whose target is not a valid origin-form path are rejected before routing, not normalized into one | todo |
| [CVE-2025-62522](https://nvd.nist.gov/vuln/detail/CVE-2025-62522) — Windows trailing backslash bypass | `\` is treated as a path separator on every platform when deciding access, never only on Windows | todo |

The dev server binds loopback by default. Exposing it requires an explicit
opt-in, and that opt-in must also require an allowed-origin list rather than
defaulting to `*`.

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

## Parser, formatter, linter, and test runner

These read attacker-authored source text. The failure mode is denial of service
on a CI machine, or a formatter silently changing program meaning.

| Risk | Structural decision in `uf` | Test |
| --- | --- | --- |
| A hosted JavaScript engine budgeting its stack from wherever it was created, so parsing inside a work-stealing pool exhausts it on ordinary files — found in `uf lint` at a few hundred files, reported to the user as a syntax error in their own code | `uf_flow::prepare_thread` creates each worker's engine from a shallow frame before any nested work begins | `uf_lint` |
| Stack overflow on deeply nested input | Depth is tracked on an explicit stack, never the call stack, and bounded | todo |
| Quadratic or exponential scanning | Single-pass lexing with byte scanning; no backtracking regex on source text | todo |
| A formatter that changes the token stream | Formatting is verified token-preserving: the lexer output of input and output must match, ignoring trivia | todo |
| Unbounded memory on a hostile file | File size caps with typed errors | `uf_rsc::scan`, `uf_pm::detect`, `uf_bundle::size` |
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
