// @flow
//
// sqlc's Flow target, checked end to end. `cases/*/gen` is what `uf` wrote
// for each case (the same files `crates/uf_sqlc/tests/golden.rs` compares
// against), and this project type-checks them, lints them and runs them
// against real databases. `tools/ci/sqlc.sh` is the whole check.

import type { UniflowedConfig } from "@uniflowed/config";
import { defineConfig } from "@uniflowed/config";

const config: UniflowedConfig = defineConfig({
  // `capture.mjs` drives sqlc, `bridge.mjs` reaches into PGlite-socket and
  // `mysql.mjs` stands in for types uf cannot translate yet;
  // all three are plain JavaScript, and say why at their tops.
  ignore: ["node_modules", "capture.mjs", "bridge.mjs", "mysql.mjs"],
});
export default config;
