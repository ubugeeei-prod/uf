#!/bin/sh
# Generate through sqlc's WASI transport, using the artifact's actual digest.
set -eu
root=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
wasm=${1:?Usage: sqlc-wasm.sh /absolute/path/sqlc-gen-flow.wasm}
uf=${UF_BINARY:-$root/target/release/uf}
sqlc=${SQLC:-$(sh "$root/tools/ci/install-sqlc.sh" "$root/target/sqlc")}
scratch=$(mktemp -d "$root/tests/sqlc/.wasm-smoke.XXXXXX")
trap 'rm -rf "$scratch"' EXIT HUP INT TERM
cp "$root/tests/sqlc/cases/authors-sqlite/schema.sql" "$scratch/schema.sql"
cp "$root/tests/sqlc/cases/authors-sqlite/query.sql" "$scratch/query.sql"
node --input-type=module - "$scratch" "$wasm" <<'JS'
import fs from 'node:fs';
import crypto from 'node:crypto';
import { pathToFileURL } from 'node:url';
const [directory, wasm] = process.argv.slice(2);
const sha256 = crypto.createHash('sha256').update(fs.readFileSync(wasm)).digest('hex');
fs.writeFileSync(`${directory}/sqlc.json`, JSON.stringify({
  version: '2', plugins: [{ name: 'flow', wasm: { url: pathToFileURL(wasm).href, sha256 } }],
  sql: [{ engine: 'sqlite', schema: 'schema.sql', queries: 'query.sql', codegen: [{ plugin: 'flow', out: 'gen' }] }],
}));
JS
(cd "$scratch" && "$sqlc" generate -f sqlc.json)
# A real generated query, compiled by uf's loader and run against SQLite.
cd "$root/tests/sqlc"
UF_BINARY="$uf" UF_PROJECT_ROOT="$root/tests/sqlc" node --import @uniflowed/host/register --input-type=module - "$scratch" <<'JS'
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { pathToFileURL } from 'node:url';
import { DatabaseSync } from 'node:sqlite';
import { fromNodeSqlite } from '@uniflowed/sql/node-sqlite';
const directory = process.argv[2];
const query = await import(pathToFileURL(`${directory}/gen/query.sql.js`).href);
const database = new DatabaseSync(':memory:');
try {
  database.exec(fs.readFileSync(`${directory}/schema.sql`, 'utf8'));
  const db = fromNodeSqlite(database);
  await query.createAuthor(db, { name: 'WASI', bio: null });
  assert.deepEqual(await query.getAuthor(db, { id: 1 }), { id: 1, name: 'WASI', bio: null });
} finally { database.close(); }
console.log('Pinned sqlc WASM generation and generated SQLite query passed');
JS
