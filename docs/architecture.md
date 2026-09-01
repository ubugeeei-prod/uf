# Architecture

## Product Shape

uniflowed is the Unified Toolchain for Flow (React): a zero-config toolchain
that gives Flow React apps a single fast native interface. The CLI surface is
intentionally broad, while each engine is isolated behind small Rust crate
boundaries so we can deepen behavior without destabilizing the user-facing
command model.

## Crates

- `uniflowed_cli`: command router for `uf`
- `uniflowed_config`: zero-config defaults and `uniflowed.config.flow` loading
- `uniflowed_flow`: Flow parser/typechecker adapter boundary
- `uniflowed_fmt`: native formatter runner
- `uniflowed_infra`: Arena, FxHash, PHF, SIMD UTF-8, SmallVec, CompactString
- `uniflowed_lib`: native standard library registry exposed to Flow
- `uniflowed_lint`: native lint runner and framework rules
- `uniflowed_project`: project discovery and `create` templates
- `uniflowed_test`: native test discovery core

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

`uniflowed_lib` follows the Bun-style shape: native Rust engines expose a compact
Flow module surface. The initial package is declaration-first, then the runtime
loader can map `@uniflowed/*` modules to native implementations.

Planned native engines:

- query cache and mutation scheduler, similar in capability to TanStack Query
- generator/yield EffectSystem inspired by Redux-Saga but typed for Flow
- DOM and React Native testing utilities compatible with Testing Library habits
- ORM schema/runtime with Flow opaque types at module boundaries
- StyleX compiler/runtime integration
- React Compiler syntax-mode integration
- Relay integration
- headless UI components with preset styles and RSC split metadata
- React hook utilities that preserve render idempotency
- default `uf` runtime with Node.js, Bun, Deno, edge, serverless, static, and
  container deployment adapters in a Nitro-like deploy-anywhere model
- Hermes-backed runtime execution mode once the native bridge is deep enough

## Build And Dev

`uniflowed.config.flow` mirrors the Vite style because the build and dev pipeline is
expected to reuse Vite-compatible plugin semantics and Rolldown where it gives us
the best performance. Users should not need `vite.config.*` or `rolldown`
configuration files; those engines stay internal to uniflowed.

## Performance Defaults

The default Rust toolbox is centralized in `uniflowed_infra`:

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
