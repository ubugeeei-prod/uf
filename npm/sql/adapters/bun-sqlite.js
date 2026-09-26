// @flow
//
// `@uniflowed/sql/bun-sqlite`: Bun's built-in `bun:sqlite`.
//
//   import { Database } from "bun:sqlite";
//   import { fromBunSqlite } from "@uniflowed/sql/bun-sqlite";
//
//   const db = fromBunSqlite(new Database("app.db"));
//
// Safe integers are turned on per statement, so the `Database` does not have
// to be opened with `safeIntegers: true` — though it does no harm.

import type { Queryable, SqlParam } from "../index.js";
import { SqlError, singleConnection } from "../index.js";
import { DEFAULT_STATEMENT_CACHE, count, rowId, statementCache } from "../internal/statements.js";

/** The part of `bun:sqlite`'s `Statement` this uses. */
export interface BunSqliteStatement {
  values(...params: $ReadOnlyArray<SqlParam>): $ReadOnlyArray<mixed>;
  run(...params: $ReadOnlyArray<SqlParam>): {
    readonly changes: number | bigint,
    readonly lastInsertRowid: number | bigint,
    ...
  };
  safeIntegers(enabled: boolean): mixed;
}

/** The part of `bun:sqlite`'s `Database` this uses. */
export interface BunSqliteDatabase {
  prepare(text: string): BunSqliteStatement;
}

export type BunSqliteOptions = {|
  readonly begin?: string,
  readonly statementCache?: number,
  readonly maxParams?: number,
|};

/** A [`Queryable`] over one `bun:sqlite` `Database`. */
export function fromBunSqlite(
  database: BunSqliteDatabase,
  options: BunSqliteOptions = {},
): Queryable {
  const statement = statementCache((text) => {
    const prepared = database.prepare(text);
    prepared.safeIntegers(true);
    return prepared;
  }, options.statementCache ?? DEFAULT_STATEMENT_CACHE);
  return singleConnection({
    engine: "sqlite",
    maxParams: options.maxParams ?? 32766,
    begin: options.begin,
    run: async (text, params, mode) => {
      const prepared = statement(text);
      if (mode === "rows") {
        const rows: Array<$ReadOnlyArray<mixed>> = [];
        for (const row of prepared.values(...params)) {
          if (!Array.isArray(row)) {
            throw new SqlError({ kind: "unsupported", feature: "bun:sqlite rows as objects" });
          }
          rows.push(row);
        }
        return { rows, rowsAffected: 0, lastInsertId: null };
      }
      const result = prepared.run(...params);
      return {
        rows: [],
        rowsAffected: count(result.changes),
        lastInsertId: rowId(result.lastInsertRowid),
      };
    },
  });
}
