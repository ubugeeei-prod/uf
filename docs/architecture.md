# Architecture

## Product Shape

uniflowed is the Unified Toolchain for Flow (React): a zero-config toolchain
that gives Flow React apps a single fast native interface. The CLI surface is
intentionally broad, while each engine is isolated behind small Rust crate
boundaries so we can deepen behavior without destabilizing the user-facing
command model.

The product bar is deliberately aggressive: `uf` should beat Vite+ on Flow
React DX, framework completeness, build latency, and dev-server feedback loops,
while using Vite Task for cached task execution and beating Bun Test/Vitest on
native test throughput, runtime startup, package manager performance, and
integrated feature coverage.

## Crates

- `uf_cli`: command router for `uf`
- `uf_config`: zero-config defaults and `uf.config.js` loading
- `uf_bundle`: bundle size measurement and `build.budgets` enforcement
- `uf_check`: Flow type inference, driven from `upstream/flow`
- `uf_flow`: Flow parser/typechecker adapter boundary over `upstream/flow`
- `uf_fmt`: native formatter runner
- `uf_infra`: Arena, FxHash, PHF, SIMD UTF-8, SmallVec, CompactString
- `uf_lib`: native standard library registry exposed to Flow
- `uf_lint`: native lint runner and framework rules
- `uf_pm`: self-hosted package manager plan, lockfile, and content-addressed store contracts
- `uf_project`: project discovery and `create` templates
- `uf_rm`: runtime manager inference, host detection, and adapter application contracts
- `uf_runtime`: Capability JS Host, WinterTC, and deploy-anywhere runtime contract
- `uf_std`: native stdlib modules for WinterTC-compatible Flow wrappers
- `uf_test`: native test discovery, scheduling, watch invalidation, and runner core
- `uf_transform`: Flow → JavaScript — official parser, Flow's lowering rules, the official React Compiler, oxc for JSX and code generation
- `uf_tui`: OpenTUI-compatible native TUI framework contracts

These are the crates that survive. `uf` used to ship a Rust crate per library
surface — `uf_motion`, `uf_orm`, `uf_markdown`, `uf_temporal`, `uf_web` and ten
more — each holding a `serde` struct that described a feature rather than
implementing one, and each duplicating a `packages/*/index.js` module
that was the real thing. Effect and validator are now plain `.js` + Flow
packages rather than Rust crates. Twelve had no consumer at all; three existed so
`uf inspect` could print their `::default()`. They are gone: the JavaScript in
`uf_lib/lib/core` is the source of truth for what those modules are.

## Flow Syntax Authority

`uf` never reimplements Flow's grammar, and it does not host one either.
`uf_flow` is a thin adapter over exactly one backend: Meta's Flow Rust port at
`upstream/flow/rust_port/crates/flow_parser`, vendored through the
`upstream/flow` submodule. It is the same code Flow itself runs, and it parses
`component`/`hook`/`renders`/`match`, modern variance (`readonly`, `in`, `out`)
and `extends` bounds natively.

There is no second backend and no feature flag selecting one. A build of `uf`
that spoke a different dialect would make `uf lint` and `uf check` disagree with
the grammar uf documents, which is exactly what happened while a QuickJS-hosted
build of Flow's JavaScript parser stood in for the port on stable toolchains:

- it predated component syntax, so `uf` rewrote the user's source before parsing
  and reported every diagnostic against the rewritten text;
- its AST deserialization predated `readonly`, so writing the only spelling Flow
  accepts for a read-only property crashed `uf lint`;
- it budgeted a 256 kB stack from wherever its runtime was created, so linting a
  few hundred valid files in parallel failed as `SyntaxError: stack overflow` —
  scaling with parallelism rather than input size.

Removing it also removes an embedded JavaScript engine, and the `libquickjs-sys`
C dependency with it, from the release binary.

### What the port can and cannot give us yet

Measured against `rustc 1.100.0-nightly (5db7f4be8 2026-09-01)`:

- **The parser's own nightly requirement is expiring.** `flow_parser`'s only
  unstable feature is `never_type`, and that compiler already reports it as
  *stable since 1.100.0-nightly*. The type checker below is what keeps the
  whole workspace on a pinned nightly.
- **The type checker builds and runs, on a pinned nightly.** 23 crates in the
  port, including `flow_common` and everything under `flow_typing*`, declare
  `#![feature(box_patterns)]`, and that feature was *removed* from the compiler
  around the 2026-09-01 nightly — so the typing crates fail on the floating
  `nightly` channel. They compile on `nightly-2026-08-01` (rustc 1.99.0-nightly),
  which is what `rust-toolchain.toml` pins for the whole workspace. The pin is
  not a preference: `uf` parses and type-checks Flow with the official port, and
  no stable toolchain can build it. The `Upstream Flow` CI job builds the parser
  on the floating channel as an early warning, so the pin moves deliberately
  rather than being discovered when it breaks.

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

