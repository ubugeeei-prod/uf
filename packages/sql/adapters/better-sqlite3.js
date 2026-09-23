// @flow
//
// `@uniflowed/sql/better-sqlite3`: the `better-sqlite3` package.
//
//   import Database from "better-sqlite3";
//   import { fromBetterSqlite3 } from "@uniflowed/sql/better-sqlite3";
//
//   const db = fromBetterSqlite3(new Database("app.db"));
//
// Experimental: typed against better-sqlite3's documented `Statement` API
// (`raw`, `safeIntegers`, `reader`); this repository's CI does not install it.

import type { Queryable, SqlParam } from "../index.js";
import { SqlError, singleConnection } from "../index.js";
import { DEFAULT_STATEMENT_CACHE, count, rowId, statementCache } from "../internal/statements.js";

/** The part of better-sqlite3's `Statement` this uses. */
export interface BetterSqlite3Statement {
  readonly reader: boolean;
  all(...params: $ReadOnlyArray<SqlParam>): $ReadOnlyArray<mixed>;
  run(...params: $ReadOnlyArray<SqlParam>): {
    readonly changes: number,
    readonly lastInsertRowid: number | bigint,
    ...
  };
  raw(enabled: boolean): mixed;
  safeIntegers(enabled: boolean): mixed;
}

/** The part of better-sqlite3's `Database` this uses. */
export interface BetterSqlite3Database {
  prepare(text: string): BetterSqlite3Statement;
}

export type BetterSqlite3Options = {|
  readonly begin?: string,
  readonly statementCache?: number,
  readonly maxParams?: number,
|};

/** A [`Queryable`] over one better-sqlite3 `Database`. */
export function fromBetterSqlite3(
  database: BetterSqlite3Database,
  options: BetterSqlite3Options = {},
): Queryable {
  const statement = statementCache((text) => {
    const prepared = database.prepare(text);
    prepared.safeIntegers(true);
    // `raw` throws on a statement that returns no data, so only a reader
    // gets it — and only a reader is ever asked for rows.
    if (prepared.reader) {
      prepared.raw(true);
    }
    return prepared;
  }, options.statementCache ?? DEFAULT_STATEMENT_CACHE);
  return singleConnection({
    engine: "sqlite",
    maxParams: options.maxParams ?? 32766,
    begin: options.begin,
    run: async (text, params, mode) => {
      const prepared = statement(text);
      if (mode === "rows" && prepared.reader) {
        const rows: Array<$ReadOnlyArray<mixed>> = [];
        for (const row of prepared.all(...params)) {
          if (!Array.isArray(row)) {
            throw new SqlError({ kind: "unsupported", feature: "better-sqlite3 rows as objects" });
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
