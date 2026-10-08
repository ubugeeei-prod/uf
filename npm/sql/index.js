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
//     expansion, `:copyfrom` (PostgreSQL `COPY` when the adapter can send it,
//     otherwise chunked multi-row inserts), `:batch*` in one transaction, and
//     nested transactions as savepoints.
//   * **A synchronous path** for drivers that run a statement on the calling
//     thread (`node:sqlite`, `better-sqlite3`, `bun:sqlite`). Generated code
//     with `sync: true` calls it, so a transaction can stay inside `BEGIN`
//     through `COMMIT` without yielding.
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
/**
 * Send one `COPY … FROM STDIN` payload and resolve to the number of rows the
 * server accepted.
 *
 * Present on a PostgreSQL adapter that can speak `COPY`. Absent everywhere
 * else, where `:copyfrom` stays a chunked `INSERT`. The payload is PostgreSQL
 * text format: tab-separated fields, one row per line.
 */
export type CopyIn = (statement: string, payload: string) => Promise<number>;

export type Queryable = {
  readonly engine: Engine,
  /** The most bound parameters one statement may carry; `:copyfrom` chunks under it. */
  readonly maxParams: number,
  query(text: string, params: $ReadOnlyArray<SqlParam>, mode: QueryMode): Promise<QueryResult>,
  readonly transaction?: <T>(body: (tx: Queryable) => Promise<T>) => Promise<T>,
  /** PostgreSQL `COPY`, when this adapter can send one. */
  readonly copy?: CopyIn,
  ...
};

/**
 * A connection whose statements run on the calling thread and return before
 * the call does.
 *
 * This is the contract `sync: true` generated code needs. `node:sqlite`,
 * `better-sqlite3` and `bun:sqlite` implement it on the same object as
 * [`Queryable`]. Drivers that are asynchronous all the way down (D1, `pg`,
 * `postgres`, `mysql2`) do not have it, and a synchronous function does not
 * typecheck against them.
 *
 * `transactionSync` does not yield between `BEGIN` and `COMMIT`. Nested
 * `transactionSync` opens a savepoint. Calling the outer handle while a
 * transaction is open throws: an asynchronous transaction cannot be waited
 * out on this thread, and reaching back to the outer handle from inside the
 * body would deadlock the same way `await db.query` does inside `transaction`.
 */
