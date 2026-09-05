# Roadmap: uf, the Unified Toolchain for Flow (React)

## P-1: Repository Bootstrap

- [x] Rename product to **uniflowed**.
- [x] Use **`uf`** as the only user-facing command.
- [x] Configure GitHub repository metadata for "Unified Toolchain for Flow (React)".
- [x] Enable GitHub auto-merge.
- [x] Add Blacksmith 32 vCPU CI jobs for format, clippy, tests, bench compile, and metadata.
- [x] Keep admin branch-protection bypass available.
- [x] Document the product bar: beat Vite+ on Flow React DX/build performance and beat Bun on native runtime/test/package-manager performance.

## P0: Native Toolchain Spine

- [ ] Keep all core implementation in Rust native crates.
- [x] Use `uf.config.js` as the single user-visible config surface.
- [x] Prefer `.js` files with `// @flow` for user-authored Flow source.
- [x] Default app execution to runtime-agnostic Capability JS Hosts: Node.js,
      Deno, and Bun.
- [x] Start native `@uniflowed/test`, `@uniflowed/pm`, and `@uniflowed/rm` contracts.
- [x] Start XDG-compliant uf runtime layout.
- [x] Add `uf use uf@0.1.0` runtime-switch command surface.
- [x] Add `ufr` alias for `uf run`.
- [x] Add `ufx` temporary execution command surface.
- [x] Integrate the maintained Flow parser boundary through the `upstream/flow`
      submodule and Meta's official Flow Rust port.
- [x] Make Meta's Flow Rust port the only parser, and delete the QuickJS-hosted
      backend along with the source rewriting it required.
- [x] Integrate Flow typecheck diagnostics without requiring `.flowconfig`.
      The port's typing crates need `box_patterns`, removed from the floating
      nightly channel, so `rust-toolchain.toml` pins `nightly-2026-08-01`.
      68 ms to merge builtins, ~4 ms per file. See docs/architecture.md.
- [x] Replace whitespace formatter with a Flow AST printer backed by the
      official Flow Rust parser. Prettier-compatible, checked against
      `prettier --parser hermes` by 27 fixtures; idempotent,
      tree-preserving, comment-preserving and total.
- [x] Route non-Flow formatter configuration to Biome.
- [x] Actually run Biome for `.json`, `.jsonc`, `.css` and `.ts`. `uf fmt`
      collects the non-Flow files and hands them to the command named by
      `fmt.nonFlow.formatter`, resolved from `node_modules/.bin` before
      `PATH`.
- [x] Default formatter settings to double quotes and semicolons.
- [ ] Add large-project file discovery tests with ignored directories and non-UTF8 guardrails.
- [ ] Add benchmark gates for config loading, route discovery, lint scanning, and test discovery.
- [ ] Ban `String`, `format!`, and allocation-heavy std helpers in parser/lint/router/test hot paths.
- [ ] Audit hot paths for unnecessary `.clone()` calls and replace them with borrowed or arena-backed flows.
- [ ] Add LSP JSON-RPC loop for diagnostics, format, code actions, and inspect data.
- [x] Add editor integration directories for VS Code, Neovim, Emacs, Vim, Helix, Zed, and Cursor.
- [ ] Implement editor extension packages on top of `uf lsp`.
- [x] Use uf task definitions in `uf.config.js`.
- [x] Ban npm scripts from generated project templates and lint defaults.

## P1: Flow React Framework

- [x] Use `app.js` as the framework entrypoint.
- [x] Load `./app` through `routerView('./app')`.
- [x] Reserve `app/_uf.layout.js`.
- [x] Reserve `app/_uf.page.js`.
- [x] Reserve `app/_uf.middleware.js`.
- [x] Define one reserved-name grammar, `_uf.<role>[.<variant>].js`, shared by
      `uf create`, the router, and the linter.
