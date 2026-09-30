// @flow
//
// sqlc's Flow target, checked end to end. `cases/*/gen` is what `uf` wrote
// for each case (the same files `crates/uf_sqlc/tests/golden.rs` compares
// against), and this project type-checks them, lints them and runs them
// against real databases. `tools/ci/sqlc.sh` is the whole check.

import { defineConfig } from "@uniflowed/config";

export default defineConfig({
  // Keep the checked-in integration tests in their existing layout.
  fmt: { align: false },
  // `capture.mjs` drives sqlc, `bridge.mjs` reaches into PGlite-socket and
  // `mysql.mjs` stands in for types uf cannot translate yet;
  // all three are plain JavaScript, and say why at their tops.
  ignore: ["node_modules", "capture.mjs", "bridge.mjs", "mysql.mjs", "external-adapters.mjs"],
});
