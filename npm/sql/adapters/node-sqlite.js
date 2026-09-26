// @flow
//
// `@uniflowed/sql/node-sqlite`: Node's built-in `node:sqlite`.
//
//   import { DatabaseSync } from "node:sqlite";
//   import { fromNodeSqlite } from "@uniflowed/sql/node-sqlite";
//
//   const db = fromNodeSqlite(new DatabaseSync("app.db"));
//
// Needs `StatementSync#setReturnArrays`, which Node added in 24.0 and 22.16.
// An older Node gets an error that says so, rather than rows keyed by column
// name — which would silently merge two columns that share one.

import type { Queryable, SqlParam } from "../index.js";
import { SqlError, singleConnection } from "../index.js";
import { DEFAULT_STATEMENT_CACHE, count, rowId, statementCache } from "../internal/statements.js";

/** The part of `node:sqlite`'s `StatementSync` this uses. */
export interface NodeSqliteStatement {
  all(...params: $ReadOnlyArray<SqlParam>): $ReadOnlyArray<mixed>;
  run(...params: $ReadOnlyArray<SqlParam>): {
    readonly changes: number | bigint,
    readonly lastInsertRowid: number | bigint,
    ...
  };
  setReadBigInts(enabled: boolean): void;
  setReturnArrays(enabled: boolean): void;
}

/** The part of `node:sqlite`'s `DatabaseSync` this uses. */
export interface NodeSqliteDatabase {
  prepare(text: string): NodeSqliteStatement;
}

export type NodeSqliteOptions = {|
  /** How a transaction starts. `BEGIN IMMEDIATE` takes the write lock up front. */
  readonly begin?: string,
  /** Prepared statements kept; 0 turns the cache off. */
  readonly statementCache?: number,
  /** `SQLITE_MAX_VARIABLE_NUMBER` if the build changed it. */
  readonly maxParams?: number,
|};

function prepared(database: NodeSqliteDatabase, text: string): NodeSqliteStatement {
  const statement = database.prepare(text);
  statement.setReadBigInts(true);
  // Declared, so that a Node without it is refused with a reason rather than
  // failing on the call below.
  if (!("setReturnArrays" in statement)) {
    throw new SqlError({
      kind: "unsupported",
      feature: "node:sqlite without StatementSync#setReturnArrays (Node 24.0 or 22.16 and newer)",
    });
  }
  statement.setReturnArrays(true);
  return statement;
}

/** A [`Queryable`] over one `DatabaseSync`. */
export function fromNodeSqlite(
  database: NodeSqliteDatabase,
  options: NodeSqliteOptions = {},
): Queryable {
  const statement = statementCache(
    (text) => prepared(database, text),
    options.statementCache ?? DEFAULT_STATEMENT_CACHE,
  );
  return singleConnection({
    engine: "sqlite",
    maxParams: options.maxParams ?? 32766,
    begin: options.begin,
    run: async (text, params, mode) => {
      const prepared = statement(text);
      if (mode === "rows") {
        const rows = prepared.all(...params);
        const arrays: Array<$ReadOnlyArray<mixed>> = [];
        for (const row of rows) {
          if (!Array.isArray(row)) {
            throw new SqlError({ kind: "unsupported", feature: "node:sqlite rows as objects" });
          }
          arrays.push(row);
        }
        return { rows: arrays, rowsAffected: 0, lastInsertId: null };
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