- [x] Generate `router.js` with route path and params types.
- [ ] Enforce typed route guards and constraints.
- [ ] Make Server Components the default.
- [ ] Require client components to opt in with `"use client";`.
- [ ] Require server actions to opt in with `"use server";`.
- [ ] Support RSC graph splitting.
- [ ] Support PPR, SSR, SSG, and ISR.
- [ ] Keep route, fetch, action, and data cache defaults OFF.
- [x] Default to Node.js through the Capability JS Host contract.
- [x] Keep Node.js, Deno, and Bun as zero-config host targets.
- [ ] Align the deferred `uf` runtime with WinterTC.
- [ ] Execute Flow through Hermes once the Vite/host-runtime path is stable.
- [ ] Implement Vite-backed server entry generation and RSC streaming adapters.
- [ ] Map host-provided IO capabilities for Node.js, Deno, and Bun.
- [ ] Support deploy-anywhere adapters for Node.js, Deno, Bun, edge, serverless, static, and container targets.
- [ ] Assume React 19, Suspense, `use`, and Async React.
- [ ] Bundle GraphQL Relay primitives.
- [ ] Provide explicit fetch clients without global fetch override.
- [x] Start Nuxt-like web primitives: Font, Image, OgImage, Link, Page, Layout, Time, Announcer, and Picture.
- [x] Start typed `useCookie`, `useHead`, `useRoute`, `useRouter`, and navigation guard contracts.
- [ ] Generate fully type-safe route hook declarations from `router.js`.
- [ ] Implement Link prefetch scheduling with opt-in cache semantics.
- [ ] Expose Nuxt Module-style builder hooks from Rust.
- [ ] Build docs with the framework as an RSC fully static site.
- [ ] Deploy generated docs to `void`.

## P2: Native Libraries Exposed To Flow

- [x] Implement `@uniflowed/query` as a TanStack Query-style data layer, in Flow.
      Deduplication, supersession, retry policies, collection, optimistic
      mutations and infinite pages, in twelve modules with no `internal/`.
- [ ] Implement `@uniflowed/effect` as a typed generator/yield EffectSystem.
- [x] Start `@uniflowed/state` and `@uniflowed/cell` from this repository.
- [x] Complete `@uniflowed/state` and `@uniflowed/cell` as Flow JS implementations.
- [x] Start `@uniflowed/validator` with `v.pipe`-style validation.
- [x] Implement `@uniflowed/immer`: immutable updates through a draft, in Flow.
- [x] Implement `@uniflowed/form`: uncontrolled fields, narrow subscriptions and
      schema validation. Eleven characters typed cost two renders against a
      controlled `useState`'s twelve.
- [ ] Add validator-driven Flow type inference and schema exports.
- [x] Start `@uniflowed/cli` as a Gunshi-style stdlib CLI framework.
- [x] Start `@uniflowed/fetch` as an explicit fetch client.
- [x] Start `@uniflowed/graphql` as a Relay-based GraphQL client.
- [x] Start `@uniflowed/web` for web primitives and route/head/cookie hooks.
- [x] Start `@uniflowed/markdown` with an ox-content wasm-backed contract.
- [x] Start `@uniflowed/motion` with React Compiler-safe motion contracts.
- [x] Start `@uniflowed/tui` as an OpenTUI-aligned native TUI framework.
- [x] Start `@uniflowed/temporal` as a lite Temporal contract.
- [x] Start `@uniflowed/pwa` with opt-in cache defaults.
- [ ] Implement `@uniflowed/orm`.
- [ ] Implement `@uniflowed/stylex` with preset StyleX defaults.
- [ ] Implement `@uniflowed/ui` as an RSC-compatible headless UI library that can replace shadcn for Flow React apps.
- [ ] Implement native terminal rendering, layout, input, and snapshots for `@uniflowed/tui`.
- [ ] Cover the shadcn-style component catalog with typed imports, preset styles, and no copy step.
- [ ] Keep compound UI APIs cohesive, for example `Dialog.Body`.
- [x] Add UI `renders` type utility declarations under `packages/ui`.
- [x] Make form UI validator-backed and React Compiler-safe by contract.
- [ ] Add compile-time form value/error type generation from validator schemas.
- [ ] Expose runtime bindings through Flow declarations.
- [ ] Back the declarations with Rust native runtime modules.

## P3: Test Runner And DX

- [x] Implement `import { describe, it, expect } from '@uniflowed/testing'`.
- [x] Start self-hosted `@uniflowed/test` runner planning.
- [x] Execute the first native source-level assertion subset in `uf test`.
- [x] Replace that subset with real execution on a Capability JS Host.
- [x] Make `@uniflowed/test` execute full Flow suites through Capability JS
      Hosts while keeping scheduling and reporting in Rust.
