<div align="center">
  
<img width="200" src="assets/uf.png" alt="uf logo">

# uf

Unified Toolchain for Flow (React)

</div>



`uf` stands for unifiedflow: unified flow for React applications, libraries, and
tooling. It is intended to become the all-in-one toolchain for Flow projects:
create, dev server, build, lint, format, type check, test, task running, env
inspection, publishing, LSP, and runtime execution.

The package scope and ecosystem name is **uniflowed**. The command is **`uf`**.

The bar is explicit: beat Vite+ on Flow React DX, framework completeness, and
build/dev performance; beat Bun on native test/runtime/package-manager
performance and integrated feature coverage.

Roadmap tracking lives in [Issue #1](https://github.com/ubugeeei-prod/uf/issues/1).

## Usage

```sh
curl -fsSL https://setup.uniflowed.dev | sh
uf create
uf dev
uf build
uf lint
uf fmt
uf check
uf test
```

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
- `uf` runtime by default, with deploy-anywhere adapters for Node.js, Bun, Deno,
  edge, serverless, static, and container targets
- WinterTC-aligned Flow runtime direction on Hermes
- Rust-native server, event loop, and libuv-level IO contracts
- XDG-compliant config, data, cache, state, runtime, and shim locations
- StyleX as the default styling layer
- explicit ofetch-style `@uniflowed/fetch`; no global fetch override
- Relay-based `@uniflowed/graphql`
- Nuxt-like `Font`, `Image`, `OgImage`, `Link`, `Page`, `Layout`, `Time`,
  `Announcer`, and `Picture` primitives
- fully type-safe `useRoute`, `useRouter`, and navigation guards
- `useCookie` and `useHead`
- `uf prepare` for lint-staged-compatible checks and code generation
- ox-content wasm-backed `@uniflowed/markdown`
- lite Temporal and PWA primitives
- native stdlib modules for os, net, dns, path, stream, URL, wasm, glob, motion,
  TUI, cron, S3, SigV4, worker/lambda functions, uuid, and zip
- all route, fetch, image, font, markdown, and PWA caches are opt-in
- native query, effect, ORM, Relay, validator, and state/flow-cell builtin modules
- React Compiler-safe motion primitives with reduced-motion defaults
- OpenTUI-aligned native TUI framework targeting a React Ink replacement
- React-minded hooks inspired by VueUse without render-time impurity
- headless RSC-compatible UI primitives with preset styles
- validator-backed form UI designed for React Compiler-safe render idempotency
- docs site built through the same RSC/static framework and targeting `void`
- story and VRT contracts with `@uniflowed/mock` and `@uniflowed/browser`
- first publish from local `uf publish`, then tokenless OIDC trusted publish from
  GitHub Actions on `uf@*` tag push
- self-hosted `@uniflowed/test` runner targeting faster-than-Bun execution
- native test declarations via `import { describe, it, expect } from '@uniflowed/test'`
- React Testing Library-compatible declarations via `@uniflowed/react-testing`
- self-hosted `@uniflowed/pm` package manager with `uf.lock` and a content-addressed store
- `@uniflowed/rm` runtime manager inferred from config, with automatic acquire/apply planning
- install via `curl -fsSL https://setup.uniflowed.dev | sh`, with sh, bash, zsh, ush,
  Windows, macOS, and Linux targets
- React Native platform linting and `.native/.ios/.android` split guidance
- editor integration targets for VS Code, Neovim, Emacs, Vim, Helix, Zed, and Cursor
- formatter defaults to double quotes and semicolons

No Babel, Jest, Yarn, npm scripts, or `.flowconfig` is required for generated
projects. Project automation belongs in `uf.config.js` tasks and runs through
the uf Vite Task-compatible runner.

When config is needed, use the Vite-like entrypoint through Flow syntax:

```js
import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  dev: {
    port: 3000,
  },
  lint: {
    rules: {
      "react/component-syntax": "error",
      "react-native/platform-split": "warn",
    },
  },
  tasks: {
    ci: { command: "cargo test --workspace --all-features" },
  },
});
```

## Native Library Surface

The Flow package surface starts in `crates/uf_lib/lib/core` and is exposed as bundled
`@uniflowed/*` modules:

- `@uniflowed/core`
- `@uniflowed/config`
- `@uniflowed/react`
- `@uniflowed/react-native`
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
- `@uniflowed/flow-cell`
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

Core implementation should remain native Rust. User projects are `.js` files
with Flow syntax and `// @flow`, starting from `uf.config.js`; Vite and
Rolldown stay hidden behind uniflowed.
Native package artifacts are generated from Rust using a napi-rs-style target
package model, and generated TypeScript declarations are converted back to Flow.

## Development

The repository is pinned to Rust 1.98.

```sh
nix develop ./tools/nix
cargo test --workspace --all-features
```

The installer is not published yet. The target endpoint is:

```sh
curl -fsSL https://setup.uniflowed.dev | sh
```
