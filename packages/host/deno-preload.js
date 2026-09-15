// @noflow
//
// Plain JavaScript: this file registers the loader, so it cannot need one.
//
// `deno run --preload <path>/@uniflowed/host/deno-preload.js app.js` runs a Flow
// project on Deno without a build step: every module uf is responsible for is
// transformed through `uf transform` as Deno loads it. It is the Deno
// counterpart of `./register.js` and `./bun-preload.js`, and the policy of
// which files count is the same `isFlowModule`. A path rather than the package
// specifier, because Deno reads `--preload` as a path.
//
// # The same loader as Node's, and why there is no second one
//
// Deno implements `node:module`'s `registerHooks` from 2.8, and nothing else a
// loader could be installed through: no `register()`, no plugin API. The hooks
// `registerHooks` takes are the in-thread ones `./register.js` installs on a
// Node new enough to have them (`./internal/sync-hooks.js`), so this installs
// exactly those. One loader for both runtimes means one cache key, one framing
// and one answer to "how is a module compiled while the importing thread
// waits" — a transform thread the importer sleeps on — rather than a Deno copy
// of each that would have to be kept equal.
//
// What `./register.js` has and this does not is the fallback to `register()`
// on a runtime without `registerHooks`. On Deno there is nothing to fall back
// to, and `installFlowHooks` says so by version instead of failing with a
// `TypeError` about a missing function.
//
// # What a hook bought over what came before it
//
// Before 2.8 Deno had nothing to install, and uf's Deno loader was an
// ahead-of-time pass: compile every module uf could enumerate into `.uf/deno`
// and hand Deno an import map pointing at the copies. A pass compiles what it
// can list, and a hook is asked about every module — so a module reached by a
// path computed at run time is compiled like any other, `uf test --watch`
// re-imports with a fresh query that the hook is asked about again, and
// `uft.mock` has the synchronous hook it needs. `crates/uf_cli/tests/deno_host.rs`
// starts a real Deno and asks each of those questions of this file.

import { installFlowHooks } from "./internal/sync-hooks.js";
import { environmentVariable } from "./transform.js";

installFlowHooks(environmentVariable("UF_PROJECT_ROOT") ?? process.cwd());
