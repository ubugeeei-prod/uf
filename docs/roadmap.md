# Roadmap

## North Star

- Beat Vite+ on Flow React DX, framework completeness, build latency, and dev
  server feedback loops.
- Use Vite Task for cached, dependency-aware task execution while beating Vitest
  and Bun Test on native test throughput, runtime startup, package manager
  performance, and integrated toolchain coverage.
- Keep Vite itself as the internal bundler, dev server, and plugin system, but
  make `uf.config.js` the only user-authored config entry.

The threat model that every one of these must satisfy is in
[docs/security.md](security.md): each row names a published CVE in an
incumbent tool and the structural decision that makes the same bug impossible
in `uf`.

## P0: Toolchain Spine

- Keep the first executable native slice green: `uf create`, `uf build`,
  `uf dev --once`, `uf install`, `uf upgrade`, `uf use`, `uf publish`,
  `uf release`, `ufx`, `uf test`, `uf prepare`, and `uf lsp` already produce
  local artifacts or protocol responses.
- Finish parser/typechecker integration against Flow's maintained parser and
  type checker boundary.
- Replace whitespace formatter core with a Flow AST printer backed by the
  official Flow Rust parser, and route non-Flow files to Biome formatting.
- Wire `uf build` and `uf dev` to Vite's bundler, dev server, and plugin
  container.
- Keep `uf.config.js` as the single config and task surface; generated projects
  do not use npm scripts, and task execution goes through Vite Task.
- Keep app execution runtime-agnostic through Capability JS Hosts: Node.js,
  Deno, and Bun.
- Add LSP JSON-RPC loop over config, parser diagnostics, formatter, and lints.

## P1: Native App Framework

- File-system router compiler for web and React Native targets.
- Nuxt-like web primitives: Font, Image, OgImage, Link with prefetch, Page,
  Layout, Time, Announcer, Picture, useCookie, and useHead.
- Fully type-safe `useRoute`, `useRouter`, navigation guards, Remix-style
  loaders/actions, Next-style metadata/static params, and React Router-style
  route modules.
- RSC module graph split.
- Server action transform and request bridge.
- StyleX transform as the default style engine.
- React Compiler syntax-mode pass.
- Vite-backed server entry generation, RSC streaming, and server action bridge
  for Node.js, Deno, and Bun hosts.
- WinterTC-aligned Flow runtime execution on Hermes after the Vite/host-runtime
  path is stable.
- ORM, Valibot-class validator, Jotai-class state atoms, and cell runtime
  primitives.
- Lite Temporal, PWA primitives, and opt-in-only cache controls.
- React Compiler-safe motion primitives with reduced-motion defaults.
- OpenTUI-aligned native TUI framework targeting a React Ink replacement.
- shadcn-class UI catalog as typed headless Flow React imports.

## P2: Native Test Runner

- Keep `@uniflowed/test` self-hosted in Rust, with JavaScript execution
  delegated to Capability JS Hosts.
- Target faster-than-Bun-Test and faster-than-Vitest execution for Flow-heavy suites.
- Implement JavaScript execution backend.
- Add React DOM and React Native renderers.
- Add native terminal renderers and snapshots through `@uniflowed/tui`.
- Add Testing Library-compatible queries and user events.
- Add story system, MSW-compatible mocks, Playwright-compatible browser
  automation, and VRT baseline diffing.
- Add `uf prepare` with lint-staged-compatible checks and code generation.
- Add watch mode with dependency-aware reruns.

## P3: Package And Runtime

- Self-hosted `@uniflowed/pm` package resolver, `uf.lock`, content-addressed cache, and installer.
- `@uniflowed/rm` runtime manager that infers, detects, and applies Capability
  JS Hosts from config.
- napi-rs-style target package generation with generated TS declarations
  converted back to Flow.
- ox-content wasm-backed stdlib markdown renderer.
- `uf install` and `uf upgrade`.
- First local `uf publish`, tokenless trusted publishing on `uf@*` tag push,
  and `uf release alpha` tag orchestration.
- Hermes-backed `uf index.js` runtime, deferred until after the host runtime
  path is stable.
- `curl -fsSL https://setup.uniflowed.dev | sh` installer.
