// @flow
//
// `@uniflowed/sql/pg`: node-postgres.
//
//   import pg from "pg";
//   import { fromPgPool } from "@uniflowed/sql/pg";
//
//   const db = fromPgPool(new pg.Pool({ connectionString }));
//
// Every query asks for array rows and replaces node-postgres's type parsers
// with the identity for that query alone, so the pool's own configuration is
// left as the application set it, and generated code still sees the server's
// text. A transaction checks a client out of the pool for its duration.

import type { Connection, QueryResult, Queryable, Run } from "../index.js";
import { singleConnection, transactionOn } from "../index.js";

type PgResult = { readonly rows: $ReadOnlyArray<mixed>, readonly rowCount: number | null, ... };

type PgQuery = {|
  readonly text: string,
  readonly values: $ReadOnlyArray<mixed>,
  readonly rowMode: "array",
  readonly types: {| readonly getTypeParser: () => (value: string) => string |},
|};

/** A connected `pg.Client` or a client checked out of a pool. */
export interface PgClient {
  query(config: PgQuery): Promise<PgResult>;
}

/** A client checked out of a `pg.Pool`. */
export interface PgPoolClient extends PgClient {
  release(error?: mixed): void;
}

/** A `pg.Pool`. */
export interface PgPool {
  query(config: PgQuery): Promise<PgResult>;
  connect(): Promise<PgPoolClient>;
}

const RAW = { getTypeParser: () => (value: string) => value };

function runOn(client: PgClient): Run {
  return async (text, params): Promise<QueryResult> => {
    const result = await client.query({ text, values: params, rowMode: "array", types: RAW });
    const rows: Array<$ReadOnlyArray<mixed>> = [];
    for (const row of result.rows) {
      if (Array.isArray(row)) {
        rows.push(row);
      }
    }
    return { rows, rowsAffected: result.rowCount ?? 0, lastInsertId: null };
  };
}

function connection(client: PgClient): Connection {
  return { engine: "postgresql", maxParams: 65535, run: runOn(client) };
}

/** A [`Queryable`] over one connected `pg.Client`, which it serialises. */
export function fromPgClient(client: PgClient): Queryable {
  return singleConnection(connection(client));
}

/** A [`Queryable`] over a `pg.Pool`. */
export function fromPgPool(pool: PgPool): Queryable {
  return {
    engine: "postgresql",
    maxParams: 65535,
    query: runOn(pool),
    transaction: async <T>(body: (tx: Queryable) => Promise<T>): Promise<T> => {
      const client = await pool.connect();
      let broken: mixed = undefined;
      try {
        return await transactionOn(connection(client), body);
      } catch (error) {
        broken = error;
        throw error;
      } finally {
        // Handing the error back makes the pool discard a connection that may
        // still be inside a failed transaction rather than lend it out again.
        client.release(broken);
      }
    },
  };
}