- [x] Benchmark `@uniflowed/test` against Bun Test and Vitest and keep the faster-than-Bun target visible.
      Measured: 9x faster than Vitest, 3x slower than Bun. See docs/architecture.md.
- [ ] Implement native React Testing Library-compatible DOM queries.
- [ ] Implement native React Native testing utilities.
- [x] Add watch mode with dependency-aware reruns.
- [ ] Add strict CLI integration tests for every command.
- [x] Report what a test printed. `console.log` in a test used to kill the file
      it was in, because the worker's stdout was the protocol.
- [ ] Add snapshot tests for generated templates and router types.
- [ ] Add e2e type-safety fixtures for app, server actions, router, query, effect, and UI.
- [x] Start story, mock, browser, and VRT contracts.
- [ ] Implement `@uniflowed/story` component story runner.
- [x] Start `@uniflowed/vrt` native visual regression contracts.
- [ ] Implement `@uniflowed/mock` MSW-compatible request mocking.
- [ ] Implement `@uniflowed/browser` Playwright-compatible browser automation.
- [ ] Add visual regression baselines, diffing, and update flows.
- [x] Add `uf prepare` command surface for lint-staged-compatible checks and code generation.
- [x] Write `.uf/prepare.json` and generated route metadata from `uf prepare`.
- [ ] Wire `uf prepare` to staged file discovery and generated type writes.

## P4: Build, Runtime, Package Manager, Publish

- [x] Wire `uf build` to Vite itself and the Vite plugin container.
- [x] Measure emitted bundle size and enforce `build.budgets` from `uf build`.
- [x] Write native build manifest and generated router types from `uf build`.
- [x] Wire `uf dev` to Vite's dev server.
- [x] Start self-hosted `@uniflowed/pm` package manager planning.
- [ ] Implement native package resolver.
- [x] Implement native workspace lockfile and content-addressed store entries.
- [x] Start `@uniflowed/rm` runtime manager inference/acquire/apply planning.
- [x] Implement local current-binary runtime activation for `uf use`.
- [ ] Implement host detection and adaptation in `@uniflowed/rm`.
- [ ] Publish `curl -fsSL https://setup.uniflowed.dev | sh` installer.
- [ ] Support sh, bash, zsh, ush, Windows, macOS, and Linux installer targets.
- [x] Start napi-rs-style native target package generation contracts.
- [x] Start generated TypeScript declaration to Flow declaration conversion.
- [x] Implement `uf install` for workspace package discovery, lockfile writes, store manifest writes, and npm-script rejection.
- [x] Implement `uf upgrade` package/runtime manifest writes.
- [ ] Implement Hermes-backed `uf index.js` runtime after the host runtime path.
- [x] Implement `uf publish` trusted publish manifest writes.
- [x] Add trusted publish config defaults for local first publish and tokenless OIDC publishing.
- [x] Add tag-push trusted publish GitHub Actions scaffold.
- [x] Add `uf release alpha` command surface.
- [x] Wire `uf release alpha` to prerelease calculation and `uf@*` tag metadata.
- [ ] Wire `uf release alpha` to changelog generation and tag push.
- [ ] Ship `curl -fsSL https://setup.uniflowed.dev | sh`.

## P5: Standard Library, Legal, And Formal Methods

- [x] Start `@uniflowed/std` registry for vfs, fs, types, pipeline, effect, env, format, stdio, hash, debug, defs, lock, colors, qs, equality, http, buffer, ws, sql, json, yaml, toml, collections, crypto, dotenv, math, os, net, dns, path, stream, url, wasm, glob, motion, tui, cron, s3, sigv4, functions, uuid, zip, import-meta, and defer.
- [ ] Bind `@uniflowed/std` modules to Rust-native implementations.
- [x] Start type-safe native ORM contracts.
- [ ] Implement ORM drivers and generated Flow row/query types.
- [x] Add license inventory and builtin intake checklist under `tools/legal`.
- [ ] Add automated license checks for builtin dependencies.
- [x] Add Why3/Z3 formal verification scaffold under `tools/formal`.
- [ ] Prove hot-path std invariants through Why3/Z3.
