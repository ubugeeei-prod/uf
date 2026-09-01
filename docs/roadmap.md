# Roadmap

## P0: Toolchain Spine

- Finish parser/typechecker integration against Flow's maintained parser and
  type checker boundary.
- Replace whitespace formatter core with a Flow AST printer.
- Wire `uf build` and `uf dev` to a Vite-compatible plugin container and
  Rolldown-backed production build.
- Keep `uf.config.flow` as the single config and Vite Task-compatible task
  surface; generated projects do not use npm scripts.
- Add LSP JSON-RPC loop over config, parser diagnostics, formatter, and lints.

## P1: Native App Framework

- File-system router compiler for web and React Native targets.
- Nuxt-like web primitives: Font, Image, OgImage, Link with prefetch, Page,
  Layout, Time, Announcer, Picture, useCookie, and useHead.
- Fully type-safe `useRoute`, `useRouter`, and navigation guards.
- RSC module graph split.
- Server action transform and request bridge.
- StyleX transform as the default style engine.
- React Compiler syntax-mode pass.
- Native server, RSC streaming, server actions, and libuv-level IO in Rust.
- WinterTC-aligned Flow runtime execution on Hermes.
- ORM, validator, state, and flow-cell runtime primitives.
- Lite Temporal, PWA primitives, and opt-in-only cache controls.
- React Compiler-safe motion primitives with reduced-motion defaults.

## P2: Native Test Runner

- Keep `@uniflowed/test` self-hosted and native.
- Target faster-than-Bun execution for Flow-heavy suites.
- Implement JavaScript execution backend.
- Add React DOM and React Native renderers.
- Add Testing Library-compatible queries and user events.
- Add story system, MSW-compatible mocks, Playwright-compatible browser
  automation, and VRT baseline diffing.
- Add `uf prepare` with lint-staged-compatible checks and code generation.
- Add watch mode with dependency-aware reruns.

## P3: Package And Runtime

- Self-hosted `@uniflowed/pm` package resolver, `uf.lock`, content-addressed cache, and installer.
- `@uniflowed/rm` runtime manager that infers, acquires, and applies runtimes from config.
- napi-rs-style target package generation with generated TS declarations
  converted back to Flow.
- ox-content wasm-backed stdlib markdown renderer.
- `uf install` and `uf upgrade`.
- First local `uf publish`, tokenless trusted publishing on `uf@*` tag push,
  and `uf release minor` tag orchestration.
- Hermes-backed `uf index.js` runtime.
- `curl -fsSL https://setup.uniflowed.dev | sh` installer.
