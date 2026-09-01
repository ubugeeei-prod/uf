# Roadmap: uf, the Unified Toolchain for Flow (React)

## P-1: Repository Bootstrap

- [x] Rename product to **uniflowed**.
- [x] Use **`uf`** as the only user-facing command.
- [x] Configure GitHub repository metadata for "Unified Toolchain for Flow (React)".
- [x] Enable GitHub auto-merge.
- [x] Add Blacksmith 32 vCPU CI jobs for format, clippy, tests, bench compile, and metadata.
- [x] Keep admin branch-protection bypass available.

## P0: Native Toolchain Spine

- [ ] Keep all core implementation in Rust native crates.
- [ ] Use `uniflowed.config.flow` as the single user-visible config surface.
- [ ] Integrate the maintained Flow parser boundary.
- [ ] Integrate Flow typecheck diagnostics without requiring `.flowconfig`.
- [ ] Replace whitespace formatter with a Flow AST printer.
- [x] Default formatter settings to double quotes and semicolons.
- [ ] Add large-project file discovery tests with ignored directories and non-UTF8 guardrails.
- [ ] Add benchmark gates for config loading, route discovery, lint scanning, and test discovery.
- [ ] Ban `String`, `format!`, and allocation-heavy std helpers in parser/lint/router/test hot paths.
- [ ] Audit hot paths for unnecessary `.clone()` calls and replace them with borrowed or arena-backed flows.
- [ ] Add LSP JSON-RPC loop for diagnostics, format, code actions, and inspect data.

## P1: Flow React Framework

- [ ] Use `app.flow` as the framework entrypoint.
- [ ] Load `./app` through `routerView('./app')`.
- [ ] Reserve `app/_uf.layout.flow`.
- [ ] Reserve `app/_uf.page.flow`.
- [ ] Reserve `app/_uf.middleware.flow`.
- [ ] Generate `router.flow` with route path and params types.
- [ ] Enforce typed route guards and constraints.
- [ ] Make Server Components the default.
- [ ] Require client components to opt in with `"use client";`.
- [ ] Require server actions to opt in with `"use server";`.
- [ ] Support RSC graph splitting.
- [ ] Support PPR, SSR, SSG, and ISR.
- [ ] Keep route, fetch, action, and data cache defaults OFF.
- [ ] Default to the `uf` runtime.
- [ ] Support deploy-anywhere adapters for Node.js, Bun, Deno, edge, serverless, static, and container targets.
- [ ] Assume React 19, Suspense, `use`, and Async React.
- [ ] Bundle GraphQL Relay primitives.
- [ ] Expose Nuxt Module-style builder hooks from Rust.
- [ ] Build docs with the framework as an RSC fully static site.
- [ ] Deploy generated docs to `void`.

## P2: Native Libraries Exposed To Flow

- [ ] Implement `@uniflowed/query` as a native TanStack Query-style data layer.
- [ ] Implement `@uniflowed/effect` as a typed generator/yield EffectSystem.
- [ ] Implement `@uniflowed/flow-cell`.
- [ ] Implement `@uniflowed/orm`.
- [ ] Implement `@uniflowed/stylex` with preset StyleX defaults.
- [ ] Implement `@uniflowed/ui` as an RSC-compatible headless UI library.
- [ ] Cover the shadcn-style component catalog.
- [ ] Keep compound UI APIs cohesive, for example `Dialog.Body`.
- [ ] Expose runtime bindings through Flow declarations.
- [ ] Back the declarations with Rust native runtime modules.

## P3: Test Runner And DX

- [ ] Implement `import { describe, it, expect } from '@uniflowed/testing'`.
- [ ] Implement native React Testing Library-compatible DOM queries.
- [ ] Implement native React Native testing utilities.
- [ ] Add watch mode with dependency-aware reruns.
- [ ] Add strict CLI integration tests for every command.
- [ ] Add snapshot tests for generated templates and router types.
- [ ] Add e2e type-safety fixtures for app, server actions, router, query, effect, and UI.

## P4: Build, Runtime, Package Manager, Publish

- [ ] Wire `uf build` to Vite-compatible plugin semantics.
- [ ] Wire production builds to Rolldown where possible.
- [ ] Wire `uf dev` to a Vite-compatible dev server.
- [ ] Implement native package resolver.
- [ ] Implement native lockfile and content-addressed cache.
- [ ] Implement `uf install`.
- [ ] Implement `uf upgrade`.
- [ ] Implement Hermes-backed `uf index.js` runtime.
- [ ] Implement `uf publish`.
- [ ] Ship `curl -fsSL https://uniflowed.dev | sh`.
