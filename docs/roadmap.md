# Roadmap

## P0: Toolchain Spine

- Finish parser/typechecker integration against Flow's maintained parser and
  type checker boundary.
- Replace whitespace formatter core with a Flow AST printer.
- Wire `uf build` and `uf dev` to a Vite-compatible plugin container and
  Rolldown-backed production build.
- Add LSP JSON-RPC loop over config, parser diagnostics, formatter, and lints.

## P1: Native App Framework

- File-system router compiler for web and React Native targets.
- RSC module graph split.
- Server action transform and request bridge.
- StyleX transform as the default style engine.
- React Compiler syntax-mode pass.
- ORM and `flow-cell` runtime primitives.

## P2: Native Test Runner

- Implement JavaScript execution backend.
- Add React DOM and React Native renderers.
- Add Testing Library-compatible queries and user events.
- Add watch mode with dependency-aware reruns.

## P3: Package And Runtime

- Native package resolver, lockfile, content-addressed cache, and installer.
- `uf install` and `uf upgrade`.
- Hermes-backed `uf index.js` runtime.
- `curl -fsSL https://uniflowed.dev | sh` installer.
