# Architecture

## Product Shape

uniflowed is the Unified Toolchain for Flow (React): a zero-config toolchain
that gives Flow React apps a single fast native interface. The CLI surface is
intentionally broad, while each engine is isolated behind small Rust crate
boundaries so we can deepen behavior without destabilizing the user-facing
command model.

The product bar is deliberately aggressive: `uf` should beat Vite+ on Flow
React DX, framework completeness, build latency, and dev-server feedback loops,
while beating Bun on native test runner throughput, runtime startup, package
manager performance, and integrated feature coverage.

## Crates

- `uf_cli`: command router for `uf`
- `uf_config`: zero-config defaults and `uf.config.js` loading
- `uf_browser`: Playwright-compatible browser and VRT contracts
- `uf_bundle`: bundle size measurement and `build.budgets` enforcement
- `uf_check`: Flow type inference, driven from `upstream/flow`
- `uf_fetch`: explicit ofetch-style client contracts
- `uf_flow`: Flow parser/typechecker adapter boundary over `upstream/flow`
- `uf_fmt`: native formatter runner
- `uf_graphql`: Relay-based GraphQL client contracts
- `uf_infra`: Arena, FxHash, PHF, SIMD UTF-8, SmallVec, CompactString
- `uf_lib`: native standard library registry exposed to Flow
- `uf_lint`: native lint runner and framework rules
- `uf_loader`: flow-cell-backed data loading contracts
- `uf_markdown`: ox-content wasm-backed markdown contracts
- `uf_mock`: MSW-compatible mock handler contracts
- `uf_motion`: React Compiler-safe motion contracts
- `uf_package`: napi-rs-style target package generation and TS-to-Flow declaration conversion
- `uf_pm`: self-hosted package manager plan, lockfile, and content-addressed store contracts
- `uf_project`: project discovery and `create` templates
- `uf_rm`: runtime manager inference, acquisition, and adapter application contracts
- `uf_runtime`: WinterTC, Hermes, native event-loop, and deploy-anywhere runtime contract
- `uf_state`: native state and flow-cell primitives
- `uf_story`: story and visual regression contracts
- `uf_std`: native stdlib modules for WinterTC-compatible Flow wrappers
- `uf_stdlib_cli`: Gunshi-style stdlib CLI framework for Flow
- `uf_temporal`: lite Temporal contracts
- `uf_test`: native test discovery, scheduling, watch invalidation, and runner core
- `uf_tui`: OpenTUI-compatible native TUI framework contracts
- `uf_validator`: valibot-style native validator primitives
- `uf_vrt`: native visual regression contracts
- `uf_web`: Nuxt-like web primitives and typed route hooks

## Flow Syntax Authority

`uf` never reimplements Flow's grammar. `uf_flow` is a thin adapter over exactly
one backend at a time:

| Backend | Feature | Toolchain | Notes |
| --- | --- | --- | --- |
| Meta's Flow Rust port | `upstream-parser` | nightly | `upstream/flow/rust_port/crates/flow_parser`, parses `component`/`hook`/`renders`/`match` natively |
| Reference parser in QuickJS | `official-parser` (default) | 1.98.0 | Flow's OCaml parser compiled to JavaScript, needs source rewriting for component syntax |
| Guard | none | any | compile error surface only, no real grammar |

The upstream port is the target: it is the same code Flow itself runs, it is
native Rust rather than JavaScript interpreted in an embedded engine, and it
removes the `quick-js` C dependency from the release binary. It is gated behind a
feature only because the port still uses the unstable `!` type, so it needs
nightly for now. Both real backends report
`ParserKind::OfficialFlowParser` because they implement the same grammar;
`active_backend()` reports which implementation a build selected, and
`upstream-parser` always wins when both are enabled.

### What the port can and cannot give us yet

Measured against `rustc 1.100.0-nightly (5db7f4be8 2026-09-01)`:

- **The parser works, and its nightly requirement is expiring.** `flow_parser`'s
  only unstable feature is `never_type`, and that compiler already reports it as
  *stable since 1.100.0-nightly*. When the toolchain pin reaches 1.100 stable,
  `upstream-parser` becomes the default with no nightly involved — a one-line
  change to `uf_flow`'s default features.
- **The type checker builds and runs, on a pinned nightly.** 23 crates in the
  port, including `flow_common` and everything under `flow_typing*`, declare
  `#![feature(box_patterns)]`, and that feature was *removed* from the compiler
  around the 2026-09-01 nightly — so the typing crates fail on the floating
  `nightly` channel. They compile on `nightly-2026-08-01` (rustc 1.99.0-nightly),
  which is what the type-check feature pins.

  Measured on that toolchain: Flow's builtin library definitions merge into a
  master context in **68 ms** (once, then cached), and checking a file costs
  about **4 ms**. Assigning a string to a `number`, passing a `string` where a
  `number` is declared, and dereferencing a `?string` are each reported; a
  well-typed file reports nothing.

  Two things an embedder has to know, neither of them documented upstream:
  `flow_parser::file_key::{set_project_root, set_flowlib_root}` are
  process-global and panic on first use if unset, and
  `flow_parsing::docblock_parser::Docblock` is private, so the context metadata
  has to be computed where the docblock is parsed rather than carried around.

