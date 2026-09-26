// @flow
//
// `@uniflowed/sql/d1`: a Cloudflare D1 binding.
//
//   import { fromD1 } from "@uniflowed/sql/d1";
//
//   export default { fetch: (request, env) => handle(request, fromD1(env.DB)) };
//
// Experimental, and narrower than the other adapters in three ways D1 decides:
//
//   * **No interactive transactions.** The returned `Queryable` has no
//     `transaction`, so `:batch*` and `:copyfrom` run their statements in order
//     without one. Use D1's own `batch()` where atomicity matters.
//   * **100 bound parameters per statement**, which `:copyfrom` chunks under.
//   * **No `bigint`.** D1 returns every INTEGER as a `number`, so the checked
//     `INTEGER` codec throws on a value past 2^53 instead of rounding it, and a
//     `bigint` parameter is sent as its digits, which INTEGER affinity stores
//     exactly.

import type { Queryable, SqlParam } from "../index.js";
import { SqlError } from "../index.js";
import { count, rowId } from "../internal/statements.js";

/** The part of a D1 prepared statement this uses. */
export interface D1Statement {
  bind(...params: $ReadOnlyArray<mixed>): D1Statement;
  raw(): Promise<$ReadOnlyArray<mixed>>;
  run(): Promise<{
    readonly meta: { readonly changes?: number, readonly last_row_id?: number, ... },
    ...
  }>;
}

/** The part of a D1 database binding this uses. */
export interface D1Database {
  prepare(text: string): D1Statement;
}

function toD1(param: SqlParam): mixed {
  if (typeof param === "bigint") {
    return String(param);
  }
  return param;
}

/** A [`Queryable`] over a D1 binding. */
export function fromD1(database: D1Database): Queryable {
  return {
    engine: "sqlite",
    maxParams: 100,
    query: async (text, params, mode) => {
      const statement = database.prepare(text).bind(...params.map(toD1));
      if (mode === "rows") {
        const rows: Array<$ReadOnlyArray<mixed>> = [];
        for (const row of await statement.raw()) {
          if (!Array.isArray(row)) {
            throw new SqlError({ kind: "unsupported", feature: "D1 rows as objects" });
          }
          rows.push(row);
        }
        return { rows, rowsAffected: 0, lastInsertId: null };
      }
      const { meta } = await statement.run();
      return {
        rows: [],
        rowsAffected: count(meta.changes ?? 0),
        lastInsertId: rowId(meta.last_row_id),
      };
    },
  };
}
