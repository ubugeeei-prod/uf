<div align="center">
  
<img width="200" src="brand/uniflowed-mark.svg" alt="uf logo">

# uf

Unified Toolchain for Flow (React)

**🚧 Under active construction — not usable yet. 🚧**

</div>

> [!WARNING]
> **This is a work in progress, not a released tool.** It is being built in the
> open and large parts of it are unfinished or actively being replaced.
>
> - **Nothing is published.** There is no release on GitHub, nothing on
>   crates.io, and nothing on npm. `curl … | sh` will not install anything until
>   the first tag exists.
> - **The build is being moved onto Vite.** The dev server and bundler in this
>   repository are being deleted as Vite takes over, so `uf dev` and `uf build`
>   are mid-migration.
> - **The formatter prints Flow only.** `uf fmt` now formats `.js` from the
>   official parser's syntax tree, matching Prettier; JSON, CSS and
>   TypeScript are not routed to Biome yet.
> - Commands, config keys and package names may change without notice.
>
> Everything below describes where `uf` is going. Treat it as the plan, not as
> documentation of something you can rely on today.



`uf` stands for unified-flow: unified flow for React applications, libraries, and
tooling. It is the all-in-one native command surface for Flow projects: create,
dev server, build, lint, format, type check, test, task running, env inspection,
publishing, LSP, and runtime execution.

The package scope and ecosystem name is **uniflowed**. The command is **`uf`**.

The bar is explicit: beat Vite+ on Flow React DX, framework completeness, and
build/dev performance; beat Bun on native test/runtime/package-manager
performance and integrated feature coverage.

