// @flow
//
// `@uniflowed/sql/postgres`: the `postgres` package (postgres.js).
//
//   import postgres from "postgres";
//   import { fromPostgres } from "@uniflowed/sql/postgres";
//
//   const db = fromPostgres(postgres(connectionString));
//
// Queries go through `sql.unsafe(text, params)` — the text is generated SQL
// with `$n` placeholders, not a tagged template — and `.raw()`, which returns
// each value as the bytes the server sent, so postgres.js's own parsers never
// run. Transactions are postgres.js's `begin` and `savepoint`.
//
// Its serialisers are kept out the same way. A prepared statement is
// described first, and postgres.js then serialises each parameter by the type
// the server inferred: `JSON.stringify` for `jsonb`, `Buffer.from(x)` for
// `bytea`, `x === true ? "t" : "f"` for `bool` — each of which would corrupt a
// value the codecs already wrote as text. Declaring every parameter as
// `unknown` (OID 705) leaves the server to infer the type, as an undeclared
// one would, and leaves postgres.js nothing to serialise. `:copyfrom` writes
// the text-format payload to the stream `unsafe(statement, []).writable()`
// opens for `COPY FROM STDIN`.

import { SqlError } from "../index.js";
import type { CopyIn, Queryable, SqlParam } from "../index.js";

type RawRows = Promise<$ReadOnlyArray<mixed> & { readonly count?: number | null, ... }>;

/** The Node writable postgres.js resolves `COPY FROM STDIN` to. */
type CopyWritable = {
  write(chunk: string, callback: (error?: mixed) => mixed): mixed,
  end(callback: (error?: mixed) => mixed): mixed,
  once(event: "error", listener: (error: mixed) => mixed): mixed,
  ...
};

type Unsafe = {
  raw(): RawRows,
  writable(): Promise<CopyWritable>,
  /** Rejects the query. postgres.js calls it for a server error during `COPY`. */
  reject: (error: mixed) => mixed,
  ...
};

/** What postgres.js's `Sql` and its transaction handles have in common. */
export interface PostgresQueries {
  unsafe(
    text    : string,
    params  : $ReadOnlyArray<mixed>,
    options?: {| readonly prepare?: boolean |},
  ): Unsafe;
  typed(value: SqlParam, oid: number): mixed;
}

/** A transaction handle, as `begin` and `savepoint` pass one. */
export interface PostgresTransaction extends PostgresQueries {
  savepoint<T>(body: (tx: PostgresTransaction) => Promise<T>): Promise<T>;
}

/** A postgres.js `Sql` instance. */
export interface PostgresSql extends PostgresQueries {
  begin<T>(body: (tx: PostgresTransaction) => Promise<T>): Promise<T>;
}

const decoder = new TextDecoder();

function text(value: mixed): mixed {
  return value instanceof Uint8Array ? decoder.decode(value) : value;
}

/** `unknown`: a parameter whose type the server decides. */
const UNKNOWN = 705;

function rowCount(payload: string): number {
  let rows = 0;
  for (let i = 0; i < payload.length; i += 1) {
    if (payload[i] === "\n") {
      rows += 1;
    }
  }
  return rows;
}

/** Write `payload` and wait until the server finishes the `COPY`. */
function writeCopy(writable: CopyWritable, payload: string): Promise<void> {
  return new Promise((resolve, reject) => {
    let settled = false;
    const fail = (error: mixed) => {
      if (settled) {
        return;
      }
      settled = true;
      reject(error instanceof Error ? error : new Error(String(error)));
    };
    const succeed = () => {
      if (settled) {
        return;
      }
      settled = true;
      resolve();
    };
    writable.once("error", fail);
    writable.write(payload, (error) => {
      if (error != null) {
        fail(error);
        return;
      }
      writable.end((endError) => {
        if (endError != null) {
          fail(endError);
        } else {
          succeed();
        }
      });
    });
  });
}

function copyOn(sql: PostgresQueries): CopyIn {
  return async (statement, payload) => {
    const query = sql.unsafe(statement, []);
    // CopyInResponse resolves the query with the writable, and sending
    // CopyDone clears that stream before the server answers. A failure after
    // the data is in (a foreign key) calls `query.reject` and never reaches
    // the writable, so its end callback would wait forever.
    let fail: (error: mixed) => void = () => {};
    const failed = new Promise((resolve, reject) => {
      void resolve;
      fail = reject;
    });
    failed.catch(() => {});
    const rejectQuery = query.reject.bind(query);
    query.reject = (error) => {
      fail(error);
      return rejectQuery(error);
    };
    const writable = await query.writable();
    await Promise.race([writeCopy(writable, payload), failed]);
    // CommandComplete carries `COPY n`, and postgres.js keeps that count on
    // its internal result. A `COPY` without `WHERE` accepts every row or
    // fails, so the number of lines written is the number the server copied.
    return rowCount(payload);
  };
}

function run(sql: PostgresQueries): Queryable["query"] {
  return async (statement, params) => {
    const typed = params.map((param) => (param === null ? null : sql.typed(param, UNKNOWN)));
    const result = await sql.unsafe(statement, typed, { prepare: true }).raw();
    const rows: Array<$ReadOnlyArray<mixed>> = [];
    for (const row of result) {
      if (Array.isArray(row)) {
        rows.push(row.map(text));
      }
    }
    return { rows, rowsAffected: result.count ?? 0, lastInsertId: null };
  };
}

/**
 * Run `body` on a `Queryable` over the transaction handle `tx`, which stops
 * working once `body` settles.
 *
 * postgres.js keeps answering on a handle after its `begin` or `savepoint`
 * has ended, and a statement run on it then lands outside the transaction.
 * The other adapters refuse such a statement (`transactionOn` does it for
 * them), so this one does too.
 */
async function scoped<T>(tx: PostgresTransaction, body: (tx: Queryable) => Promise<T>): Promise<T> {
  let open = true;
  const query = run(tx);
  const queryable: Queryable = {
    engine   : "postgresql",
    maxParams: 65535,
    query: (statement, params, mode) =>
      open ? query(statement, params, mode) : Promise.reject(new SqlError({ kind: "closed" })),
    copy: (statement, payload) =>
      open ? copyOn(tx)(statement, payload) : Promise.reject(new SqlError({ kind: "closed" })),
    transaction: <U>(inner: (tx: Queryable) => Promise<U>): Promise<U> =>
      open
        ? tx.savepoint((savepoint) => scoped(savepoint, inner))
        : Promise.reject(new SqlError({ kind: "closed" })),
  };
  try {
    return await body(queryable);
  } finally {
    open = false;
  }
}

/** A [`Queryable`] over a postgres.js `Sql`. */
export function fromPostgres(sql: PostgresSql): Queryable {
  return {
    engine   : "postgresql",
    maxParams: 65535,
    query    : run(sql),
    copy     : copyOn(sql),
    transaction: <T>(body: (tx: Queryable) => Promise<T>): Promise<T> =>
      sql.begin((tx) => scoped(tx, body)),
  };
}
