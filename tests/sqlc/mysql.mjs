// `mysql2/promise` for `mysql.test.js`, outside the Flow check.
//
// mysql2 declares `Connection` and `Pool` as classes extending
// `QueryableBase(ExecutableBase(EventEmitter))` — TypeScript mixins, which
// uf's declaration translation does not follow yet, so the translated classes
// have no `query` or `execute`. That is a gap in the translation, not in
// mysql2 or the adapter; until it closes, the three calls that need those
// methods live here.

import mysql from "mysql2/promise";

/** Create a database named `name` with `schema` in it. */
export async function createDatabase(url, name, schema) {
  const admin = await mysql.createConnection({ uri: url, multipleStatements: true });
  await admin.query(`DROP DATABASE IF EXISTS ${name}; CREATE DATABASE ${name}; USE ${name};`);
  await admin.query(schema);
  await admin.end();
}

/** Drop it again. */
export async function dropDatabase(url, name) {
  const admin = await mysql.createConnection({ uri: url });
  await admin.query(`DROP DATABASE ${name}`);
  await admin.end();
}

/** A pool on `name`. */
export function pool(url, name) {
  return mysql.createPool({ uri: url, database: name, connectionLimit: 4 });
}