export type SyncQueryable = {
  readonly engine   : Engine,
  readonly maxParams: number,
  querySync(text: string, params: $ReadOnlyArray<SqlParam>, mode: QueryMode): QueryResult,
  readonly transactionSync?: <T>(body: (tx: SyncQueryable) => T) => T,
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
  | {| readonly kind: "unsupported", readonly feature: string |}
  | {| readonly kind: "params", readonly width: number, readonly max: number |}
  | {| readonly kind: "row", readonly index: number, readonly length: number |};

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
      {kind: "decode", expected: const expected, value: const value}  =>
        `expected ${expected} from the database, got ${show(value)}`,
      {kind: "encode", expected: const expected, value: const value}  =>
        `expected ${expected} as a parameter, got ${show(value)}`,
      {kind: "shape", expected: const expected, actual: const actual} =>
        `the adapter returned ${actual} columns where the query has ${expected}; ` +
          "regenerate with `uf sqlc generate`, or check that the adapter returns rows as arrays",
      {kind: "closed"}                                                => "the transaction has already committed or rolled back",
      {kind: "unsupported", feature: const feature}                   =>
        `${feature} is not supported by this adapter`,
      {kind: "params", width: const width, max: const max}            =>
        `a :copyfrom row binds ${width} parameters and this adapter allows ${max}`,
      {kind: "row", index: const index, length: const length}         =>
        `a :copyfrom row has ${length} fields and the statement reads index ${index}`,
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

function unsupported(feature: string): empty {
  throw new SqlError({ kind: "unsupported", feature });
}

function firstRow<T>(rows: $ReadOnlyArray<Row>, width: number, decode: (row: Row) => T): T | null {
  if (rows.length === 0) {
    return null;
  }
  const row = rows[0];
  checkWidth(row, width);
  return decode(row);
}

function allRows<T>(rows: $ReadOnlyArray<Row>, width: number, decode: (row: Row) => T): Array<T> {
  const out: Array<T> = new Array(rows.length);
  for (let i = 0; i < rows.length; i += 1) {
    const row = rows[i];
    checkWidth(row, width);
    out[i] = decode(row);
  }
  return out;
}

// ---------------------------------------------------------------------------
// Running a generated query.
//
// `name` is the query's name in the SQL file, for error messages; `width` is
// how many columns the generator saw, which a stale schema or an adapter that
// returns objects instead of arrays would get wrong.

/** `:one` — the first row, or `null`. */
export async function one<T>(
  db    : Queryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
  width : number,
  decode: (row: Row) => T,
): Promise<T | null> {
  try {
    const { rows } = await db.query(text, params, "rows");
    return firstRow(rows, width, decode);
  } catch (error) {
    throw attribute(error, name);
  }
}

/** `:one` on a [`SyncQueryable`]. */
export function oneSync<T>(
  db    : SyncQueryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
  width : number,
  decode: (row: Row) => T,
): T | null {
  try {
    const { rows } = db.querySync(text, params, "rows");
    return firstRow(rows, width, decode);
  } catch (error) {
    throw attribute(error, name);
  }
}

/** `:many` — every row. */
export async function many<T>(
  db    : Queryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
  width : number,
  decode: (row: Row) => T,
): Promise<Array<T>> {
  try {
    const { rows } = await db.query(text, params, "rows");
    return allRows(rows, width, decode);
  } catch (error) {
    throw attribute(error, name);
  }
}

/** `:many` on a [`SyncQueryable`]. */
export function manySync<T>(
  db    : SyncQueryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
  width : number,
  decode: (row: Row) => T,
): Array<T> {
  try {
    const { rows } = db.querySync(text, params, "rows");
    return allRows(rows, width, decode);
  } catch (error) {
    throw attribute(error, name);
  }
}

async function run(
  db    : Queryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<QueryResult> {
  try {
    return await db.query(text, params, "exec");
  } catch (error) {
    throw attribute(error, name);
  }
}

function executed(
  db    : SyncQueryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): QueryResult {
  try {
    return db.querySync(text, params, "exec");
  } catch (error) {
    throw attribute(error, name);
  }
}

/** `:exec`. */
export async function exec(
  db    : Queryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<void> {
  await run(db, name, text, params);
}

/** `:exec` on a [`SyncQueryable`]. */
export function execSync(
  db    : SyncQueryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): void {
  executed(db, name, text, params);
}

/** `:execrows` — how many rows the statement changed. */
export async function execRows(
  db    : Queryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<number> {
  return (await run(db, name, text, params)).rowsAffected;
}

/** `:execrows` on a [`SyncQueryable`]. */
export function execRowsSync(
  db    : SyncQueryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): number {
  return executed(db, name, text, params).rowsAffected;
}

/** What `:execresult` resolves to. */
export type ExecResult = {|
  readonly rowsAffected: number,
  readonly lastInsertId: bigint | null,
|};

/** `:execresult` — the affected-row count and, on MySQL and SQLite, the last id. */
export async function execResult(
  db    : Queryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<ExecResult> {
  const { rowsAffected, lastInsertId } = await run(db, name, text, params);
  return { rowsAffected, lastInsertId };
}

/** `:execresult` on a [`SyncQueryable`]. */
export function execResultSync(
  db    : SyncQueryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): ExecResult {
  const { rowsAffected, lastInsertId } = executed(db, name, text, params);
  return { rowsAffected, lastInsertId };
}

function insertedId(result: QueryResult, name: string): bigint {
  if (result.lastInsertId === null) {
    throw new SqlError({ kind: "unsupported", feature: ":execlastid on PostgreSQL" }, name);
  }
  return result.lastInsertId;
}

/**
 * `:execlastid` — the id the statement inserted.
 *
 * PostgreSQL has no such thing; sqlc's answer there is `RETURNING id` with
 * `:one`, and this throws rather than resolve to a made-up zero.
 */
export async function execLastId(
  db    : Queryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): Promise<bigint> {
  return insertedId(await run(db, name, text, params), name);
}

/** `:execlastid` on a [`SyncQueryable`]. */
export function execLastIdSync(
  db    : SyncQueryable,
  name  : string,
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
): bigint {
  return insertedId(executed(db, name, text, params), name);
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
  parts : $ReadOnlyArray<string>,
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
  readonly head : string,
  readonly tuple: $ReadOnlyArray<string>,
  readonly refs : $ReadOnlyArray<number>,
  readonly tail : string,
|};

/** The most rows one statement carries when a row has no parameters at all. */
const COPY_ROWS_WITHOUT_PARAMS = 1000;

/**
 * `COPY <table> (<columns>) FROM STDIN` for an `INSERT … VALUES` plan, or
 * `null` when the plan is not that shape.
 *
 * sqlc only accepts `INSERT INTO t (…) VALUES (…)` for `:copyfrom`, and the
 * generator puts the table and the column list in `head`. Anything else keeps
 * the chunked `INSERT`, which is the path every engine can run.
 */
function copyStatement(plan: CopyPlan): string | null {
  const prefix = "INSERT INTO ";
  const suffix = " VALUES ";
  if (!plan.head.startsWith(prefix) || !plan.head.endsWith(suffix)) {
    return null;
  }
  const target = plan.head.slice(prefix.length, plan.head.length - suffix.length);
  return `COPY ${target} FROM STDIN WITH (FORMAT text)`;
}

/** One field of PostgreSQL's text `COPY` format. */
function copyField(value: SqlParam): string {
  if (value === null) {
    return "\\N";
  }
  return escapeCopyText(copyText(value));
}

function copyText(value: string | number | bigint | Uint8Array): string {
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number" || typeof value === "bigint") {
    return String(value);
  }
  let hex = "\\x";
  for (let i = 0; i < value.length; i += 1) {
    hex += value[i].toString(16).padStart(2, "0");
  }
  return hex;
}

/** `\`, newline, carriage return and tab, the four escapes text `COPY` requires. */
function escapeCopyText(text: string): string {
  let out = "";
  for (let i = 0; i < text.length; i += 1) {
    const character = text[i];
    if (character === "\\") {
      out += "\\\\";
    } else if (character === "\n") {
      out += "\\n";
    } else if (character === "\r") {
      out += "\\r";
    } else if (character === "\t") {
      out += "\\t";
    } else {
      out += character;
    }
  }
  return out;
}

/**
 * Every row of a `:copyfrom` as one text-format payload.
 *
 * A short row throws before anything is sent, the same way the `INSERT` path
 * does. A null field is `\N`. The payload ends each row with a newline, which
 * is the row terminator `COPY` counts.
 */
function copyPayload(
  plan: CopyPlan,
  rows: $ReadOnlyArray<$ReadOnlyArray<SqlParam>>,
  name: string,
): string {
  const width = plan.refs.length;
  let payload = "";
  for (const row of rows) {
    for (let i = 0; i < width; i += 1) {
      const index = plan.refs[i];
      if (index >= row.length) {
        throw new SqlError({ kind: "row", index, length: row.length }, name);
      }
      if (i > 0) {
        payload += "\t";
      }
      payload += copyField(row[index]);
    }
    payload += "\n";
  }
  return payload;
}

function copyChunk(
  engine: Engine,
  plan  : CopyPlan,
  chunk : $ReadOnlyArray<$ReadOnlyArray<SqlParam>>,
  name  : string,
): {| readonly text: string, readonly params: $ReadOnlyArray<SqlParam> |} {
  const width = plan.refs.length;
  const params: Array<SqlParam> = [];
  const tuples: Array<string> = [];
  for (const row of chunk) {
    let tuple = plan.tuple[0];
    for (let i = 0; i < width; i += 1) {
      const index = plan.refs[i];
      if (index >= row.length) {
        throw new SqlError({ kind: "row", index, length: row.length }, name);
      }
      params.push(row[index]);
      tuple += (engine === "postgresql" ? `$${params.length}` : "?") + plan.tuple[i + 1];
    }
    tuples.push(tuple);
  }
  return { text: plan.head + tuples.join(", ") + plan.tail, params };
}

function copyWidth(db: { readonly maxParams: number, ... }, plan: CopyPlan, name: string): number {
  const width = plan.refs.length;
  // One row is the smallest statement. Wider than `maxParams`, it cannot be sent.
  if (width > db.maxParams) {
    throw new SqlError({ kind: "params", width, max: db.maxParams }, name);
  }
  return width === 0 ? COPY_ROWS_WITHOUT_PARAMS : Math.max(1, Math.floor(db.maxParams / width));
}

/**
 * `:copyfrom` — insert every row.
 *
 * A PostgreSQL adapter that implements `copy` receives one `COPY … FROM STDIN`
 * in text format, which is one statement and so one success or one failure.
 * Every other adapter, and a plan that is not an `INSERT … VALUES`, stays on
 * chunked multi-row `INSERT`s. Those run inside one transaction when the
 * `Queryable` can open one and the rows do not fit in a single statement, so
 * a failure in the third chunk does not leave the first two behind. Resolves
 * to the number of rows inserted.
 */
export async function copyFrom(
  db  : Queryable,
  name: string,
  plan: CopyPlan,
  rows: $ReadOnlyArray<$ReadOnlyArray<SqlParam>>,
): Promise<number> {
  if (rows.length === 0) {
    return 0;
  }
  const copy = db.copy;
  if (db.engine === "postgresql" && typeof copy === "function") {
    const statement = copyStatement(plan);
    if (statement !== null) {
      return copy(statement, copyPayload(plan, rows, name));
    }
  }
  const perStatement = copyWidth(db, plan, name);
  const insert = async (q: Queryable): Promise<number> => {
    let total = 0;
    for (let start = 0; start < rows.length; start += perStatement) {
      const statement = copyChunk(q.engine, plan, rows.slice(start, start + perStatement), name);
      total += (await run(q, name, statement.text, statement.params)).rowsAffected;
    }
    return total;
  };
  const transaction = db.transaction;
  if (transaction === undefined || rows.length <= perStatement) {
    return insert(db);
  }
  return transaction(insert);
}

/** `:copyfrom` on a [`SyncQueryable`]. */
export function copyFromSync(
  db  : SyncQueryable,
  name: string,
  plan: CopyPlan,
  rows: $ReadOnlyArray<$ReadOnlyArray<SqlParam>>,
): number {
  if (rows.length === 0) {
    return 0;
  }
  const perStatement = copyWidth(db, plan, name);
  const insert = (q: SyncQueryable): number => {
    let total = 0;
    for (let start = 0; start < rows.length; start += perStatement) {
      const statement = copyChunk(q.engine, plan, rows.slice(start, start + perStatement), name);
      total += executed(q, name, statement.text, statement.params).rowsAffected;
    }
    return total;
  };
  const transaction = db.transactionSync;
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
  db   : Queryable,
  items: $ReadOnlyArray<A>,
  body : (q: Queryable, item: A) => Promise<R>,
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

/**
 * [`batch`] on a [`SyncQueryable`].
 *
 * The body returns its value directly. Where `transactionSync` is absent the
 * items still run in order, on this thread.
 */
export function batchSync<A, R>(
  db   : SyncQueryable,
  items: $ReadOnlyArray<A>,
  body : (q: SyncQueryable, item: A) => R,
): Array<R> {
  const each = (q: SyncQueryable): Array<R> => {
    const out: Array<R> = [];
    for (const item of items) {
      out.push(body(q, item));
    }
    return out;
  };
  const transaction = db.transactionSync;
  if (transaction === undefined || items.length <= 1) {
    return each(db);
  }
  return transaction(each);
}

// ---------------------------------------------------------------------------
// Transactions, for adapters.

/** How an adapter runs one statement on one connection. */
export type Run = (
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
  mode  : QueryMode,
) => Promise<QueryResult>;

/** [`Run`] for a driver that finishes the statement before returning. */
export type RunSync = (
  text  : string,
  params: $ReadOnlyArray<SqlParam>,
  mode  : QueryMode,
) => QueryResult;

/** What [`transactionOn`] needs from an adapter. */
export type Connection = {|
  readonly engine   : Engine,
  readonly maxParams: number,
  readonly run      : Run,
  /** Present when the driver can also run a statement on the calling thread. */
  readonly runSync?: RunSync,
  /** The statement that opens a transaction; `BEGIN` unless the adapter knows better. */
  readonly begin?: string,
  /** PostgreSQL `COPY` on this connection, when the driver can send one. */
  readonly copy?: CopyIn,
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
  body      : (tx: Queryable) => Promise<T>,
): Promise<T> {
  await connection.run(connection.begin ?? "BEGIN", [], "exec");
  return scoped(connection, 0, body, "COMMIT", "ROLLBACK");
}

async function scoped<T>(
  connection: Connection,
  depth     : number,
  body      : (tx: Queryable) => Promise<T>,
  commit    : string,
  rollback  : string,
): Promise<T> {
  let open = true;
  const guarded: Run = (text, params, mode) => {
    if (!open) {
      return Promise.reject(new SqlError({ kind: "closed" }));
    }
    return connection.run(text, params, mode);
  };
  const copy = connection.copy;
  const guardedCopy: CopyIn | void =
    copy === undefined
      ? undefined
      : (statement, payload) =>
          open ? copy(statement, payload) : Promise.reject(new SqlError({ kind: "closed" }));
  const tx: Queryable = {
    engine   : connection.engine,
    maxParams: connection.maxParams,
    query    : guarded,
    ...(guardedCopy === undefined ? null : { copy: guardedCopy }),
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
  // A failed COMMIT leaves the transaction open. Roll it back before the
  // error leaves, or the next statement runs inside a transaction this
  // handle has already forgotten. A nested RELEASE fails the same way;
  // `ROLLBACK TO` then `RELEASE` is the body-error path.
  open = false;
  try {
    await connection.run(commit, [], "exec");
  } catch (error) {
    try {
      await connection.run(rollback, [], "exec");
      if (depth > 0) {
        await connection.run(commit, [], "exec");
      }
    } catch {
      // The rollback failed too — usually because the connection is gone.
      // The error worth reporting is the commit's.
    }
    throw error;
  }
  return value;
}

/**
 * [`transactionOn`] for a driver with [`RunSync`].
 *
 * The body runs to completion before this returns. `ROLLBACK` runs when it
 * throws, and the original error is rethrown; a failed rollback is swallowed
 * so it cannot hide that error. The [`SyncQueryable`] the body receives
 * refuses statements after this returns.
 */
export function transactionOnSync<T>(connection: Connection, body: (tx: SyncQueryable) => T): T {
  const runSync =
    connection.runSync ?? unsupported("a synchronous transaction on an asynchronous connection");
  runSync(connection.begin ?? "BEGIN", [], "exec");
  return scopedSync(runSync, connection, 0, body, "COMMIT", "ROLLBACK");
}

function scopedSync<T>(
  runSync   : RunSync,
  connection: Connection,
  depth     : number,
  body      : (tx: SyncQueryable) => T,
  commit    : string,
  rollback  : string,
): T {
  let open = true;
  const guarded: RunSync = (text, params, mode) => {
    if (!open) {
      throw new SqlError({ kind: "closed" });
    }
    return runSync(text, params, mode);
  };
  const tx: SyncQueryable = {
    engine   : connection.engine,
    maxParams: connection.maxParams,
    querySync: guarded,
    transactionSync: <U>(inner: (tx: SyncQueryable) => U): U => {
      const savepoint = `uf_sp_${depth + 1}`;
      guarded(`SAVEPOINT ${savepoint}`, [], "exec");
      return scopedSync(
        guarded,
        connection,
        depth + 1,
        inner,
        `RELEASE SAVEPOINT ${savepoint}`,
        `ROLLBACK TO SAVEPOINT ${savepoint}`,
      );
    },
  };
  let value: T;
  try {
    value = body(tx);
  } catch (error) {
    open = false;
    try {
      runSync(rollback, [], "exec");
      if (depth > 0) {
        // `ROLLBACK TO` leaves the savepoint in place; release it so the
        // enclosing transaction can go on as if the inner one never started.
        runSync(commit, [], "exec");
      }
    } catch {
      // The rollback failed too — usually because the connection is gone,
      // which rolls back on its own. The error worth reporting is the first.
    }
    throw error;
  }
  // Same as `scoped`: a failed COMMIT or RELEASE must not clear the
  // transaction while the database still holds it.
  open = false;
  try {
    runSync(commit, [], "exec");
  } catch (error) {
    try {
      runSync(rollback, [], "exec");
      if (depth > 0) {
        runSync(commit, [], "exec");
      }
    } catch {
      // The rollback failed too. The error worth reporting is the commit's.
    }
    throw error;
  }
  return value;
}

type Handle = Queryable & SyncQueryable;

/**
 * One connection serving a whole application: a SQLite database, or an
 * embedded PostgreSQL.
 *
 * While an asynchronous transaction is open, every asynchronous statement on
 * the returned value waits for it to finish, so a request that is not in the
 * transaction never lands inside it. Statements on the transaction's own
 * handle do not wait. A synchronous call during that wait throws, because
 * this thread cannot block until the promise settles.
 *
 * A synchronous transaction does not yield, so nothing else runs until it
 * returns. A call on this outer handle from inside that body throws: waiting
 * for the body to finish is the body itself.
 */
function connect(connection: Connection): Handle {
  let gate: Promise<void> | null = null;
  let syncOpen = false;
  const runSync = connection.runSync;

  const query = async (
    text  : string,
    params: $ReadOnlyArray<SqlParam>,
    mode  : QueryMode,
  ): Promise<QueryResult> => {
    if (syncOpen) {
      unsupported("an asynchronous query while a synchronous transaction is open");
    }
    while (gate !== null) {
      await gate;
    }
    return connection.run(text, params, mode);
  };
  const transaction = async <T>(body: (tx: Queryable) => Promise<T>): Promise<T> => {
    if (syncOpen) {
      unsupported("an asynchronous transaction while a synchronous transaction is open");
    }
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
  const querySync = (
    text  : string,
    params: $ReadOnlyArray<SqlParam>,
    mode  : QueryMode,
  ): QueryResult => {
    const run =
      connection.runSync ?? unsupported("a synchronous query on an asynchronous connection");
    if (gate !== null) {
      unsupported("a synchronous query while an asynchronous transaction is open");
    }
    if (syncOpen) {
      unsupported(
        "a synchronous query on the outer connection while a synchronous transaction is open",
      );
    }
    return run(text, params, mode);
  };
  const rawCopy = connection.copy;
  const copy =
    rawCopy === undefined
      ? undefined
      : async (statement: string, payload: string): Promise<number> => {
          if (syncOpen) {
            unsupported("a COPY while a synchronous transaction is open");
          }
          while (gate !== null) {
            await gate;
          }
          return rawCopy(statement, payload);
        };
  const transactionSync = <T>(body: (tx: SyncQueryable) => T): T => {
    if (runSync == null) {
      unsupported("a synchronous transaction on an asynchronous connection");
    }
    if (gate !== null) {
      unsupported("a synchronous transaction while an asynchronous transaction is open");
    }
    if (syncOpen) {
      unsupported(
        "a synchronous transaction on the outer connection while a synchronous transaction is open",
      );
    }
    syncOpen = true;
    try {
      return transactionOnSync(connection, body);
    } finally {
      syncOpen = false;
    }
  };
  return {
    engine   : connection.engine,
    maxParams: connection.maxParams,
    query,
    transaction,
    querySync,
    transactionSync,
    ...(copy === undefined ? null : { copy }),
  };
}

/**
 * A [`Queryable`] over one connection.
 *
 * The returned object also has `querySync` and `transactionSync` when
 * `connection.runSync` is set. [`singleConnectionSync`] is that fact in the
 * type, for the SQLite adapters.
 */
export function singleConnection(connection: Connection): Queryable {
  return connect(connection);
}

/**
 * [`singleConnection`] typed as both [`Queryable`] and [`SyncQueryable`].
 *
 * Requires `connection.runSync`. Without it this throws, rather than handing
 * back a handle whose synchronous methods fail on first use.
 */
export function singleConnectionSync(connection: Connection): Queryable & SyncQueryable {
  if (connection.runSync == null) {
    unsupported("a synchronous connection");
  }
  return connect(connection);
}
