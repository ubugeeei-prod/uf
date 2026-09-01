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
- [x] Use `uf.config.flow` as the single user-visible config surface.
- [x] Start native `@uniflowed/test`, `@uniflowed/pm`, and `@uniflowed/rm` contracts.
- [x] Start XDG-compliant uf runtime layout.
- [x] Add `uf use uf@0.1.0` runtime-switch command surface.
- [x] Add `ufr` alias for `uf run`.
- [x] Add `ufx` temporary execution command surface.
- [ ] Integrate the maintained Flow parser boundary.
- [ ] Integrate Flow typecheck diagnostics without requiring `.flowconfig`.
- [ ] Replace whitespace formatter with a Flow AST printer.
- [x] Default formatter settings to double quotes and semicolons.
- [ ] Add large-project file discovery tests with ignored directories and non-UTF8 guardrails.
- [ ] Add benchmark gates for config loading, route discovery, lint scanning, and test discovery.
- [ ] Ban `String`, `format!`, and allocation-heavy std helpers in parser/lint/router/test hot paths.
- [ ] Audit hot paths for unnecessary `.clone()` calls and replace them with borrowed or arena-backed flows.
- [ ] Add LSP JSON-RPC loop for diagnostics, format, code actions, and inspect data.
- [x] Add editor integration directories for VS Code, Neovim, Emacs, Vim, Helix, Zed, and Cursor.
- [ ] Implement editor extension packages on top of `uf lsp`.
- [x] Use Vite Task-compatible task definitions in `uf.config.flow`.
- [x] Ban npm scripts from generated project templates and lint defaults.

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
- [ ] Align the `uf` runtime with WinterTC.
- [ ] Execute Flow through Hermes by default once runtime execution lands.
- [ ] Implement Rust-native server request handling and streaming.
- [ ] Implement Rust-native libuv-level IO primitives.
- [ ] Support deploy-anywhere adapters for Node.js, Bun, Deno, edge, serverless, static, and container targets.
- [ ] Assume React 19, Suspense, `use`, and Async React.
- [ ] Bundle GraphQL Relay primitives.
- [ ] Provide explicit ofetch-style clients without global fetch override.
- [x] Start Nuxt-like web primitives: Font, Image, OgImage, Link, Page, Layout, Time, Announcer, and Picture.
- [x] Start typed `useCookie`, `useHead`, `useRoute`, `useRouter`, and navigation guard contracts.
- [ ] Generate fully type-safe route hook declarations from `router.flow`.
- [ ] Implement Link prefetch scheduling with opt-in cache semantics.
- [ ] Expose Nuxt Module-style builder hooks from Rust.
- [ ] Build docs with the framework as an RSC fully static site.
- [ ] Deploy generated docs to `void`.

## P2: Native Libraries Exposed To Flow

- [ ] Implement `@uniflowed/query` as a native TanStack Query-style data layer.
- [ ] Implement `@uniflowed/effect` as a typed generator/yield EffectSystem.
- [x] Start `@uniflowed/state` and `@uniflowed/flow-cell` from this repository.
- [ ] Complete `@uniflowed/state` and `@uniflowed/flow-cell` runtime bindings.
- [x] Start `@uniflowed/validator` with `v.pipe`-style validation.
- [ ] Add validator-driven Flow type inference and schema exports.
- [x] Start `@uniflowed/cli` as a Gunshi-style stdlib CLI framework.
- [x] Start `@uniflowed/fetch` as an explicit ofetch-style client.
- [x] Start `@uniflowed/graphql` as a Relay-based GraphQL client.
- [x] Start `@uniflowed/web` for web primitives and route/head/cookie hooks.
- [x] Start `@uniflowed/markdown` with an ox-content wasm-backed contract.
- [x] Start `@uniflowed/motion` with React Compiler-safe motion contracts.
- [x] Start `@uniflowed/temporal` as a lite Temporal contract.
- [x] Start `@uniflowed/pwa` with opt-in cache defaults.
- [ ] Implement `@uniflowed/orm`.
- [ ] Implement `@uniflowed/stylex` with preset StyleX defaults.
- [ ] Implement `@uniflowed/ui` as an RSC-compatible headless UI library.
- [ ] Cover the shadcn-style component catalog.
- [ ] Keep compound UI APIs cohesive, for example `Dialog.Body`.
- [x] Add UI `renders` type utility declarations under `crates/uf_lib/lib/ui`.
- [x] Make form UI validator-backed and React Compiler-safe by contract.
- [ ] Add compile-time form value/error type generation from validator schemas.
- [ ] Expose runtime bindings through Flow declarations.
- [ ] Back the declarations with Rust native runtime modules.

