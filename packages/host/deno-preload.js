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
// The loader itself is `./internal/deno-hooks.js`, which says why it is built
// on `registerHooks` rather than `register()`, what that costs, and the one
// thing a load hook breaks on Deno.
//
// # What a hook bought over what came before it
//
// Before 2.8 Deno had nothing to install, and uf's Deno loader was an
// ahead-of-time pass: compile every module uf could enumerate into `.uf/deno`
// and hand Deno an import map pointing at the copies. A pass compiles what it
// can list, and a hook is asked about every module — so three gaps closed at
// once when the hook arrived. A module reached by a path computed at run time
// is transformed like any other; `uf test --watch` re-imports with a fresh
// query and the hook is asked again, so there is no stale tree to refuse; and
// a bare specifier resolves through the project's own `node_modules`, which
// Deno already reads, rather than through a map uf had to write.
// `crates/uf_cli/tests/deno_host.rs` starts a real Deno and asks each of those
// questions of this file.

import { registerDenoFlowHooks } from "./internal/deno-hooks.js";

registerDenoFlowHooks();