`flow_flowlib` embeds Flow's library definitions with `include_str!` paths that
reach outside `rust_port` into `lib/`, `prelude/`, and `tslib/`, so
`tools/upstream/sync.sh` checks those out too and asserts they arrived.

The submodule costs one gate. `cargo-semver-checks` builds its baseline from a
copy of each crate, outside the workspace, where the relative path to
`upstream/flow` no longer resolves — and a path dependency is relative by
definition, so no crate can fix it from its own manifest.

Five crates are excluded from that check: `uf_flow` and `uf_check` name the
submodule, and `uf_bundler`, `uf_cli` and `uf_lint` reach it transitively,
because cargo resolves a path dependency whether or not the feature using it is
enabled. That leaves 37 of 42 crates gated, which is worth more than switching
the gate off — but the list grows as more crates use the parser, and it already
includes three of the more interesting public APIs.

If it grows to where the gate covers little, the fix is to make the upstream
dependency a rev-pinned git dependency, which cargo resolves from any directory,
and keep the submodule for reading. That would also undo the other three costs
the submodule has charged: `cargo fmt --all` visiting vendored code, every CI job
needing `tools/upstream/sync.sh` before cargo will parse the workspace at all,
and the `include_str!` paths reaching outside `rust_port`. The trade is that the
built code and the readable checkout stop being the same bytes by construction.

## Type Checking

`uf_check` is the embedding of Flow's own inference, behind the
`upstream-typecheck` feature. It is a separate crate rather than a feature on
`uf_flow` for one reason: `uf_lint` depends on `uf_flow`, Cargo resolves path
dependencies whether or not the feature that uses them is on, and inference
needs sixteen path dependencies on the submodule plus a pinned nightly. Keeping
them here means exactly one crate — and, through it, `uf_cli` — carries that
weight.

| Concern | Where it lands |
| --- | --- |
| Toolchain | `nightly-2026-08-01`, pinned by the `Upstream Flow Typecheck` CI job |
| Default build | feature off; `uf check` is the linter alone and `uf` still builds on 1.98.0 |
| Diagnostics | typed: severity, Flow's error code, primary and root spans, message fragments, and every location the message references |
| Bounds | `Options::recursion_limit`, `CheckBudget`, a 4 MiB source cap, and a 1 GiB check stack |

Measured on that toolchain, optimized: builtins merge in **19 ms** cold and cost
nothing warm; a dense Flow React component file checks in **4.3 ms**
(230 files/s, one thread). Unoptimized those are 60 ms and 15 ms.

What it does not do yet is resolve modules. Every file is checked against Flow's
standard library — `react` and everything else the library definitions declare —
but `uf` does not run Flow's merge service, so an import of another project file
has no signature to check against. Those resolve to Flow's own *unchecked
module*, which types the import as `any` and lets the rest of the file check, and
the specifiers are reported in `CheckReport::untyped_modules` so the hole is
stated rather than silent. Cross-module inference is the next step.

Errors are never flattened into strings. `flow_common_errors`'s accessors give
the code, kind, and primary location directly; the message tree itself is private
to that crate, so `json_output`'s v2 rendering is walked once to recover the
message fragments and the locations they point at, and each fragment is mapped
back onto a typed segment. Two embedding details worth knowing: Flow's renderer
panics on a relative path and reads a location's file from disk to build a
codepoint offset table, so `uf_check` resolves every path against a synthetic
absolute root that cannot exist — the read always misses, and the columns stay in
the **bytes** that `uf_term`'s code frames and `uf_lint` both measure in.

## Flow And React

The default app preset is Flow-first React. New app templates use Flow component
syntax, `app.js`, file-system routes, server actions, StyleX,
query/effect APIs, Relay, `flow-cell`, headless UI, hooks, and React
Native-compatible entry files.

Server Components are the default. Client Components must opt in with
`"use client";`, and server action modules must opt in with `"use server";`.
Caches are off by default. React 19, Suspense, `use`, and Async React are
assumed.

The linter starts with framework rules that guide teams away from legacy React
function component typing and toward Flow component syntax. React Native support
starts with platform split diagnostics for generic files that branch on
`Platform.OS` or `Platform.select`.

## Native Runtime Direction

`uf_lib` follows the Bun-style shape: native Rust engines expose a compact
Flow module surface. The initial package is declaration-first, then the runtime
loader can map `@uniflowed/*` modules to native implementations.

User-authored Flow source uses `.js` files with `// @flow`, and so do the
published `@uniflowed/*` packages: there are no `.js.flow` declaration files.
A shipped module owns its own declarations, raises only when a native binding is
actually called, and runs nothing at import time, so `"sideEffects": false` and
per-subpath exports let a bundler drop everything an application never touches.

Implemented native slices already cover:

