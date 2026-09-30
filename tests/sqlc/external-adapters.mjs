// @noflow
// Native better-sqlite3 and workerd's real D1 binding, through generated Flow.
import assert from "node:assert/strict";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { fromBetterSqlite3 } from "@uniflowed/sql/better-sqlite3";
import { fromD1 } from "@uniflowed/sql/d1";
import { SqlError, exec } from "@uniflowed/sql";
import * as authors from "./cases/authors-sqlite/gen/query.sql.js";
import { schemaOf } from "./databases.js";
import { transactionScenario } from "./transactions.js";

async function queries(db, label) {
  const created = await authors.createAuthor(db, { name: "Ada", bio: null });
  assert.equal(created.rowsAffected, 1);
  assert.equal(created.lastInsertId, 1n);
  assert.deepEqual(await authors.getAuthor(db, { id: 1 }), { id: 1, name: "Ada", bio: null });
  await authors.createAuthor(db, { name: "Grace", bio: "compiler" });
  assert.deepEqual((await authors.listAuthors(db)).map((row) => row.name), ["Ada", "Grace"]);
  await authors.deleteAuthor(db, { id: 1 });
  assert.equal(await authors.getAuthor(db, { id: 1 }), null);
  await authors.deleteAuthor(db, { id: 2 });
  await db.query("INSERT INTO authors (id, name) VALUES (?, ?)", [9007199254740991n, "exact"], "exec");
  assert.equal((await authors.listAuthors(db))[0].id, 9007199254740991);
  await db.query("DELETE FROM authors", [], "exec");
  // Bind an exact 64-bit integer; generated number codecs must refuse it.
  await db.query("INSERT INTO authors (id, name) VALUES (?, ?)", [9007199254740993n, "too large"], "exec");
  await assert.rejects(authors.listAuthors(db), (error) => error instanceof SqlError && /INTEGER within/.test(error.message));
  await db.query("DELETE FROM authors", [], "exec");
  await db.query("DELETE FROM sqlite_sequence WHERE name = 'authors'", [], "exec");
  await assert.rejects(exec(db, "Broken", "INSERT INTO missing VALUES (?)", [1]), /missing/);
  console.log(`${label}: generated CRUD, nulls, exact integers and errors passed`);
}

// v13 ships the addons in its tarball. The platform entry can only load that
// prebuild, so a node-gyp fallback cannot make this check pass.
const { default: Database } = await import(`better-sqlite3/${process.platform}-${process.arch}`);
const sqlite = new Database(":memory:");
try {
  sqlite.exec(schemaOf("authors-sqlite"));
  const db = fromBetterSqlite3(sqlite);
  await queries(db, "better-sqlite3");
  await transactionScenario(db,
    (tx, name) => authors.createAuthor(tx, { name, bio: null }),
    async (tx) => (await authors.listAuthors(tx)).map((row) => row.name));
  const raw = await db.query("SELECT ?", [9007199254740993n], "rows");
  assert.equal(raw.rows[0][0], 9007199254740993n);
  console.log("better-sqlite3: commit, rollback, nested savepoints and closed transactions passed");
} finally {
  sqlite.close();
}

const mf = new Miniflare(convertV4MiniflareOptions({
  modules: true,
  script: 'export default { fetch() { return new Response("ok"); } };',
  compatibilityDate: "2026-09-01",
  d1Databases: { DB: "uf-sqlc" },
}));
try {
  const binding = await mf.getD1Database("DB");
  await binding.exec(schemaOf("authors-sqlite").replace(/\s+/g, " "));
  const db = fromD1(binding);
  assert.equal(db.transaction, undefined, "D1 does not support interactive transactions");
  assert.equal(db.maxParams, 100);
  await queries(db, "D1 (workerd)");
  const blob = await db.query("SELECT ?", [new Uint8Array([1, 2, 3])], "rows");
  assert.deepEqual(Array.from(blob.rows[0][0]), [1, 2, 3]);
  // Atomicity belongs to D1's batch API, not an invented interactive transaction.
  await assert.rejects(binding.batch([
    binding.prepare("INSERT INTO authors (id, name) VALUES (?, ?)").bind(7, "first"),
    binding.prepare("INSERT INTO authors (id, name) VALUES (?, ?)").bind(7, "duplicate"),
  ]));
  assert.deepEqual(await authors.listAuthors(db), []);
  console.log("D1: binary binding and native batch rollback passed; interactive transactions are unavailable");
} finally {
  await mf.dispose();
}
