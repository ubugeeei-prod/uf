// @flow
//
// `@uniflowed/sql`: the runtime sqlc's Flow target generates code against.
//
// Not a query builder and not an ORM. A BFF that reads its data through SQL it
// wrote gets three things here, and nothing else:
//
//   * **One interface over every driver.** [`Queryable`] is the whole contract
//     generated code needs: run this text with these parameters, give the rows
//     back positionally. An adapter in `@uniflowed/sql/<driver>` makes a
//     driver's pool, client or database handle into one.
//   * **Exact values.** An adapter hands rows back in the wire's own
//     representation — PostgreSQL's text format, MySQL's text protocol,
//     SQLite's storage classes with safe integers — and the codecs in
//     `@uniflowed/sql/postgresql`, `/mysql` and `/sqlite` decode them by SQL
//     type. So an `int8` is a `bigint` under `pg` and under `postgres` alike,
//     rather than whatever each driver's own parser decided.
//   * **The parts of sqlc's semantics a driver does not have**: `sqlc.slice`
//     expansion, `:copyfrom` as chunked multi-row inserts, `:batch*` in one
//     transaction, and nested transactions as savepoints.
//
// docs/sqlc.md is the design record.

/** Which SQL dialect a [`Queryable`] speaks, which decides placeholder syntax. */
export type Engine = "postgresql" | "mysql" | "sqlite";

/**
 * A parameter as it goes to an adapter.
 *
 * Already encoded by a codec: PostgreSQL and MySQL parameters are text (or
 * bytes) and the server casts them to the type the statement needs, which is
 * the one representation every driver sends unchanged. SQLite parameters are
 * its storage classes.
 */
export type SqlParam = null | string | number | bigint | Uint8Array;

/** One row, positionally: a join may return two columns called `id`. */
export type Row = $ReadOnlyArray<mixed>;

/** What a statement produced. */
export type QueryResult = {|
  readonly rows: $ReadOnlyArray<Row>,
  /** Rows inserted, updated or deleted; 0 for a statement that changes none. */
  readonly rowsAffected: number,
  /** MySQL's `LAST_INSERT_ID()` and SQLite's `last_insert_rowid()`; `null` on PostgreSQL. */
  readonly lastInsertId: bigint | null,
|};

/**
 * Whether a statement is run for its rows or for its effect.
 *
 * SQLite drivers have two entry points (`all` and `run`) and only the second
 * reports `changes` and the last row id; the other engines ignore this.
 */
export type QueryMode = "rows" | "exec";

/**
 * A connection, a pool or an open transaction: anything generated code can
 * run a statement on.
 *
 * `transaction` is absent where the platform has no interactive transactions
 * (Cloudflare D1). Inside a transaction it opens a savepoint.
 */
export type Queryable = {
  readonly engine: Engine,
  /** The most bound parameters one statement may carry; `:copyfrom` chunks under it. */
  readonly maxParams: number,
  query(text: string, params: $ReadOnlyArray<SqlParam>, mode: QueryMode): Promise<QueryResult>,
  readonly transaction?: <T>(body: (tx: Queryable) => Promise<T>) => Promise<T>,
  ...
};

/** A JSON document, as `json` and `jsonb` columns decode to. */
export type JsonValue =
  | null
  | boolean
  | number
  | string
  | $ReadOnlyArray<JsonValue>
  | { readonly [string]: JsonValue };

/** What went wrong, as a value. */
export type SqlFailure =
  | {| readonly kind: "decode", readonly expected: string, readonly value: mixed |}
  | {| readonly kind: "encode", readonly expected: string, readonly value: mixed |}
  | {| readonly kind: "shape", readonly expected: number, readonly actual: number |}
  | {| readonly kind: "closed" |}
  | {| readonly kind: "unsupported", readonly feature: string |};

/**
 * A value that did not fit its column, a row of the wrong width, or a
 * transaction used after it ended.
 *
 * Never a database error: those are the driver's, and are rethrown untouched
 * so that a caller matching on `code: "23505"` still can.
 */
export class SqlError extends Error {
  readonly failure: SqlFailure;
  /** The generated query that was running, when there was one. */
  query: string | null;

  constructor(failure: SqlFailure, query: string | null = null) {
    super(explain(failure, query));
    this.name = "SqlError";
    this.failure = failure;
    this.query = query;
  }
}

function show(value: mixed): string {
  if (typeof value === "string") {
    return JSON.stringify(value.length > 64 ? `${value.slice(0, 64)}…` : value);
  }
  if (typeof value === "bigint") {
    return `${value}n`;
  }
  if (value instanceof Uint8Array) {
    return `<${value.length} bytes>`;
  }
  return String(value);
}