Five crates are excluded from that check: `uf_flow`, `uf_check` and
`uf_transform` name the submodule, and `uf_cli` and `uf_lint` reach it
transitively, because cargo resolves a path dependency whether or not the
feature using it is enabled. That leaves most crates gated, which is worth more
than switching the gate off — but the list grows as more crates use the parser,
and it already includes three of the more interesting public APIs.

If it grows to where the gate covers little, the fix is to make the upstream
dependency a rev-pinned git dependency, which cargo resolves from any directory,
and keep the submodule for reading. That would also undo the other three costs
the submodule has charged: `cargo fmt --all` visiting vendored code, every CI job
needing `tools/upstream/sync.sh` before cargo will parse the workspace at all,
and the `include_str!` paths reaching outside `rust_port`. The trade is that the
built code and the readable checkout stop being the same bytes by construction.

## Flow To JavaScript

Every host runs JavaScript, and `uf` projects are Flow. `uf_transform` is the
one place that turns one into the other, and it is assembled from the code
that owns each step rather than from anything `uf` invented. There is no Babel
in the pipeline.

| Step | Implementation |
| --- | --- |
| Parse | Meta's Flow Rust port (`flow_parser`), rendered as ESTree by its own translator |
| Lower `component`/`hook`, `match`, enums; erase types | Ports of `hermes-parser`'s `TransformComponentSyntax`, `TransformMatchSyntax`, `TransformEnumSyntax` and `StripFlowTypes` — the rules Flow's own toolchain applies |
| Babel AST + scopes | A port of `hermes-parser`'s `TransformESTreeToBabel`, plus a scope analysis in Babel's terms, because that is the contract the compiler consumes |
| React Compiler | The official Rust implementation (`react_compiler` on crates.io) in `syntax` mode: only `component` and `hook` declarations are memoised |
| JSX, Fast Refresh, code generation, source maps | oxc — the engine inside Vite and Rolldown — so a module is byte-identical whether Vite or `uf test` asked for it |

The output is the JavaScript Flow documents: a `match` becomes the
`typeof x === "object" && x !== null` and `"k" in x` tests Flow specifies, a
`component Foo(a: A, ...rest: R)` becomes `function Foo({ a, ...rest })`, and
an enum becomes a frozen object with the `flow-enums-runtime` contract
(`cast`, `isValid`, `members`, `getName`), with the runtime prepended to the
module so no import is needed.

`uf transform` serves this as a long-lived process: newline-delimited JSON in,
replies in request order out. `@uniflowed/vite`, the Node loader hook and the
Bun preload all speak that protocol, and every one of them applies the same
module policy first (`is_flow_module`): project `.js` files and `@uniflowed/*`
under `node_modules` are uf's to transform, a third-party dependency is not,
and a build driver's own virtual modules never are.

Source maps point at the Flow source. The printer records a mapping for every
node the author wrote and none for nodes the compiler or the lowering passes
invented, and oxc's map over the printed text is composed with those, so a
debugger lands on the author's line or nowhere — never on the wrong line.

The compiler's panic threshold is `none`: a function it cannot compile is left
as written and reported as a diagnostic on the reply, never a failed build.

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
query/effect APIs, Relay, `cell`, headless UI, hooks, and React
Native-compatible entry files.

Server Components are the default. Client Components must opt in with
`"use client";`, and server action modules must opt in with `"use server";`.
Caches are off by default. React 19, Suspense, `use`, and Async React are
assumed.

The linter starts with framework rules that guide teams away from legacy React
function component typing and toward Flow component syntax. React Native support
starts with platform split diagnostics for generic files that branch on
`Platform.OS` or `Platform.select`.

## Runtime Agnostic Direction

`uf_lib` follows the Bun-style shape for builtin modules, but the user project
runtime is deliberately host-agnostic. Native Rust owns toolchain work —
configuration, linting, Flow parsing/type checking, Flow formatting, test
scheduling, package metadata, and builtin binding contracts — while ordinary
JavaScript execution is delegated to a Capability JS Host.

The zero-config host set is Node.js, Deno, and Bun. `uf.config.js` names the
default host and the accepted host set once, and `@uniflowed/rm` detects and
applies that host instead of installing a bespoke runtime. The self-hosted
Hermes-backed `uf` runtime is still documented as a later line, but it is no
longer the default direction for app execution.

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

- query cache and mutation scheduler, aimed at replacing TanStack Query for
  Flow React applications
- generator/yield EffectSystem inspired by Redux-Saga but typed for Flow
- explicit fetch clients without global fetch override
- Relay-based GraphQL client primitives
- Valibot-class validator utilities exposed as `@uniflowed/validator` with
  `v.pipe`
