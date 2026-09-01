# Architecture

## Product Shape

uniflowed is the Unified Toolchain for Flow (React): a zero-config toolchain
that gives Flow React apps a single fast native interface. The CLI surface is
intentionally broad, while each engine is isolated behind small Rust crate
boundaries so we can deepen behavior without destabilizing the user-facing
command model.

## Crates

- `uf_cli`: command router for `uf`
- `uf_config`: zero-config defaults and `uf.config.flow` loading
- `uf_browser`: Playwright-compatible browser and VRT contracts
- `uf_fetch`: explicit ofetch-style client contracts
- `uf_flow`: Flow parser/typechecker adapter boundary
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
- `uf_test`: native test discovery core
- `uf_validator`: valibot-style native validator primitives
- `uf_vrt`: native visual regression contracts
- `uf_web`: Nuxt-like web primitives and typed route hooks

## Flow And React

The default app preset is Flow-first React. New app templates use Flow component
syntax, `app.flow`, file-system routes, server actions, StyleX,
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

Planned native engines:

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
- stdlib contracts for OS, net, DNS, path, streams, URL, WebAssembly, glob, cron,
  S3, SigV4, worker/lambda functions, UUID, and ZIP utilities
- Rust-native server with owned request handling, streaming, and libuv-level IO
- WinterTC-aligned Flow runtime backed by Hermes
- default `uf` runtime with Node.js, Bun, Deno, edge, serverless, static, and
  container deployment adapters in a Nitro-like deploy-anywhere model

## Build And Dev

`uf.config.flow` mirrors the Vite style because the build and dev pipeline is
expected to reuse Vite-compatible plugin semantics and Rolldown where it gives us
the best performance. Users should not need `vite.config.*` or `rolldown`
configuration files; those engines stay internal to uniflowed.

Generated projects do not use npm scripts. Tasks are declared in
`uf.config.flow` and executed by `uf run` through the Vite Task-compatible runner.

Editor integrations live under `editors/` and should stay thin. VS Code,
Neovim, Emacs, Vim, Helix, Zed, and Cursor all connect to `uf lsp`; the Rust
workspace remains responsible for parsing, linting, formatting, route type
generation, and diagnostics.

Native package output follows a napi-rs-style target model. The generated
TypeScript declaration files are converted into Flow declaration files so the
repository and published library surface remain Flow-first.

`uf publish` is planned to bootstrap the first release locally. After trusted
publishing is configured from the CLI, `uf release minor` should create a
`uf@*` tag and GitHub Actions should publish through OIDC without a long-lived
npm token.

`@uniflowed/pm` owns package resolution, `uf.lock`, a content-addressed store,
and script-free install policy. `@uniflowed/rm` reads `uf.config.flow`, infers
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