## P3: Test Runner And DX

- [ ] Implement `import { describe, it, expect } from '@uniflowed/testing'`.
- [x] Start self-hosted `@uniflowed/test` runner planning.
- [ ] Make `@uniflowed/test` execute Flow suites through the native runtime.
- [ ] Benchmark `@uniflowed/test` against Bun and keep the faster-than-Bun target visible.
- [ ] Implement native React Testing Library-compatible DOM queries.
- [ ] Implement native React Native testing utilities.
- [ ] Add watch mode with dependency-aware reruns.
- [ ] Add strict CLI integration tests for every command.
- [ ] Add snapshot tests for generated templates and router types.
- [ ] Add e2e type-safety fixtures for app, server actions, router, query, effect, and UI.
- [x] Start story, mock, browser, and VRT contracts.
- [ ] Implement `@uniflowed/story` component story runner.
- [x] Start `@uniflowed/vrt` native visual regression contracts.
- [ ] Implement `@uniflowed/mock` MSW-compatible native request mocking.
- [ ] Implement `@uniflowed/browser` Playwright-compatible browser automation.
- [ ] Add visual regression baselines, diffing, and update flows.
- [x] Add `uf prepare` command surface for lint-staged-compatible checks and code generation.
- [ ] Wire `uf prepare` to staged file discovery and generated type writes.

## P4: Build, Runtime, Package Manager, Publish

- [ ] Wire `uf build` to Vite-compatible plugin semantics.
- [ ] Wire production builds to Rolldown where possible.
- [ ] Wire `uf dev` to a Vite-compatible dev server.
- [x] Start self-hosted `@uniflowed/pm` package manager planning.
- [ ] Implement native package resolver.
- [ ] Implement native lockfile and content-addressed cache.
- [x] Start `@uniflowed/rm` runtime manager inference/acquire/apply planning.
- [ ] Implement runtime acquisition and host adaptation in `@uniflowed/rm`.
- [ ] Publish `curl -fsSL https://setup.uniflowed.dev | sh` installer.
- [ ] Support sh, bash, zsh, ush, Windows, macOS, and Linux installer targets.
- [x] Start napi-rs-style native target package generation contracts.
- [x] Start generated TypeScript declaration to Flow declaration conversion.
- [ ] Implement `uf install`.
- [ ] Implement `uf upgrade`.
- [ ] Implement Hermes-backed `uf index.js` runtime.
- [ ] Implement `uf publish`.
- [x] Add trusted publish config defaults for local first publish and tokenless OIDC publishing.
- [x] Add tag-push trusted publish GitHub Actions scaffold.
- [x] Add `uf release minor` command surface.
- [ ] Wire `uf release minor` to semver calculation, changelog generation, and `uf@*` tag push.
- [ ] Ship `curl -fsSL https://setup.uniflowed.dev | sh`.

## P5: Standard Library, Legal, And Formal Methods

- [x] Start `@uniflowed/std` registry for vfs, fs, types, pipeline, effect, env, format, stdio, hash, debug, defs, lock, colors, qs, equality, http, buffer, ws, sql, json, yaml, toml, collections, crypto, dotenv, math, os, net, dns, path, stream, url, wasm, glob, motion, cron, s3, sigv4, functions, uuid, zip, import-meta, and defer.
- [ ] Bind `@uniflowed/std` modules to Rust-native implementations.
- [x] Start type-safe native ORM contracts.
- [ ] Implement ORM drivers and generated Flow row/query types.
- [x] Add license inventory and builtin intake checklist under `tools/legal`.
- [ ] Add automated license checks for builtin dependencies.
- [x] Add Why3/Z3 formal verification scaffold under `tools/formal`.
- [ ] Prove hot-path std invariants through Why3/Z3.
