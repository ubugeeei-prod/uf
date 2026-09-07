// @flow
//
// Root entry point of `@uniflowed/core`.
//
// This module exists for discoverability only. It re-exports, by name, exactly
// the surface `uf_lib`'s Rust registry promises for the `@uniflowed/core`
// specifier — the test API and the config entry point. Everything else lives
// behind a subpath, and subpath imports are the supported entry points.
//
// Four of those subpaths are this package's own code rather than a
// re-export, and they are here rather than anywhere else for one reason: every
// layer above needs them, and `@uniflowed/core` is the only package all of
// those layers are allowed to depend on. `@uniflowed/hooks` is published and
// `@uniflowed/temporal` is not, so a clock that lived in the second could not
// be read by the first without `npm install @uniflowed/hooks` resolving to
// nothing — `tools/ci/publishable.sh` refuses exactly that, and it is a real
// constraint rather than a filing preference.
//
// - `./native` — the "this needs the uf binary" contract, and the phantom
//   carriers behind every package's opaque handles.
// - `./clock` — the one place uf reads the time, so that a server render and
//   the hydration that follows it can be made to agree.
// - `./random` — a seeded stream, for the same reason, plus `useId`'s half of
//   the problem that `useId` does not cover.
// - `./temporal` — Temporal itself, native where the host has it and
//   implemented here where it does not.
//
// The two re-exported surfaces are implemented in `@uniflowed/test` and
// `@uniflowed/config` and reached by specifier, not by relative path: each
// `packages/*` directory is published as its own npm package, so a relative
// path into a sibling names a file that exists in this repository and nowhere
// in an installed tree.
//
// There is deliberately no `export *` here. A star re-export forces a bundler
// to keep the whole module graph reachable from this file, and a whole-surface
// barrel could not exist anyway: several domains legitimately export the same
// name (`graphql`, `Image`, `Markdown`, `plan`, `Text`, `contract`, ...).

export type { TestBody, TestOptions } from "@uniflowed/test";
export {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  fn,
  it,
  test,
} from "@uniflowed/test";

export type {
  CapabilityJsHost,
  RuleLevel,
  TaskDefinition,
  UniflowedConfig,
} from "@uniflowed/config";
export { defineConfig } from "@uniflowed/config";
