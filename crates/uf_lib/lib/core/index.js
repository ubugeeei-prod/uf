// @flow
//
// Root entry point of `@uniflowed/core`.
//
// This module exists for discoverability only. It re-exports, by name, exactly
// the surface `uf_lib`'s Rust registry promises for the `@uniflowed/core`
// specifier — the test API and the config entry point. Everything else lives
// behind a subpath, and subpath imports are the supported entry points:
// `@uniflowed/core/effect`, `@uniflowed/core/ui`, `@uniflowed/core/tui`, ...
//
// There is deliberately no `export *` here. A star re-export forces a bundler
// to keep the whole module graph reachable from this file, and a whole-surface
// barrel could not exist anyway: several domains legitimately export the same
// name (`graphql`, `Image`, `Markdown`, `plan`, `Text`, `contract`, ...).

export type { Expectation, TestBody } from "./testing.js";
export { afterEach, beforeEach, describe, expect, it, test } from "./testing.js";

export type { RuleLevel, TaskDefinition, UniflowedConfig } from "./config.js";
export { defineConfig } from "./config.js";
