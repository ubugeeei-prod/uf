#!/bin/sh
# sqlc's Flow target, end to end, over `tests/sqlc`:
#
#   1. the captured requests are what the pinned sqlc sends a plugin;
#   2. that sqlc, running this `uf` as its plugin, writes exactly the golden
#      files (`uf sqlc diff`), which `crates/uf_sqlc/tests/golden.rs` also holds;
#   3. the golden files and the runtime type-check, lint and are formatted;
#   4. they run against SQLite and PostgreSQL (and MySQL when
#      `UF_SQLC_MYSQL_URL` names one, which CI's service container does);
#   5. the SQLite suite runs again under Bun, through `bun:sqlite` (skipped
#      without Bun, except in CI, which installs it).
#
#   UF_BINARY=./target/release/uf sh tools/ci/sqlc.sh
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
uf=$(CDPATH='' cd -- "$(dirname -- "${UF_BINARY:-$root/target/release/uf}")" && pwd)/uf
if [ ! -x "$uf" ]; then
  echo "sqlc.sh: no uf at $uf (set UF_BINARY)" >&2
  exit 2
fi
SQLC=${SQLC:-$(sh "$root/tools/ci/install-sqlc.sh" "$root/target/sqlc")}
export SQLC

cd "$root/tests/sqlc"
npm ci --no-audit --no-fund
node capture.mjs --check
"$uf" sqlc diff -f sqlc.json
"$uf" check
"$uf" lint
"$uf" fmt --check
"$uf" test
if command -v bun >/dev/null 2>&1; then
  "$uf" test --host bun sqlite.test.js
elif [ -n "${CI:-}" ]; then
  echo "sqlc.sh: bun is not installed; CI must run the bun:sqlite suite" >&2
  exit 1
else
  echo "sqlc.sh: bun is not installed, so the bun:sqlite suite did not run" >&2
fi