- Jotai-class atom state primitives exposed through `@uniflowed/state` and
  `@uniflowed/cell`
- DOM and React Native testing utilities compatible with Testing Library habits
- self-hosted `@uniflowed/test` runner targeting faster-than-Bun execution
- ORM schema/runtime with Flow opaque types at module boundaries
- StyleX compiler/runtime integration
- React Compiler syntax-mode integration
- Relay integration
- headless UI components with preset styles, validator-backed form contracts,
  RSC split metadata, and `renders` type utilities, aimed at replacing shadcn's
  copy-and-edit workflow with typed imports
- MSW-compatible mocks, Playwright-compatible browser automation, story system,
  and VRT baseline planning
- React Compiler-safe motion primitives with reduced-motion defaults
- React hook utilities that preserve render idempotency and cover the practical
  VueUse-style browser/state/async hooks a React app reaches for
- OpenTUI-aligned terminal UI primitives targeting a faster, richer React Ink
  replacement with native cell-diff rendering and in-memory tests
- stdlib contracts for OS, net, DNS, path, streams, URL, WebAssembly, glob, TUI,
  cron, S3, SigV4, worker/lambda functions, UUID, and ZIP utilities
- host-provided event loop and IO capability mapping for Node.js, Deno, and Bun
- deferred WinterTC-aligned Flow runtime backed by Hermes
- deploy-anywhere adapters for Node.js, Deno, Bun, edge, serverless, static, and
  container targets in a Nitro-like model

## Build And Dev

`uf.config.js` mirrors the Vite style because it replaces the user-authored
`vite.config.ts`. Vite *is* the dev server, the bundler and the plugin system;
`uf` owns the Flow-specific config surface, the generated route and RSC data,
the Rust lint/typecheck/format/test work, and the transform every module goes
through. Users never write `vite.config.*`.

`uf dev` and `uf build` start `@uniflowed/vite`'s driver on the project's
Capability JS Host — Node.js, Bun or Deno, whichever `uf.config.js` names and
the machine has — and keep the terminal: the driver writes one JSON event per
line and `uf` renders them. The driver loads `uf.config.js` (through `uf
transform`, since the config is Flow), builds Vite's inline config from it,
and registers uf's plugins:

- `uf:flow` pipes every Flow module through `uf transform`, adds the React
  Fast Refresh wiring in development, and serves the virtual modules that make
  a directory of pages an application — the route table generated from `app/`,
  the client entry that hydrates it, and the server entry that renders it. In
  development it renders every document request on the server, so `uf dev`
  serves the markup `uf build` writes.
- `uf:mdx` is `@mdx-js/rollup` with GitHub-flavoured markdown, front matter and
  heading ids, so `_uf.page.mdx` works with no configuration.

A build is three passes: the client bundle (with a manifest, so the renderer
knows which script and stylesheet tags to write), the server bundle (kept
under `.uf/build/server/`, never in `dist/`), and every static route
prerendered to `dist/<route>/index.html` — with `generateStaticParams` on a
page enumerating a parameterised route. `uf` then measures `dist/` and
enforces `build.budgets`.

`@uniflowed/router` is the runtime the virtual modules call into: matching
(`[param]`, `[...rest]`, `(group)`, most specific wins), nested layouts,
`loader` data embedded for hydration, `metadata` hoisted into `<head>`,
client-side navigation with `Link` prefetching on intent, and `notFound()`/
`redirect()`. A page or layout exports its component as `default` or as the
named `Page`/`Layout` that `uf create` scaffolds.

Generated projects do not use npm scripts. Tasks are declared in
`uf.config.js` and executed by `uf run` through Vite Task.

Editor integrations live under `editors/` and should stay thin. VS Code,
Neovim, Emacs, Vim, Helix, Zed, and Cursor all connect to `uf lsp`; the Rust
workspace remains responsible for parsing, linting, formatting, route type
generation, and diagnostics.

Native package output follows a napi-rs-style target model. The generated
TypeScript declaration files are converted into Flow declaration files so the
repository and published library surface remain Flow-first.

`uf publish` writes the local/trusted publishing manifest used to bootstrap the
first release locally. After trusted publishing is configured from the CLI,
`uf release alpha` computes the next `uf@*` tag metadata and GitHub Actions
publishes through OIDC without a long-lived npm token.

`@uniflowed/pm` owns package resolution, `uf.lock`, a content-addressed store,
and script-free install policy. `@uniflowed/rm` reads `uf.config.js`, infers the
required Capability JS Host, applies host adapters, and feeds `uf env doctor`
with runtime checks. Explicit `uf use uf@...` remains available for the
postponed self-hosted runtime line, but zero-config apps do not depend on it.

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