Roadmap tracking lives in [Issue #1](https://github.com/ubugeeei-prod/uf/issues/1).

## Usage

Once a release exists, this is how `uf` will be installed. **Neither command
works yet** — no tag has been cut, so there is nothing behind either URL.

```sh
curl -fsSL https://setup.uniflowed.dev | sh
nix run github:ubugeeei-prod/uf#uf -- --version
```

Until then, build from a checkout:

```sh
tools/upstream/sync.sh && cargo build --release --bin uf
```

- `uf create`: Create your new flow project (zero-config)
- `uf dev`: Start the Vite-owned development server through `uf.config.js`
- `uf build`: Build your Flow project metadata and generated route types
- `uf info`: Show brand, install, docs, and local toolchain information
- `uf lint`: Run the native lint runner
- `uf fmt`: Run the native formatter
- `uf check`: Run Flow type checker
- `uf test`: Run the native Flow-aware test runner
- `uf install`: Write `uf.lock` and the content-addressed `.uf/store`
- `uf upgrade`: Re-apply the native package/runtime manifests
- `uf use`: Install and activate a versioned `uf` runtime when the deferred
  native runtime line is explicitly selected
- `uf publish`: Write local/trusted publish metadata
- `uf release`: Compute release metadata and the next `uf@*` tag
- `uf lsp`: Start the native JSON-RPC language server
- `uf prepare`: Write generated route/codegen preparation metadata
- `ufx`: Execute known native `@uniflowed/*` package entrypoints

## Current Execution Surface

The current CLI already executes the first native vertical slice:

- `uf create app react` and `uf create lib` generate `.js` + `// @flow`
  projects without npm scripts.
- `uf build` discovers file-system routes, writes `router.js`, runs the RSC
  analysis, then builds through Vite: a client bundle, a server bundle, and
  every static route prerendered to HTML, with `dist/uf-build-manifest.json`
  and a shipped-size report beside them.
- `uf dev` starts Vite's dev server through `@uniflowed/vite` on the project's
  JavaScript host, renders every document on the server, and hot-reloads
  through Vite with React Fast Refresh.
- `uf test` runs the suite for real: it discovers and orders the files, fans
  them across worker processes on the project's JavaScript host, and executes
  each one through `@uniflowed/test` — `describe`/`it`, hooks, `.only`/`.skip`/
  `.todo`/`.each`, async tests, spies, and a full matcher set. About nine times
  faster than Vitest on a 1,000-test suite, and still about three times slower
  than Bun's built-in runner; see `docs/architecture.md`.
- `uf install` discovers workspace `package.json` files, rejects npm scripts by
  default, writes `uf.lock`, writes `.uf/store/manifest.json`, and materializes
  content-addressed package entries in `.uf/store/packages`.
- `uf upgrade` reapplies the package store and writes `.uf/upgrade.json`.
- `uf use uf@0.1.0` installs the current `uf` binary into the XDG runtime
  version directory, writes active runtime metadata, and writes the user-local
  shim for the deferred self-hosted runtime line.
- `uf publish` and `uf release alpha` write trusted publish and release
  manifests, including the next `uf@*` tag metadata.
- `ufx @uniflowed/create app` runs the native create package entrypoint and
  records the exec-cache manifest.
- `uf prepare` writes `.uf/prepare.json` and generated router metadata.
- `uf lsp` answers JSON-RPC `initialize` over stdio with formatting and
  diagnostic capabilities.

## Defaults

React app projects work without configuration, and project-specific settings
live in `uf.config.js`. The default app preset
enables:

- Flow component/hook style for React
- `app.js` as the entrypoint
- file-system router rooted at `app`
- reserved `app/_uf.layout.js`, `app/_uf.page.js`, and `app/_uf.middleware.js`
- generated `router.js` route path and params types
- Server Components by default
- explicit `"use client";` for Client Components
- explicit `"use server";` for server actions
- RSC and server actions
- PPR, SSR, SSG, and ISR support
- route, fetch, action, and data cache defaults OFF
- React 19, Suspense, `use`, and Async React assumptions
- React Compiler with `mode: "syntax"`
- Runtime-agnostic execution by default through Capability JS Hosts: Node.js,
  Deno, and Bun
- the self-hosted `uf` runtime and Hermes execution line are deferred until the
  Vite-backed build/dev pipeline is stable
- host-provided event loop and IO capability contracts, with deploy-anywhere
  adapters for Node.js, Deno, Bun, edge, serverless, static, and container
  targets
- XDG-compliant config, data, cache, state, runtime, and shim locations
- StyleX as the default styling layer
- explicit `@uniflowed/fetch` clients; no global fetch override
- Relay-based `@uniflowed/graphql`
- Nuxt-like `Font`, `Image`, `OgImage`, `Link`, `Page`, `Layout`, `Time`,
  `Announcer`, and `Picture` primitives
- fully type-safe `useRoute`, `useRouter`, and navigation guards, shaped to
  replace React Router and Remix flows for Flow React apps
- `useCookie` and `useHead`
- `uf prepare` for lint-staged-compatible checks and code generation
- ox-content wasm-backed `@uniflowed/markdown`
- lite Temporal and PWA primitives
- native stdlib modules for os, net, dns, path, stream, URL, wasm, glob, motion,
  TUI, cron, S3, SigV4, worker/lambda functions, uuid, and zip
- all route, fetch, image, font, markdown, and PWA caches are opt-in
- bundle size budgets in `build.budgets`, measured with real gzip and brotli
- query, effect, ORM, Relay, validator, atom state, and cell builtin modules
  usable from `.js` Flow source without user plugin registration
- React Compiler-safe motion primitives with reduced-motion defaults
- OpenTUI-aligned native TUI framework targeting a React Ink replacement
- React-minded hooks inspired by VueUse without render-time impurity
- headless RSC-compatible UI primitives with preset styles, shaped to replace
  shadcn's copy-and-edit workflow with typed imports
- validator-backed form UI designed for React Compiler-safe render idempotency
- TanStack Query-class data fetching through `@uniflowed/query`, and
  Next.js-class route metadata, static params, and cache controls
- docs site built through the same RSC/static framework and targeting `void`
- story and VRT contracts with `@uniflowed/mock` and `@uniflowed/browser`
- first publish from local `uf publish`, then tokenless OIDC trusted publish from
  GitHub Actions on `uf@*` tag push
- Rust-owned `@uniflowed/test` runner targeting faster-than-Bun execution while
  delegating JavaScript execution to Node.js, Deno, or Bun hosts
- native test declarations via `import { describe, it, expect } from '@uniflowed/test'`
- React Testing Library-compatible declarations via `@uniflowed/react-testing`
- self-hosted `@uniflowed/pm` package manager with `uf.lock` and a content-addressed store
- `@uniflowed/rm` runtime manager inferred from config, with automatic host
  detection/apply planning
- install via `curl -fsSL https://setup.uniflowed.dev | sh`, with sh, bash, zsh, and
  ush support, on macOS and Linux for both x86_64 and aarch64; Windows artifacts
  are not published yet and `install.ps1` says so rather than failing obscurely
- React Native platform linting and `.native/.ios/.android` split guidance
- editor integration targets for VS Code, Neovim, Emacs, Vim, Helix, Zed, and Cursor
- formatter defaults to double quotes and semicolons; Flow files are parsed by
  the official Flow Rust parser and printed by uf's own Prettier-compatible
  printer, checked against Prettier's output by a fixture corpus. Non-Flow files
  are configured to route to Biome, which is not wired up yet

No Babel, Jest, Yarn, npm scripts, or `.flowconfig` is required for generated
projects. Flow becomes JavaScript through `uf transform`: the official Flow
Rust parser, Flow's own lowering rules for `component`/`hook`/`match`/enums,
the official React Compiler (Rust, `syntax` mode) and oxc for JSX and code
generation — no Babel anywhere. Project automation belongs in `uf.config.js` tasks and runs through
Vite Task.

When config is needed, use the Vite-like entrypoint through Flow syntax. The
file replaces a user-authored `vite.config.ts`; Vite, plugins, lint, format, and
test host selection all flow through this one `.js` config:

```js
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  app: {
    runtime: {
      default: "node",
      capabilityJsHost: {
        hosts: ["node", "deno", "bun"],
      },
    },
  },
  dev: {
    port: 3000,
  },
  fmt: {
    flow: {
      parser: "official-flow-rust",
      printer: "uf-rust",
    },
    nonFlow: {
      formatter: "biome",
    },
  },
  lint: {
    engine: "rust",
    flow: {
      builtins: "mixed",
    },
    rules: {
      "react/component-syntax": "error",
      "react-native/platform-split": "warn",
    },
  },
  test: {
    runner: {
      runtime: "capability-js-host",
      jsHosts: ["node", "deno", "bun"],
    },
  },
  tasks: {
    ci: { command: "cargo test --workspace --all-features" },
  },
});
```

## Native Library Surface

The Flow package surface starts in `packages/core` and is exposed as bundled
`@uniflowed/*` modules:

- `@uniflowed/core`
- `@uniflowed/config`
- `@uniflowed/react`
- `@uniflowed/react-native`
- `@uniflowed/brand`
- `@uniflowed/testing`
- `@uniflowed/test`
- `@uniflowed/react-testing`
- `@uniflowed/router`
- `@uniflowed/server`
- `@uniflowed/web`
- `@uniflowed/query`
- `@uniflowed/fetch`
- `@uniflowed/effect`
- `@uniflowed/orm`
- `@uniflowed/relay`
- `@uniflowed/graphql`
- `@uniflowed/markdown`
- `@uniflowed/temporal`
- `@uniflowed/pwa`
- `@uniflowed/cell`
- `@uniflowed/state`
- `@uniflowed/validator`
- `@uniflowed/hooks`
- `@uniflowed/ui`
- `@uniflowed/mock`
- `@uniflowed/browser`
- `@uniflowed/story`
- `@uniflowed/vrt`
- `@uniflowed/motion`
- `@uniflowed/tui`
- `@uniflowed/cli`
- `@uniflowed/prepare`
- `@uniflowed/pm`
- `@uniflowed/rm`
- `@uniflowed/std`
- `@uniflowed/stylex`
- `@uniflowed/react-compiler`
- `@uniflowed/runtime`
- `@uniflowed/lib`
- `@uniflowed/lint`

The Rust source of truth for that registry is `uf_lib`, so CLI inspection,
docs, and runtime binding can converge on one module table.

Core implementation should remain native Rust where Rust owns the toolchain:
linting, Flow formatting, type checks, test scheduling, and package metadata.
Effect, validator, and cell stay ordinary `.js` Flow packages; user projects are
`.js` files with Flow syntax and `// @flow`, starting from `uf.config.js`; Vite
owns bundling, plugin execution, and the development server behind uniflowed.
Native package artifacts are generated from Rust using a napi-rs-style target
package model, and generated TypeScript declarations are converted back to Flow.

## Development

The repository is pinned to Rust 1.98, and `upstream/flow` carries Meta's
official Flow Rust port as a submodule because those crates are not published to
crates.io.

```sh
nix develop ./tools/nix
tools/upstream/sync.sh
cargo test --workspace
```

`tools/upstream/sync.sh` checks out only `rust_port/` from a shallow, blobless
clone, and nothing builds without it: `uf` parses and type-checks Flow with that
port and has no second backend. Flow's typing crates need
`#![feature(box_patterns)]`, so `rust-toolchain.toml` pins the newest nightly
that still accepts it.

Distribution is configured for Cloudflare:

```sh
curl -fsSL https://setup.uniflowed.dev | sh
nix profile install github:ubugeeei-prod/uf#uf
```

Cloudflare IaC and the release upload layout live in
[`infra/cloudflare`](infra/cloudflare).
The docs site is generated with the same CLI:

```sh
tools/docs/build.sh
```
