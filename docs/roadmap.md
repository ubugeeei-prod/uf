# Roadmap

## North Star

**uf alone is enough to build a React application — any React application —
and everything it provides is production-ready.** Comprehensive, fast, and
typed strongly enough that inference reaches the end of a real program. It runs
on every runtime and deploys anywhere, including as a single executable file.

- Beat Vite+ on Flow React DX, framework completeness, build latency, and dev
  server feedback loops.
- Use Vite Task for cached, dependency-aware task execution while beating Vitest
  and Bun Test on native test throughput, runtime startup, package manager
  performance, and integrated toolchain coverage.
- Keep Vite itself as the internal bundler, dev server, and plugin system, but
  make `uf.config.js` the only user-authored config entry.
- Reach the feature surface of the tools a user would otherwise reach for —
  Next.js for the framework, Bun and Vite for the toolchain, shadcn/ui for the
  components, Effect, Jotai and React Hook Form for the libraries. Where uf
  deliberately differs, the reason is written down; where it simply does not
  have something, that is a gap and it has an issue.

### The three ways this fails

Stated as failures because each is easier to notice than its opposite. Two of
the three have a check behind them; the third says so, which is the point of
listing it here rather than trusting anyone to remember:

1. **A declared API that throws.** `packages/*` holds both real libraries and
   declaration modules whose functions call `nativeRuntimeRequired`, and a
   declaration is not a feature. `tools/ci/publishable.sh` refuses a real
   implementation that is on its way nowhere; nothing yet refuses a declaration
   that has been one for too long.
2. **A type that is `any` under another name.** `flow/unclear-type` catches the
   spelling. It does not catch a type that is technically not `any` and still
   tells the caller nothing, and it does not catch inference that stops at a
   package boundary — which is what
   [#248](https://github.com/ubugeeei-prod/uf/issues/248) is: 145 of the 581
   errors `uf check` reports on this repository are one resolver bug.
3. **A feature that works for the demo.** Every fix in this repository carries
   a test that fails before it and passes after, and the corpus tests run the
   formatter over 8,100 modules nobody here wrote. That is the standard, and
   it is the reason a benchmark claim in these documents is expected to have a
   number beside it.

### Where it is not true yet

Honesty about this is the point of writing it down, and each line is an issue
rather than a note:

| Claim | Today | |
| --- | --- | --- |
| Runs on every runtime | Node and Bun. `HostKind::Deno` loads no Flow; the edge runtimes have no host at all | [#246](https://github.com/ubugeeei-prod/uf/issues/246) |
| Builds a standalone binary | `uf build` emits bundles; there is no `--compile` | [#245](https://github.com/ubugeeei-prod/uf/issues/245) |
| Inference reaches the end of a program | A type imported by its published name resolves to nothing | [#248](https://github.com/ubugeeei-prod/uf/issues/248) |
| Everything implemented reaches a user | Ten implemented packages are on nobody's npm; `@uniflowed/tui` is a contract nobody can run | [#210](https://github.com/ubugeeei-prod/uf/issues/210), [#247](https://github.com/ubugeeei-prod/uf/issues/247) |

The threat model that every one of these must satisfy is in
[docs/security.md](security.md): each row names a published CVE in an
incumbent tool and the structural decision that makes the same bug impossible
in `uf`.

## P0: Toolchain Spine

- Keep the first executable native slice green: `uf create`, `uf build`,
  `uf dev`, `uf install`, `uf upgrade`, `uf use`, `uf publish`,
  `uf release`, `ufx`, `uf test`, `uf prepare`, and `uf lsp` already produce
  local artifacts or protocol responses.
- Finish parser/typechecker integration against Flow's maintained parser and
  type checker boundary.
- Route non-Flow files (JSON, CSS, TypeScript) to Biome formatting. The Flow
  side is done: `uf fmt` prints from the official parser's syntax tree and
  matches Prettier on a fixture corpus.
- `uf build` and `uf dev` run on Vite through `@uniflowed/vite`, with every
  module going through `uf transform` (done).
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
- OpenTUI-aligned TUI framework. Flow React on a cell-diff renderer, with
  flexbox, keyboard and focus. **Done**; mouse, selection and rich content are
  not, and the comparison against React Ink is still to be measured.
- shadcn-class UI catalog as typed headless Flow React imports.

## P2: Native Test Runner

- Keep scheduling, bounds and reporting in Rust, with JavaScript execution
  delegated to Capability JS Hosts. **Done.**
- Target faster-than-Bun-Test and faster-than-Vitest execution for Flow-heavy
  suites. Vitest is beaten by about 9x; Bun's runner is still about 3x faster,
  and closing that needs a worker pool that survives between runs.
- Implement JavaScript execution backend. **Done.**
- Add React DOM and React Native renderers.
- Add terminal renderers and snapshots through `@uniflowed/tui`. **Done**: an
  in-memory renderer runs the same code the terminal one does, and a frame is
  asserted as cells.
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