- zero-config `.js` + `// @flow` project generation without npm scripts
- router discovery and generated `router.js` route types
- `uf build` metadata emission through `dist/uf-build-manifest.json`
- Rust-native `uf dev` HTTP state and health endpoint
- `uf install` workspace discovery, `uf.lock`, store manifest, and
  content-addressed package entries
- `uf use` local current-binary runtime activation through XDG directories
- `uf upgrade` package/runtime manifest generation
- `ufx` native execution for known `@uniflowed/*` package entrypoints
- `uf publish` and `uf release` metadata generation for trusted publishing
- source-level native `uf test` execution for the first assertion subset,
  scheduled longest-first across a rayon pool from durations recorded in
  `.uf/test-timings.json`, with `--watch` re-running only the test files an edit
  transitively invalidates
- stdio JSON-RPC `uf lsp` initialize capabilities

Native engines being deepened:

- query cache and mutation scheduler, similar in capability to TanStack Query
- generator/yield EffectSystem inspired by Redux-Saga but typed for Flow
- explicit fetch clients similar to ofetch, without global fetch override
- Relay-based GraphQL client primitives
- validator utilities exposed as `@uniflowed/validator` with `v.pipe`
- state primitives exposed through `@uniflowed/state` and `@uniflowed/flow-cell`
- DOM and React Native testing utilities compatible with Testing Library habits
- self-hosted `@uniflowed/test` runner targeting faster-than-Bun execution
- ORM schema/runtime with Flow opaque types at module boundaries
- StyleX compiler/runtime integration
- React Compiler syntax-mode integration
- Relay integration
- headless UI components with preset styles, validator-backed form contracts,
  RSC split metadata, and `renders` type utilities
- MSW-compatible mocks, Playwright-compatible browser automation, story system,
  and VRT baseline planning
- React Compiler-safe motion primitives with reduced-motion defaults
- React hook utilities that preserve render idempotency
- OpenTUI-aligned terminal UI primitives targeting a faster, richer React Ink
  replacement with native cell-diff rendering and in-memory tests
- stdlib contracts for OS, net, DNS, path, streams, URL, WebAssembly, glob, TUI,
  cron, S3, SigV4, worker/lambda functions, UUID, and ZIP utilities
- Rust-native server with owned request handling, streaming, and libuv-level IO
- WinterTC-aligned Flow runtime backed by Hermes
- default `uf` runtime with Node.js, Bun, Deno, edge, serverless, static, and
  container deployment adapters in a Nitro-like deploy-anywhere model

## Build And Dev

`uf.config.js` mirrors the Vite style because the build and dev pipeline is
expected to reuse Vite-compatible plugin semantics and Rolldown where it gives us
the best performance. Users should not need `vite.config.*` or `rolldown`
configuration files; those engines stay internal to uniflowed.

Generated projects do not use npm scripts. Tasks are declared in
`uf.config.js` and executed by `uf run` through the Vite Task-compatible runner.

Editor integrations live under `editors/` and should stay thin. VS Code,
Neovim, Emacs, Vim, Helix, Zed, and Cursor all connect to `uf lsp`; the Rust
workspace remains responsible for parsing, linting, formatting, route type
generation, and diagnostics.

Native package output follows a napi-rs-style target model. The generated
TypeScript declaration files are converted into Flow declaration files so the
repository and published library surface remain Flow-first.

`uf publish` writes the local/trusted publishing manifest used to bootstrap the
first release locally. After trusted publishing is configured from the CLI,
`uf release minor` computes the next `uf@*` tag metadata and GitHub Actions
publishes through OIDC without a long-lived npm token.

`@uniflowed/pm` owns package resolution, `uf.lock`, a content-addressed store,
and script-free install policy. `@uniflowed/rm` reads `uf.config.js`, infers
the required runtime, acquires it, applies host adapters, and feeds `uf env
doctor` with runtime checks.

Runtime manager paths follow the XDG Base Directory layout: config in
`XDG_CONFIG_HOME`, runtime data and versions in `XDG_DATA_HOME`, cache in
`XDG_CACHE_HOME`, durable state in `XDG_STATE_HOME`, runtime sockets under
`XDG_RUNTIME_DIR` when available, and the `uf` shim under the user-local bin
directory. The target POSIX installer is
`curl -fsSL https://setup.uniflowed.dev | sh`; shell targets include sh, bash,
zsh, and ush, and platform targets include Windows, macOS, and Linux.

## Performance Defaults

The default Rust toolbox is centralized in `uf_infra`:

- `Bump` arenas for short-lived parse/lint/format work
- `FxHashMap` and `FxHashSet` for hot maps
- `phf` for static keyword and rule tables
- `memchr` and `simdutf8` for text scanning fast paths
- `SmallVec` for short diagnostic/export vectors
- `CompactString` for small identifiers and module specifiers

## Testing Strategy

Every crate should keep focused unit tests close to the behavior it owns. CLI
tests should verify that the public interface remains coherent. As engines become
real, CI should prefer GitHub Actions for full verification because it is faster
and closer to the merge gate.

Benchmark coverage should exist for every hot path: config loading, router
discovery, parser diagnostics, lint scanning, formatting, and test discovery.
