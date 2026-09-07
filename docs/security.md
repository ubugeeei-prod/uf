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
| A different `uf` on PATH transforming the project's modules | The driver is told which binary started it (`UF_BINARY`) and every transform goes through that one — including the ones answered from `.uf/cache/transform`, whose entries are keyed by the size and modification time of the binary that wrote them, so a different or rebuilt `uf` misses rather than inherits, and a host that cannot identify its binary caches nothing at all | `uf_cli::commands::vite`, `packages/host/transform.js`, `packages/host/internal/node-hooks.js` |
| A rebuilt `uf` reporting the type errors the previous one found | `.uf/cache/check` entries are keyed by the size and modification time of the binary that wrote them, alongside the limits the check ran under and a digest of every signature the file depends on; a rebuilt `uf`, a changed limit, or a moved declaration in a dependency all miss rather than inherit, and a process that cannot identify its own binary caches nothing at all. A record that is truncated, of the wrong shape, or about another file is a miss | `uf_check::cache`, `uf_check::tests::cache` |
| A cache directory growing until the disk is full, and the sweep that stops it | The three answer caches — `.uf/cache/check`, `.uf/cache/transform` and `.uf/cache/task` — are each bounded at 128 MiB and swept to 96 MiB, coldest entry first. The sweep only ever unlinks — it never truncates or rewrites, so a concurrent reader sees a whole entry or none — it never removes a file used in the last minute, which is what keeps it away from a temporary another process is about to rename into place, and it does not follow symbolic links, so a link planted in a cache directory cannot make `uf` delete what it points at | `uf_infra::cache` |
| Unbounded work from a hostile module | Every stage of the transform has a ceiling — source size, tree depth — and a typed error above it; the transform service runs on a thread with a fixed large stack so a pathological input fails with a message | `uf_transform` |
| Inbound headers that steer dispatch — [CVE-2025-29927](https://nvd.nist.gov/vuln/detail/CVE-2025-29927)'s class | Vite's middleware chain is the only dispatch, and `uf` adds one middleware: render a document for a `GET`/`HEAD` whose `Accept` includes `text/html` and whose path has no file extension. It reads nothing else from the request | `packages/vite/index.js` |
| The build's own notes served to whoever guesses the filename | `uf-build-manifest.json`, `uf-rsc-manifest.json` and `uf-bundle-report.json` are written to `.uf/build/meta/` rather than into the output directory. Between them they name every route including the ones that were never prerendered, the source file behind each, and the size of every chunk; nothing reads them to answer a request. This is not a denylist in uf's two servers — the file is not there, so a static host has nothing to serve either | `uf_cli::commands::build`, `uf_cli::tests::vite` |

## Framework, RSC, and server actions

| Past failure | Structural decision in `uf` | Test |
| --- | --- | --- |
| [CVE-2025-29927](https://nvd.nist.gov/vuln/detail/CVE-2025-29927) — spoofing `x-middleware-subrequest` skips middleware, bypassing auth | No inbound request header participates in middleware dispatch: which middleware runs is decided by the request path against the directory each one guards, and by nothing else. Recursion control is internal state, never a header a client can send | `tests/library/middleware.test.js` |
| Server Action endpoint IDs globally disclosed | Action ids are keyed hashes of (module path, export name, build id), so they are neither guessable nor stable across builds; an action not reachable from a client boundary is never registered as an endpoint, and the table the endpoint dials into is built from the manifest's callable set rather than from anything a request carries | `uf_rsc::action`, `tests/library/server-actions.test.js` |
| A deserializer that reconstructs attacker-chosen objects — the class every "RCE through a serialization format" advisory is | A server action's arguments are plain JSON data and nothing else, under the closed grammar below. No tag in a payload names a constructor, a module, a function or a reference, so there is nothing for a payload to *become* | `tests/library/server-actions.test.js` |
| Prototype pollution through a request body — `__proto__` as an own property, which `JSON.parse` produces and the next spread applies | `__proto__`, `constructor` and `prototype` are refused as keys anywhere in a payload, in the one place a payload is decoded, rather than left for each action to remember | `tests/library/server-actions.test.js` |
| CSRF against a state-changing endpoint: a cross-site page posting an action with the visitor's cookies | Three independent guards, any one of which would do. The call carries `uf-action`, which is not a header a simple request may set, so a cross-origin caller needs a preflight and uf answers none; the content type must be `application/json`, which no `<form>` can produce; and `Origin` must be present and equal `Host`. `Host` alone, never `X-Forwarded-*` — a forwarded header is a string the caller wrote, so a proxy in front of a uf application has to preserve `Host` and one that rewrites it turns every action call into a `403` | `tests/library/server-actions.test.js`, `crates/uf_cli/tests/vite.rs` |
| An action endpoint used as an enumeration oracle | Every failed lookup is the same `404` with the same body, and the table is scanned whole with a constant-time comparison, so neither the answer nor the time says whether the id existed. The payload is decoded *before* the id is resolved, so a malformed body cannot be used to tell a real id from a guess | `tests/library/server-actions.test.js` |
| An application's internals in an error response — a message, a name, a stack | An action that throws is a `500` with a fixed body; the exception goes to the host's error reporting. The same in development as in production, because `uf dev` and `uf build` have to agree about what this endpoint answers | `tests/library/server-actions.test.js` |
| Unbounded work from a request body: an enormous payload, deep nesting, non-UTF-8 bytes | A byte ceiling counted as the body arrives rather than trusted from `Content-Length`, a depth ceiling, a value-count ceiling, an argument-count ceiling, and `TextDecoder(…, { fatal: true })` so invalid UTF-8 is refused rather than replaced. The walk that applies them is iterative, so the sender's hand is not on the stack depth | `tests/library/server-actions.test.js` |
| Server code reaching the browser through an action module — a database handle, a secret, `node:async_hooks` | A `"use server"` module is replaced, in the client graph only, by one reference per callable export. The RSC graph colours it server for the same reason, so the analysis and the bundle agree about it rather than about each other | `crates/uf_cli/tests/vite.rs`, `uf_rsc::graph` |
| An action's arguments or result that cannot cross a wire at all, arriving as `{}` | `uf prepare` writes the wire grammar into the generated `server-actions.js` as a bound over every action in the project, so an action taking a callback or returning a `Map` is a `uf check` error | `tests/type-tests/server-actions.js` |
| The build's whole module graph published at `/uf-rsc-manifest.json` — module paths, export names, diagnostics | The manifest is written into the output directory and served with it. The action ids in it are the ones already in the client bundle, so it discloses no endpoint that was not disclosed anyway; the module graph beside them is disclosure with no reader | todo |
| RSC cache poisoning when a shared cache does not partition response variants ([CVE-2026-44576](https://nvd.nist.gov/vuln/detail/CVE-2026-44576)) | Route, fetch, action, and data caches are **off by default**. When enabled, the RSC variant is part of the cache key, and the response carries the matching `Vary` | todo |
| Server-code leak: a `"use client"` module importing server-only code | The RSC graph rejects the edge at build time as an error, not a warning | `uf_rsc::graph` |
| Directive parsing bugs — `"use client"` accepted when not the first statement, or built from a template literal | The directive is only recognized as a plain string literal in leading directive position; everything else is a typed diagnostic | `uf_rsc::directive` |
| `"use server"` export that is not an async function | Rejected at build time; React's calling convention makes this a correctness *and* a safety issue | `uf_rsc::graph` |

### The argument boundary

A `"use server"` export is a public HTTP endpoint the moment it exists, and the
one decision that makes it a feature rather than a remote-code-execution
surface is what the bytes on the wire are allowed to become. It is this, and
`packages/router/internal/action-wire.js` is the only place that applies it:

> **A server action's arguments are plain JSON data and nothing else.** One
> JSON object, `{"args": [...]}`, of at most 1 MiB of valid UTF-8, holding at
> most 16 values, nested at most 24 deep, with at most 10,000 values in total.
> Each of those is `null`, a boolean, a finite number, a string, an array of
> them, or a plain object whose keys are ordinary strings and are none of
> `__proto__`, `constructor` or `prototype`. The result travels back under
> exactly the same grammar, plus `undefined` for an action that returns
> nothing.

Nothing in a payload can name a function, a module, a class, a prototype, a
React element, an id or a reference, and nothing in it is revived into an
object the sender chose. What the decoder adds to `JSON.parse` is the refusal
of everything `JSON.parse` would have let through.

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
- **`FormData` and `<form action={fn}>`.** Multipart parsing is its own attack
  surface with its own bounds, and a form post is a *simple* cross-origin
  request — it reaches a server with the visitor's cookies and no preflight.
  Both are worth having; neither is worth having by accident.

The grammar is enforced twice, and the second time is what makes it a
*contract* rather than a runtime check: Flow holds every action's parameters
and return value against it at build time, through the two bounds `uf prepare`
writes into `server-actions.js`, and the endpoint applies it again to whatever
actually arrives.

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
| `npx`-style execution of a package the project never installed | `uf exec` runs an installed binary from `node_modules/.bin` without ceremony, and **refuses** to fetch a name that is not in the lockfile unless the caller passes `--yes`. Fetching and running an unpinned package is strictly more dangerous than a `postinstall`, so it asks at least as loudly | `crates/uf_cli/tests/cli.rs` |
| Lifecycle scripts as an RCE vector | npm scripts are **forbidden by default** — `uf install` fails on a manifest that declares them, and `--ignore-scripts` goes to the manager so no *dependency* runs one either. `uf pm approve-builds` is the way back in, one package at a time; see below | `crates/uf_pm`, `uf_pm::builds` |
| Shell injection through the `packageManager` field | Parsed by a hand-written single-pass parser with no regex (ReDoS), and `Invocation.program` comes only from a fixed program table, so no manifest text can name a program or inject an argument | `uf_pm::detect` |
| Prototype-pollution keys in manifest JSON | `__proto__`, `constructor`, and `prototype` are reported and dropped wherever manifest JSON becomes a map | `uf_pm::detect` |
| Terminal escape sequences in a package name, injected into a progress display that steers the cursor | Every name taken out of a manager's output is stripped of control characters and length-capped before it can be drawn, and the redrawn region cuts each row to a fixed width, so no registry text can move the cursor | `uf_pm::progress` |

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
| **A compromised release host** | **Not answered yet.** `install.sh` verifies a SHA-256 that it downloads from the same host as the archive, so it proves transit and not origin. [#551](https://github.com/ubugeeei-prod/uf/issues/551) | — |
| **A package published by a taken-over account** | **Not answered yet.** npm publishes provenance attestations and uf reads none of them; an integrity hash says the bytes are what uf resolved, not that they came from the source the package claims. [#552](https://github.com/ubugeeei-prod/uf/issues/552) | — |
| **Dependency confusion** | **Not answered yet.** A scope cannot be bound to a registry, so a private name has nothing stopping a public answer. [#553](https://github.com/ubugeeei-prod/uf/issues/553) | — |

The rows that say "not answered yet" are the point of both tables. A list where
every row says "handled" is a list nobody checked, and the three above are open
issues rather than sentences.

## Reporting

Security reports go to the repository's private vulnerability reporting. Do not
open a public issue for a suspected vulnerability.
