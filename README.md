# uniflowed

Unified Toolchain for Flow (React).

`uf` is intended to become the all-in-one toolchain for Flow projects: create,
dev server, build, lint, format, type check, test, task running, env inspection,
publishing, LSP, and eventually a Hermes-backed runtime.

The project and ecosystem name is **uniflowed**. The command is **`uf`**.

Roadmap tracking lives in [Issue #1](https://github.com/ubugeeei-prod/uf/issues/1).

![uniflowed logo](assets/brand/uniflowed-logo.svg)

## Commands

```sh
uf create app react my-app
uf create lib my-lib
uf dev
uf build
uf lint
uf fmt
uf check
uf test --list
uf env doctor
uf env use production
uf run <task>
uf inspect
uf publish
uf lsp
uf install
uf upgrade
```

## Defaults

React app projects work without an `uniflowed.config.flow`. The default app preset
enables:

- Flow component/hook style for React
- `app.flow` as the entrypoint
- file-system router rooted at `app`
- reserved `app/_uf.layout.flow`, `app/_uf.page.flow`, and `app/_uf.middleware.flow`
- generated `router.flow` route path and params types
- Server Components by default
- explicit `"use client";` for Client Components
- explicit `"use server";` for server actions
- RSC and server actions
- PPR, SSR, SSG, and ISR support
- route, fetch, action, and data cache defaults OFF
- React 19, Suspense, `use`, and Async React assumptions
- React Compiler with `mode: "syntax"`
- StyleX as the default styling layer
- native query, effect, ORM, Relay, and `flow-cell` builtin modules
- React-minded hooks inspired by VueUse without render-time impurity
- headless RSC-compatible UI primitives with preset styles
- docs site built through the same RSC/static framework and targeting `void`
- native test declarations via `import { describe, it, expect } from '@uniflowed/testing'`
- React Testing Library-compatible declarations via `@uniflowed/react-testing`
- React Native platform linting and `.native/.ios/.android` split guidance

No Babel, Jest, Yarn, or `.flowconfig` is required for generated projects.

When config is needed, use the Vite-like entrypoint through Flow syntax:

```js
import { defineConfig } from '@uniflowed/config';

export default defineConfig({
  dev: {
    port: 3000,
  },
  lint: {
    rules: {
      'react/component-syntax': 'error',
      'react-native/platform-split': 'warn',
    },
  },
  tasks: {
    ci: 'cargo test --workspace --all-features',
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
- `@uniflowed/testing`
- `@uniflowed/react-testing`
- `@uniflowed/router`
- `@uniflowed/server`
- `@uniflowed/query`
- `@uniflowed/effect`
- `@uniflowed/orm`
- `@uniflowed/relay`
- `@uniflowed/flow-cell`
- `@uniflowed/hooks`
- `@uniflowed/ui`
- `@uniflowed/stylex`
- `@uniflowed/react-compiler`
- `@uniflowed/runtime`
- `@uniflowed/lib`
- `@uniflowed/lint`

The Rust source of truth for that registry is `uniflowed_lib`, so CLI inspection,
docs, and runtime binding can converge on one module table.

Core implementation should remain native Rust. Flow is the user-facing config
syntax via `uniflowed.config.flow`; Vite and Rolldown stay hidden behind uniflowed.

## Development

The repository is pinned to Rust 1.98.

```sh
nix develop
cargo test --workspace --all-features
```

The first install target is:

```sh
curl -fsSL https://uniflowed.dev | sh
```

The installer is not published yet.