function explain(failure: SqlFailure, query: string | null): string {
  const where = query === null ? "" : `${query}: `;
  return (
    where +
    match (failure) {
      {kind: "decode", expected: const expected, value: const value} =>
        `expected ${expected} from the database, got ${show(value)}`,
      {kind: "encode", expected: const expected, value: const value} =>
        `expected ${expected} as a parameter, got ${show(value)}`,
      {kind: "shape", expected: const expected, actual: const actual} =>
        `the adapter returned ${actual} columns where the query has ${expected}; ` +
          "regenerate with `uf sqlc generate`, or check that the adapter returns rows as arrays",
      {kind: "closed"} => "the transaction has already committed or rolled back",
      {kind: "unsupported", feature: const feature} =>
        `${feature} is not supported by this adapter`,
    }
  );
}

/** Throw a decode failure. For codecs. */
export function decodeError(expected: string, value: mixed): empty {
  throw new SqlError({ kind: "decode", expected, value });
}

/** Throw an encode failure. For codecs. */
export function encodeError(expected: string, value: mixed): empty {
  throw new SqlError({ kind: "encode", expected, value });
}

function attribute(error: mixed, name: string): mixed {
  if (error instanceof SqlError && error.query === null) {
    error.query = name;
    error.message = explain(error.failure, name);
  }
  return error;
}

function checkWidth(row: Row, width: number): void {
  if (row.length !== width) {
    throw new SqlError({ kind: "shape", expected: width, actual: row.length });
  }
}

// ---------------------------------------------------------------------------
// Running a generated query.
//
// `name` is the query's name in the SQL file, for error messages; `width` is
// how many columns the generator saw, which a stale schema or an adapter that
// returns objects instead of arrays would get wrong.

/** `:one` — the first row, or `null`. */
export async function one<T>(
  db: Queryable,
  name: string,
  text: string,
  params: $ReadOnlyArray<SqlParam>,
  width: number,
  decode: (row: Row) => T,
): Promise<T | null> {
  try {
    const { rows } = await db.query(text, params, "rows");
    if (rows.length === 0) {
      return null;
    }
    const row = rows[0];
    checkWidth(row, width);
    return decode(row);
  } catch (error) {
    throw attribute(error, name);
  }
}

/** `:many` — every row. */
export async function many<T>(
  db: Queryable,
  name: string,
  text: string,
  params: $ReadOnlyArray<SqlParam>,
  width: number,
  decode: (row: Row) => T,
): Promise<Array<T>> {
  try {
    const { rows } = await db.query(text, params, "rows");
    const out: Array<T> = new Array(rows.length);
    for (let i = 0; i < rows.length; i += 1) {
      const row = rows[i];
      checkWidth(row, width);
      out[i] = decode(row);
    }
    return out;
  } catch (error) {
    throw attribute(error, name);
  }
}

