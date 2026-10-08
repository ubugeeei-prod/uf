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
// `:copyfrom` is a `COPY FROM STDIN`: node-postgres runs it when the query
// object supplies `submit` and `handleCopyInResponse`, which is the protocol
// hook it already has and does not implement for a string query.

import type { Connection, CopyIn, QueryResult, Queryable, Run } from "../index.js";
import { singleConnection, transactionOn } from "../index.js";

type PgResult = { readonly rows: $ReadOnlyArray<mixed>, readonly rowCount: number | null, ... };

type PgQuery = {|
  readonly text   : string,
  readonly values : $ReadOnlyArray<mixed>,
  readonly rowMode: "array",
  readonly types  : {| readonly getTypeParser: () => (value: string) => string |},
|};

/** The connection methods a `COPY FROM STDIN` uses. `pg` passes its own. */
type PgCopyConnection = {
  query(text: string): mixed,
  sendCopyFromChunk(chunk: Uint8Array): mixed,
  endCopyFrom(): mixed,
};

/** A query object `pg` runs when `submit` is a function. */
type PgCopyRequest = {
  text    : string,
  callback: ?(?mixed, ?PgResult) => mixed,
  submit(connection: PgCopyConnection): null,
  handleCopyInResponse(connection: PgCopyConnection): void,
  handleCommandComplete(message: { text?: string, ... }): void,
  handleReadyForQuery(): void,
  handleError(error: mixed): void,
  handleCopyData(): void,
  handleRowDescription(): void,
  handleDataRow(): void,
  handleEmptyQuery(): void,
  handlePortalSuspended(): void,
};

/** A connected `pg.Client` or a client checked out of a pool. */
export interface PgClient {
  query(config: PgQuery | PgCopyRequest): Promise<PgResult> | void;
}

/** A client checked out of a `pg.Pool`. */
export interface PgPoolClient extends PgClient {
  release(error?: mixed): void;
}

/** A `pg.Pool`. */
export interface PgPool {
  query(config: PgQuery | PgCopyRequest): Promise<PgResult>;
  connect(): Promise<PgPoolClient>;
}

const COPY_ROWS = /COPY (\d+)/;

/**
 * One `COPY FROM STDIN` on `runner`.
 *
 * The callback is left unset when `runner` is a pool: the pool sets it, and
 * setting it here would skip the pool's own, which is what releases the
 * client. A client sets it in the wrapper below, because `client.query` only
 * returns a promise when the object does not already have one and nobody
 * passed a callback.
 */
function copyRequest(statement: string, payload: string): PgCopyRequest {
  let rowCount = 0;
  let settled = false;
  const request: PgCopyRequest = {
    text    : statement,
    callback: undefined,
    submit(connection) {
      connection.query(statement);
      return null;
    },
    handleCopyInResponse(connection) {
      // pg-protocol copies the chunk with `Buffer#copy`. A `Uint8Array` has
      // no `copy`, and the throw leaves the backend waiting for `CopyData`.
      connection.sendCopyFromChunk(Buffer.from(payload, "utf8"));
      connection.endCopyFrom();
    },
    handleCommandComplete(message) {
      const text = typeof message.text === "string" ? message.text : "";
      const match = COPY_ROWS.exec(text);
      if (match?.[1] !== undefined) {
        rowCount = Number(match[1]);
      }
    },
    handleReadyForQuery() {
      if (settled) {
        return;
      }
      settled = true;
      request.callback?.(null, { rows: [], rowCount });
    },
    handleError(error) {
      if (settled) {
        return;
      }
      settled = true;
      request.callback?.(error, undefined);
    },
    handleCopyData() {},
    handleRowDescription() {},
    handleDataRow() {},
    handleEmptyQuery() {},
    handlePortalSuspended() {},
  };
  return request;
}

function copyOn(runner: PgClient): CopyIn {
  return (statement, payload) =>
    new Promise((resolve, reject) => {
      const request = copyRequest(statement, payload);
      request.callback = (error, result) => {
        if (error != null) {
          reject(error instanceof Error ? error : new Error(String(error)));
          return;
        }
        resolve(result?.rowCount ?? 0);
      };
      runner.query(request);
    });
}

function copyOnPool(pool: PgPool): CopyIn {
  return async (statement, payload) => {
    const result = await pool.query(copyRequest(statement, payload));
    return result.rowCount ?? 0;
  };
}

const RAW = { getTypeParser: () => (value: string) => value };

function runOn(client: PgClient): Run {
  return async (text, params): Promise<QueryResult> => {
    const result = await client.query({ text, values: params, rowMode: "array", types: RAW });
    // A `COPY` request returns void. This call is a normal query, which
    // returns a result; the union is what `query` is declared as.
    if (result == null) {
      throw new Error("pg query returned no result");
    }
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
  return { engine: "postgresql", maxParams: 65535, run: runOn(client), copy: copyOn(client) };
}

/** A [`Queryable`] over one connected `pg.Client`, which it serialises. */
export function fromPgClient(client: PgClient): Queryable {
  return singleConnection(connection(client));
}

/** A [`Queryable`] over a `pg.Pool`. */
export function fromPgPool(pool: PgPool): Queryable {
  return {
    engine   : "postgresql",
    maxParams: 65535,
    query    : runOn(pool),
    copy     : copyOnPool(pool),
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
