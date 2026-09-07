# Roadmap

## North Star

**uf alone is enough to build a React application — any React application —
and everything it provides is production-ready.** Comprehensive, fast, and
typed strongly enough that inference reaches the end of a real program. It runs
on every runtime and deploys anywhere, including as a single executable file.

- Beat Vite+ on Flow React DX, framework completeness, build latency, and dev
  server feedback loops.
- Run `uf.config.js` tasks natively — a dependency graph rather than a
  recursion, a concurrency limit, and a cache keyed on the files each task
  says it reads — while beating Vitest and Bun Test on native test throughput,
  runtime startup, package manager performance, and integrated toolchain
  coverage. Vite Task still runs a task that names no command of its own;
  everything anybody actually writes is uf's, because `uf.config.js` is where
  a task's meaning is written down and `vp run` cannot read it.
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
   tells the caller nothing. Inference that stopped at a package boundary was
   the worst case of it and is fixed: a bare specifier resolves through the
   manifest that publishes it ([#248](https://github.com/ubugeeei-prod/uf/issues/248)),
   and the batch a check runs over is the closure of what the files asked about
   import, `node_modules` included
   ([#403](https://github.com/ubugeeei-prod/uf/issues/403)). `uf check` reports
   1,905 errors over this repository's 358 checked files, and they are errors
   about types rather than about the resolver. Some of them are wanted: the
   fixtures under `tests/type-tests/` exist to fail, and each is held to the
   exact diagnostics it predicts.
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
| Builds a standalone binary | `uf build --compile` writes one, and needs Bun on PATH to do it; cross-compiling to another platform does not exist | [#310](https://github.com/ubugeeei-prod/uf/issues/310) |
| Deploys anywhere | `uf build --adapter` writes for `node`, `container`, `edge` and `serverless`, and no output has ever been deployed to a real platform; `bun`, `deno` and `static` are names in a config struct | [#391](https://github.com/ubugeeei-prod/uf/issues/391) |
| Inference reaches the end of a program | It does: a type imported by its published name resolves, from the workspace or from `node_modules`. What is left is a dependency that opts into no Flow, which is `any` by Flow's own rule | [#248](https://github.com/ubugeeei-prod/uf/issues/248), [#403](https://github.com/ubugeeei-prod/uf/issues/403) |
| Everything implemented reaches a user | Ten implemented packages are on nobody's npm; `@uniflowed/tui` is a contract nobody can run | [#210](https://github.com/ubugeeei-prod/uf/issues/210), [#247](https://github.com/ubugeeei-prod/uf/issues/247) |

The threat model that every one of these must satisfy is in
[docs/security.md](security.md): each row names a published CVE in an
incumbent tool and the structural decision that makes the same bug impossible
in `uf`.

## P0: Toolchain Spine

- Keep the first executable native slice green: `uf new`, `uf build`,
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
- Nuxt-like web primitives: Font, Image, Link with prefetch, Page, Layout,
  Time, Announcer, Picture, useCookie, useHead, Icon, IconSprite, and
  `OgImage`. **The decision on `OgImage` has been made and it is not "an image
  from JSX"**: rendering a document to an image is a CSS layout engine, and the
  only two ways to have one are to ship a browser — a dependency uf refuses to
  make building an application need — or to approximate one, which produces
  cards that are subtly wrong on somebody else's website. uf draws a
  **declared template** instead: a `*.og.json` with a background, a rule, an
  eyebrow, a title and a subtitle, rasterised natively with `ab_glyph`. It does
  no shaping, so it **refuses** right-to-left and Brahmic scripts, Thai and its
  neighbours, combining marks, emoji, and any character the font lacks a glyph
  for, naming the character — rather than drawing the wrong glyphs in the wrong
  order. uf ships no typeface, so a template with text names one.
- The metadata files. **Done**: `sitemap.xml` and `robots.txt`, written by
  `uf build` from the documents the prerender wrote and the origin `site.url`
  names, and a `Metadata` wide enough for a canonical URL, a Twitter card and
  an absolute `metadataBase`. **Not done**: a web app manifest, which overlaps
  `@uniflowed/pwa`; `opensearch.xml`; and a feed, which the route table cannot
  describe.
- The pipeline behind `Image`, `Font`, `Icon` and `OgImage`. **Done at build
  time**: `uf assets` decodes an imported image once, writes a variant at every
  declared width in the source's own format and in WebP where the WebP is
  smaller, and emits the `srcset`, `sizes`, intrinsic dimensions and blur
  placeholder from it; an imported font is self-hosted under a content hash and
  declared with a `size-adjust` fallback computed from its own `OS/2` metrics;
  `subset: "ranges"` splits a family into script buckets with exact
  `unicode-range` values through `skera`, Google Fonts' Rust port of
  `hb-subset`, and exactly one face is preloaded however many buckets there
  are; `uf:icon/…` turns an SVG into a `<symbol>` and `uf:icon-sprite` is one
  sprite holding the icons the build actually reached; a `*.og.json` template
  is drawn to a card. Every manifest is cached under a digest of its inputs, so
  a warm build does none of it again. **Not done**: AVIF and lossy WebP, which
  need encoders uf does not ship; a WOFF2 *encoder*, so a subsetted face is
  emitted as WOFF 1.0; subsetting a CFF font or a WOFF2 source, both refused
  with the file to use instead; an import syntax for `text` subsetting, which
  the `uf assets` protocol carries and no `.js` file can express without a
  query; and a request-time endpoint for remote and user-supplied images, which
  needs `remotePatterns` in the same commit (`docs/security.md`).
- Fully type-safe `useRoute`, `useRouter`, navigation guards, Remix-style
  loaders/actions, Next-style metadata/static params, and React Router-style
  route modules.
- RSC module graph split. **The route level is done**: a route with no
  `"use client"` boundary reachable from its page, its layouts or its
  fallbacks keeps its page module out of the client bundle, and the browser is
  not asked to hydrate it. Splitting a *module* — dropping a Server Component
  that sits above a boundary — is not, and is blocked on a Flight-shaped
  payload the client can re-render a tree from.
- Server action transform and request bridge. **Done for a module export**: a
  `"use server"` module is replaced in the client graph by one reference per
  callable export, the browser posts the keyed id to the page's own URL, and
  the endpoint runs the function inside the host's request — in `uf dev`, in
  `uf build`, and in all four deploy adapters, which serve it out of one
  `handler.js`. Arguments and results are plain JSON data under a closed
  grammar (`docs/security.md`), and Flow holds every action's signature against
  it. **Not done**: an inline `"use server"` closure, which has no export name
  to refer to; `<form action={fn}>` and the React 19 form hooks, which need
  `FormData` across the same boundary; and `useActionState`.
- StyleX transform as the default style engine.
- React Compiler syntax-mode pass.
- Vite-backed server entry generation, RSC streaming, and server action bridge
  for Node.js, Deno, and Bun hosts.
- WinterTC-aligned Flow runtime execution on Hermes after the Vite/host-runtime
  path is stable.
- ORM, Valibot-class validator, Jotai-class state atoms, and cell runtime
  primitives.
- Lite Temporal, PWA primitives, and opt-in-only cache controls. **Lite
  Temporal is done**: `@uniflowed/core/temporal` uses `globalThis.Temporal`
  where the host has it and implements `Instant`, `ZonedDateTime`, `PlainDate`,
  `PlainTime` and `Duration` over `Intl.DateTimeFormat` where it does not, and
  `Temporal.Now` reads uf's own clock in both cases so a server render and the
  hydration after it can be made to agree. ISO 8601 only — a non-ISO calendar
  needs the CLDR tables and so needs the binary, which is what
  `@uniflowed/temporal`'s `withCalendar` still says. **Not done**: nanoseconds,
  `round`, `with`, and the three plain types nothing uf ships needs yet.
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