async function run(
  db: Queryable,
  name: string,
  text: string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<QueryResult> {
  try {
    return await db.query(text, params, "exec");
  } catch (error) {
    throw attribute(error, name);
  }
}

/** `:exec`. */
export async function exec(
  db: Queryable,
  name: string,
  text: string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<void> {
  await run(db, name, text, params);
}

/** `:execrows` — how many rows the statement changed. */
export async function execRows(
  db: Queryable,
  name: string,
  text: string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<number> {
  return (await run(db, name, text, params)).rowsAffected;
}

/** What `:execresult` resolves to. */
export type ExecResult = {|
  readonly rowsAffected: number,
  readonly lastInsertId: bigint | null,
|};

/** `:execresult` — the affected-row count and, on MySQL and SQLite, the last id. */
export async function execResult(
  db: Queryable,
  name: string,
  text: string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<ExecResult> {
  const { rowsAffected, lastInsertId } = await run(db, name, text, params);
  return { rowsAffected, lastInsertId };
}

/**
 * `:execlastid` — the id the statement inserted.
 *
 * PostgreSQL has no such thing; sqlc's answer there is `RETURNING id` with
 * `:one`, and this throws rather than resolve to a made-up zero.
 */
export async function execLastId(
  db: Queryable,
  name: string,
  text: string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<bigint> {
  const { lastInsertId } = await run(db, name, text, params);
  if (lastInsertId === null) {
    throw new SqlError({ kind: "unsupported", feature: ":execlastid on PostgreSQL" }, name);
  }
  return lastInsertId;
}

// ---------------------------------------------------------------------------
// `sqlc.slice` on MySQL and SQLite.
//
// PostgreSQL passes a slice as one array parameter. The other two have no
// arrays, so sqlc writes `IN (/*SLICE:ids*/?)` and leaves the expansion to the
// generated code. The generator splits the text at those markers, so that
// nothing here has to find a `?` inside a string literal at run time.

/** A `sqlc.slice` argument, already encoded element by element. */
export class Slice {
  readonly values: $ReadOnlyArray<SqlParam>;

  constructor(values: $ReadOnlyArray<SqlParam>) {
    this.values = values;
  }
}

/** Wrap an encoded list as one `sqlc.slice` argument. */
export function slice(values: $ReadOnlyArray<SqlParam>): Slice {
  return new Slice(values);
}

/**
 * Join `parts` around the slices in `values`, in order.
 *
 * `values` are the statement's parameters in placeholder order; each [`Slice`]
 * sits where a marker was, between `parts[k]` and `parts[k + 1]`. A non-empty
 * slice becomes `?, ?, …`; an empty one becomes `NULL`, so `x IN (NULL)` is
 * false for every row — sqlc's own semantics, and what a list of nothing
 * should match.
 */
export function expand(
  parts: $ReadOnlyArray<string>,
  values: $ReadOnlyArray<SqlParam | Slice>,
): {| readonly text: string, readonly params: $ReadOnlyArray<SqlParam> |} {
  const params: Array<SqlParam> = [];
  let text = parts[0];
  let next = 1;
  for (const value of values) {
    if (value instanceof Slice) {
      text += value.values.length === 0 ? "NULL" : "?, ".repeat(value.values.length - 1) + "?";
      text += parts[next];
      next += 1;
      for (const item of value.values) {
        params.push(item);
      }
    } else {
      params.push(value);
    }
  }
  return { text, params };
}

// ---------------------------------------------------------------------------
// `:copyfrom`.

/**
 * An `INSERT … VALUES (…)` statement split so it can be repeated per row.
 *
 * `tuple` is the parenthesised group around its placeholders and `refs` says
 * which field of a row each placeholder takes, so `(owner_id, name)` with
 * `VALUES ($1, $2)` is `tuple: ["(", ", ", ")"]`, `refs: [0, 1]`.
 */
export type CopyPlan = {|
  readonly head: string,
  readonly tuple: $ReadOnlyArray<string>,
  readonly refs: $ReadOnlyArray<number>,
  readonly tail: string,
|};

/** The most rows one statement carries when a row has no parameters at all. */
const COPY_ROWS_WITHOUT_PARAMS = 1000;

/**
 * `:copyfrom` — insert every row, as few statements as `maxParams` allows.
 *
 * Inside one transaction when the `Queryable` can open one, so a failure in
 * the third chunk does not leave the first two behind: pgx's `CopyFrom` is one
 * `COPY`, and a caller of the generated function should not have to know it is
 * now several statements. Resolves to the number of rows inserted.
 */
export async function copyFrom(
  db: Queryable,
  name: string,
  plan: CopyPlan,
  rows: $ReadOnlyArray<$ReadOnlyArray<SqlParam>>,
): Promise<number> {
  if (rows.length === 0) {
    return 0;
  }
  const width = plan.refs.length;
  const perStatement =
    width === 0 ? COPY_ROWS_WITHOUT_PARAMS : Math.max(1, Math.floor(db.maxParams / width));
  const insert = async (q: Queryable): Promise<number> => {
    let total = 0;
    for (let start = 0; start < rows.length; start += perStatement) {
      const chunk = rows.slice(start, start + perStatement);
      const params: Array<SqlParam> = [];
      const tuples: Array<string> = [];
      for (const row of chunk) {
        let tuple = plan.tuple[0];
        for (let i = 0; i < width; i += 1) {
          params.push(row[plan.refs[i]]);
          tuple += (q.engine === "postgresql" ? `$${params.length}` : "?") + plan.tuple[i + 1];
        }
        tuples.push(tuple);
      }
      const text = plan.head + tuples.join(", ") + plan.tail;
      total += (await run(q, name, text, params)).rowsAffected;
    }
    return total;
  };
  const transaction = db.transaction;
  if (transaction === undefined || rows.length <= perStatement) {
    return insert(db);
  }
  return transaction(insert);
}

// ---------------------------------------------------------------------------
// `:batchexec`, `:batchmany`, `:batchone`.

/**
 * Run `body` once per item, in order, inside one transaction when there is
 * one to open.
 *
 * pgx sends a batch as one implicit transaction; this keeps the atomicity and
 * the order without pgx. Where `transaction` is absent (D1) the items run in
 * order without one, and the docs say so.
 */
export async function batch<A, R>(
  db: Queryable,
  items: $ReadOnlyArray<A>,
  body: (q: Queryable, item: A) => Promise<R>,
): Promise<Array<R>> {
  const each = async (q: Queryable): Promise<Array<R>> => {
    const out: Array<R> = [];
    for (const item of items) {
      out.push(await body(q, item));
    }
    return out;
  };
  const transaction = db.transaction;
  if (transaction === undefined || items.length <= 1) {
    return each(db);
  }
  return transaction(each);
}

// ---------------------------------------------------------------------------
// Transactions, for adapters.

/** How an adapter runs one statement on one connection. */
export type Run = (
  text: string,
  params: $ReadOnlyArray<SqlParam>,
  mode: QueryMode,
) => Promise<QueryResult>;

/** What [`transactionOn`] needs from an adapter. */
export type Connection = {|
  readonly engine: Engine,
  readonly maxParams: number,
  readonly run: Run,
  /** The statement that opens a transaction; `BEGIN` unless the adapter knows better. */
  readonly begin?: string,
|};

/**
 * Run `body` in a transaction on `connection`, which must be a single
 * connection nobody else is using until this settles.
 *
 * Commits when `body` resolves and rolls back when it throws, rethrowing what
 * it threw. The [`Queryable`] `body` receives stops working once this
 * settles — a statement run on it afterwards would otherwise land outside the
 * transaction without anyone noticing. Its own `transaction` opens a
 * savepoint.
 */
export async function transactionOn<T>(
  connection: Connection,
  body: (tx: Queryable) => Promise<T>,
): Promise<T> {
  await connection.run(connection.begin ?? "BEGIN", [], "exec");
  return scoped(connection, 0, body, "COMMIT", "ROLLBACK");
}

async function scoped<T>(
  connection: Connection,
  depth: number,
  body: (tx: Queryable) => Promise<T>,
  commit: string,
  rollback: string,
): Promise<T> {
  let open = true;
  const guarded: Run = (text, params, mode) => {
    if (!open) {
      return Promise.reject(new SqlError({ kind: "closed" }));
    }
    return connection.run(text, params, mode);
  };
  const tx: Queryable = {
    engine: connection.engine,
    maxParams: connection.maxParams,
    query: guarded,
    transaction: async <U>(inner: (tx: Queryable) => Promise<U>): Promise<U> => {
      const savepoint = `uf_sp_${depth + 1}`;
      await guarded(`SAVEPOINT ${savepoint}`, [], "exec");
      return scoped(
        { ...connection, run: guarded },
        depth + 1,
        inner,
        `RELEASE SAVEPOINT ${savepoint}`,
        `ROLLBACK TO SAVEPOINT ${savepoint}`,
      );
    },
  };
  let value: T;
  try {
    value = await body(tx);
  } catch (error) {
    open = false;
    try {
      await connection.run(rollback, [], "exec");
      if (depth > 0) {
        // `ROLLBACK TO` leaves the savepoint in place; release it so the
        // enclosing transaction can go on as if the inner one never started.
        await connection.run(commit, [], "exec");
      }
    } catch {
      // The rollback failed too — usually because the connection is gone,
      // which rolls back on its own. The error worth reporting is the first.
    }
    throw error;
  }
  open = false;
  await connection.run(commit, [], "exec");
  return value;
}

/**
 * A [`Queryable`] over one connection that serves a whole application: a
 * SQLite database, or an embedded PostgreSQL.
 *
 * While a transaction is open every statement run on the returned value waits
 * for it to finish, so a request that is not in the transaction never lands
 * inside it. Statements run on the transaction's own [`Queryable`] do not wait.
 */
export function singleConnection(connection: Connection): Queryable {
  let gate: Promise<void> | null = null;
  const query = async (
    text: string,
    params: $ReadOnlyArray<SqlParam>,
    mode: QueryMode,
  ): Promise<QueryResult> => {
    while (gate !== null) {
      await gate;
    }
    return connection.run(text, params, mode);
  };
  const transaction = async <T>(body: (tx: Queryable) => Promise<T>): Promise<T> => {
    while (gate !== null) {
      await gate;
    }
    let release: () => void = () => {};
    gate = new Promise((resolve) => {
      release = resolve;
    });
    try {
      return await transactionOn(connection, body);
    } finally {
      gate = null;
      release();
    }
  };
  return { engine: connection.engine, maxParams: connection.maxParams, query, transaction };
}
