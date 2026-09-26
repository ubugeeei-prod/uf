// @flow
//
// `@uniflowed/sql/mysql2`: the `mysql2` package's promise API.
//
//   import mysql from "mysql2/promise";
//   import { fromMysql2Pool } from "@uniflowed/sql/mysql2";
//
//   const db = fromMysql2Pool(mysql.createPool(url));
//
// `tests/sqlc/mysql.test.js` runs it against the MySQL 8.4 server CI's
// `sqlc Flow target` job provides; locally it needs `UF_SQLC_MYSQL_URL`.
//
// Statements are server-side prepared (`execute`) and rows are arrays. Each
// statement asks for big numbers and dates as strings, and its `typeCast`
// reads `JSON` and `DECIMAL` columns as their text: the text MySQL sent, which
// `@uniflowed/sql/mysql` decodes, whatever the pool was configured with.
// mysql2 reads `jsonStrings` and `decimalNumbers` from the connection's
// config only, so asking for them per statement would be silently ignored,
// and a pool opened without them would hand the codecs a parsed object.
//
// A parameterless statement run for its effect goes over the text protocol
// (`query`) instead: MySQL refuses to prepare `SAVEPOINT`, `RELEASE
// SAVEPOINT` and `ROLLBACK TO SAVEPOINT`, and such a statement returns no rows
// whose representation could differ.

import type { Connection, QueryResult, Queryable, Run } from "../index.js";
import { transactionOn } from "../index.js";

/** What a `typeCast` function is handed for one column. */
export type Mysql2Field = {
  readonly type: string,
  string(encoding?: string): string | null,
  ...
};

type Mysql2Options = {|
  readonly sql: string,
  readonly values: $ReadOnlyArray<mixed>,
  readonly rowsAsArray: true,
  readonly supportBigNumbers: true,
  readonly bigNumberStrings: true,
  readonly dateStrings: true,
  readonly typeCast: (field: Mysql2Field, next: () => mixed) => mixed,
|};

type Mysql2Result =
  | $ReadOnlyArray<mixed>
  | { readonly affectedRows: number, readonly insertId: number | string, ... };

/** A `mysql2/promise` connection. */
export interface Mysql2Connection {
  execute(options: Mysql2Options): Promise<[Mysql2Result, mixed]>;
  query(options: Mysql2Options): Promise<[Mysql2Result, mixed]>;
}

/** A connection checked out of a `mysql2/promise` pool. */
export interface Mysql2PoolConnection extends Mysql2Connection {
  release(): void;
}

/** A `mysql2/promise` pool. */
export interface Mysql2Pool extends Mysql2Connection {
  getConnection(): Promise<Mysql2PoolConnection>;
}

/**
 * `JSON` and `DECIMAL` columns as the text MySQL sent; everything else as the
 * statement's options already ask. `JSON` arrives with the binary charset, so
 * it is read as UTF-8, which is what the JSON type stores.
 */
export function typeCast(field: Mysql2Field, next: () => mixed): mixed {
  switch (field.type) {
    case "JSON":
      return field.string("utf8");
    case "NEWDECIMAL":
    case "DECIMAL":
      return field.string();
    default:
      return next();
  }
}

function lastId(value: number | string): bigint | null {
  if (typeof value === "string" && /^\d+$/.test(value)) {
    return BigInt(value);
  }
  if (typeof value === "number" && Number.isSafeInteger(value)) {
    return BigInt(value);
  }
  return null;
}

function runOn(connection: Mysql2Connection): Run {
  return async (text, params, mode): Promise<QueryResult> => {
    const options: Mysql2Options = {
      sql: text,
      values: params,
      rowsAsArray: true,
      supportBigNumbers: true,
      bigNumberStrings: true,
      dateStrings: true,
      typeCast,
    };
    const [result] =
      mode === "exec" && params.length === 0
        ? await connection.query(options)
        : await connection.execute(options);
    if (Array.isArray(result)) {
      const rows: Array<$ReadOnlyArray<mixed>> = [];
      for (const row of result) {
        if (Array.isArray(row)) {
          rows.push(row);
        }
      }
      return { rows, rowsAffected: 0, lastInsertId: null };
    }
    return { rows: [], rowsAffected: result.affectedRows, lastInsertId: lastId(result.insertId) };
  };
}

function connection(client: Mysql2Connection): Connection {
  return { engine: "mysql", maxParams: 65535, run: runOn(client) };
}

/** A [`Queryable`] over a `mysql2/promise` pool. */
export function fromMysql2Pool(pool: Mysql2Pool): Queryable {
  return {
    engine: "mysql",
    maxParams: 65535,
    query: runOn(pool),
    transaction: async <T>(body: (tx: Queryable) => Promise<T>): Promise<T> => {
      const client = await pool.getConnection();
      try {
        return await transactionOn(connection(client), body);
      } finally {
        client.release();
      }
    },
  };
}
