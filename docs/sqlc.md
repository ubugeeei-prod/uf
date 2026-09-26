# sqlc for Flow

uf does not ship an ORM, and [red-lines.md](red-lines.md#what-is-not-on-the-table)
says why: the server half of a uf application is a BFF, and a team's domain,
database and migrations live in its own service. A BFF still reads data, and
sometimes it owns a small store of its own: a session table, a cache of what
the UI shows, a SQLite file at the edge. For that, uf gives
[sqlc](https://sqlc.dev) a **Flow target**. You write SQL, sqlc parses and
type-checks it against your schema, and uf writes Flow row types and query
functions that call your driver.

This page is the design record: what was decided, why, and what each piece
promises. The guide is [SQL with sqlc](app/guide/sqlc/$page.mdx) on the
documentation site (`/guide/sqlc`).

## Status

**The target as a whole is Experimental.** Generation, the runtime and the
tested adapters work, and CI runs them against real databases, but
`@uniflowed/sql` — which every generated module imports — is not on npm yet
(it waits in `tools/release/pending-packages.txt` for trusted publishing,
#1314). Until it publishes, a project outside this repository cannot install
what the generator writes against, and nothing here is production-ready.

| Piece | Status | Checked by |
| --- | --- | --- |
| Generator (`crates/uf_sqlc`), protobuf and JSON requests | Implemented | golden tests over sqlc's examples (`crates/uf_sqlc/tests/golden.rs`) |
| `uf` as a sqlc process plugin (`cmd: uf`) | Implemented | `crates/uf_cli/tests/sqlc.rs`; the `sqlc Flow target` CI job, with the pinned real sqlc |
| `uf sqlc generate` / `uf sqlc diff` | Implemented | `crates/uf_cli/tests/sqlc.rs` (a stand-in sqlc); `tools/ci/sqlc.sh` (the pinned real one) |
| Generated code passes `uf check`, `uf lint` and `uf fmt --check` | Implemented | `tools/ci/sqlc.sh`, over every case |
| Runtime `@uniflowed/sql`: codecs, slices, `:copyfrom`, `:batch*`, transactions and savepoints | Implemented | `npm/sql/sql.test.js`; `tests/sqlc/transactions.js` against every tested adapter |
| Adapter: `node:sqlite` | Implemented | `npm/sql/sql.test.js`, `tests/sqlc/sqlite.test.js` |
| Adapter: `bun:sqlite` | Implemented | `tests/sqlc/sqlite.test.js` under `uf test --host bun`, on Bun 1.3.14 in CI |
| Adapters: PGlite, `pg` (`fromPgClient`, `fromPgPool`), `postgres` | Implemented | `tests/sqlc/postgresql.test.js`: every scenario under all four, against a real PostgreSQL (PGlite, reached by `pg` and `postgres` over the wire protocol) |
| Adapter: `mysql2` (`fromMysql2Pool`) | Implemented | `tests/sqlc/mysql.test.js`, against the MySQL 8.4 service container of the `sqlc Flow target` CI job |
| Adapters: `better-sqlite3`, Cloudflare D1 | Experimental | typed against the drivers' documented APIs; no test runs them yet |
| `tools/ci/sqlc.sh` in CI | Implemented | the `sqlc Flow target` job in `.github/workflows/ci.yml` (`uf run test:sqlc`) |
| `@uniflowed/sql` on npm | Planned | #1314; `tools/release/pending-packages.txt` |
| WASM plugin (`wasm32-wasip1`) | Planned | the `sqlc-gen-flow` binary exists; nothing builds or publishes it for the target yet |
| PostgreSQL `COPY` for `:copyfrom` | Planned | `:copyfrom` is chunked multi-row `INSERT`s on every engine today |
| Installing sqlc for a project | Planned | `uf sqlc` runs the project's own sqlc (`$SQLC` or `PATH`) |

"Implemented" means a test in this repository runs it. A row with no test is
never above Experimental. #1367 tracks the rest.

## Decisions

### sqlc stays sqlc

sqlc parses SQL, resolves the catalog and infers parameter and column types.
uf does none of that. It reads sqlc's `plugin.GenerateRequest` and writes Flow.
The project keeps a normal `sqlc.yaml` (or `sqlc.json`) that sqlc itself reads,
so every sqlc feature, flag and upgrade reaches a uf project unchanged, and a
project can leave uf and keep its SQL.

This is the same line uf draws for Relay: upstream owns the compiler and its
config; uf owns the Flow it produces and the build around it.

### The generator is Rust, and ships inside `uf`

Code generation is a build-time hot path, so it is native
(`ubugeeei-redundancy.md`, "Native Hot Paths at Scale"). `crates/uf_sqlc`
decodes the request, maps types and prints modules. It has no dependency
beyond `serde`/`serde_json`: the protobuf reader is a small hand-written wire
decoder over the eight messages sqlc sends, so the crate also builds for
`wasm32-wasip1` without `protoc` or `prost`.

sqlc has two plugin transports, and both carry the same protobuf:

- **process**: sqlc runs `cmd` with the single argument
  `/plugin.CodegenService/Generate`, writes the request to stdin and reads the
  response from stdout. The `uf` binary answers that argument itself, so
  `cmd: uf` is the whole setup. sqlc clears the environment for the child
  (only `SQLC_VERSION` and the names under `env:` survive), which a native
  binary does not mind.
- **wasm**: sqlc downloads a module by URL and checks its sha256. The crate
  has a `sqlc-gen-flow` binary for that target. Publishing it as a release
  asset belongs to the release pipeline and is Planned.

`uf sqlc generate` runs `sqlc generate` with the running `uf` first on `PATH`,
so the plugin sqlc starts is the same `uf` that was asked. `uf sqlc diff` runs
`sqlc diff`, which exits non-zero when checked-in output is stale; that is the
CI check.

### Generated code is formatted, and marked generated

The printer emits the layout `uf fmt` produces for these shapes, and every
file starts with `@generated SignedSource<<…>>`. `uf fmt` leaves a signed file
alone, so a project whose own `fmt` settings differ never fights the
generator. The signature is Meta's SignedSource scheme (MD5 of the file with a
placeholder token), the same one Relay's compiler writes.

### One runtime contract, text in and text out

Drivers disagree about everything a type system cares about. `pg` returns
`int8` as a string and `timestamp` as a `Date` in the process's zone;
`postgres` parses `date` into a `Date` at local midnight; `better-sqlite3`
returns numbers past 2^53 rounded unless asked not to; D1 cannot return a
bigint at all. A generator that trusted any of them would print types that are
true for one driver and false for the next.

So the generator does not target a driver. It targets a small interface,

```js
export interface Queryable {
  +engine: "postgresql" | "mysql" | "sqlite";
  +maxParams: number;
  query(text: string, params: $ReadOnlyArray<SqlParam>): Promise<QueryResult>;
}
```

and every adapter's one job is to hand rows back **positionally** and **in the
wire's own representation**: PostgreSQL's text format (`pg` with every type
parser disabled, `postgres`' `.raw()`, PGlite with identity parsers), MySQL's
text form (`mysql2` with a `typeCast` that returns the string), and SQLite's
storage classes with safe integers on. The generated code then decodes each
column with the codec its SQL type calls for. The same codecs run under every
driver, so a type is exact or it is a bug with one place to fix.

Rows are positional because a join can return two columns called `id`, and
because `sqlc.embed` expands to a run of columns that only positions can split.

### Exact types

The defaults, and the option that changes each:

| SQL | Flow | Notes |
| --- | --- | --- |
| `smallint`, `integer`, `serial`, MySQL `int`/`year` | `number` | |
| `bigint`, `bigserial`, MySQL `bigint` | `bigint` | `int8: "number"` gives a checked `number` that throws past 2^53; `int8: "string"` gives the digits |
| SQLite `INTEGER` | `number`, checked | `sqliteInteger: "bigint"`. Every SQLite integer is 64-bit, and making every row id a `bigint` is a tax for a case most tables never meet; the checked number throws rather than rounds |
| `numeric`, `decimal`, `money` | `string` | never a lossy `number` by default; `numeric: "number"` opts in |
| `real`, `double precision`, `float` | `number` | `NaN` and `±Infinity` round-trip |
| `boolean`, MySQL `tinyint(1)` | `boolean` | |
| text types, `uuid`, `inet`, `interval`, `time`, ranges, unknown PostgreSQL types | `string` | the text format of an unknown type is a string, so that type is still true |
| `bytea`, `blob`, MySQL `binary` and `bit` | `Uint8Array` | |
| `date` | `string` (`YYYY-MM-DD`) | a calendar date has no time zone; a `Date` would invent one |
| `timestamp` (without time zone), MySQL `datetime` and `timestamp` | `string` | wall-clock time as the server printed it; no zone is assumed |
| `timestamptz` | `Date` | an instant. `timestamptz: "string"` keeps microseconds, which a `Date` cannot |
| `json`, `jsonb` | `JsonValue` | an override with a `decode` function narrows it; see below |
| enums | a union of string literals, and a `$ReadOnlyArray` of the values | MySQL `ENUM` columns too |
| arrays | `$ReadOnlyArray<T>`, nested per dimension | a `NULL` element throws: sqlc does not tell a plugin whether elements may be null, and silently typing them non-null would be false |
| pseudo-types sqlc could not resolve (`any`, `anyarray`, …) | `mixed`, nullable | write a cast (`max(x)::timestamptz`) and the column gets a real type |

A nullable column is `T | null`, never `?T`: drivers return `null`, and `?T`
would also admit `undefined`, which nothing produces.

### Queries

- Every generated function takes the `Queryable` first and an arguments object
  second (omitted when the query has no parameters): `getAuthor(db, { id })`.
  Transactions pass the transaction in the same position.
- `:one` resolves to the row or `null`; `:many` to an `Array` of rows;
  `:exec` to `void`; `:execrows` to the affected row count; `:execresult` to
  `{ rowsAffected, lastInsertId }`; `:execlastid` to the last id as a `bigint`.
- `:copyfrom` inserts an array of argument objects with multi-row `INSERT`s,
  chunked under the adapter's `maxParams` (PostgreSQL and MySQL 65,535, SQLite
  32,766, D1 100). It works on every engine; PostgreSQL's `COPY` protocol is
  Planned as an adapter fast path.
- `:batchexec`, `:batchmany` and `:batchone` take an array of argument objects
  and return one result per item, in order, inside one transaction when the
  `Queryable` can open one — the semantics of pgx's implicit batch
  transaction, without pgx.
- `sqlc.arg` and `@name` name an argument; `sqlc.narg` makes it `T | null`;
  `sqlc.slice` takes a `$ReadOnlyArray<T>` (a PostgreSQL array, or an expanded
  `IN (?, ?, …)` list on MySQL and SQLite, `IN (NULL)` when empty — sqlc's own
  semantics); `sqlc.embed(t)` nests the table's row type under its name.
- Placeholders are re-numbered at generation time. sqlc's MySQL and SQLite
  output mixes bare `?` with numbered `?N`, which SQLite binds by a different
  rule than sqlc assumed; the generator resolves each placeholder to its
  parameter once and emits positional `?` in bind order.

### Options

Plugin options live under the codegen entry in `sqlc.yaml`:

```yaml
codegen:
  - out: src/db
    plugin: flow
    options:
      int8: bigint            # bigint | number | string
      sqliteInteger: number   # number | bigint
      numeric: string         # string | number
      timestamptz: Date       # Date | string
      naming: camelCase       # camelCase | preserve
      rename:
        spotify_url: spotifyURL
      overrides:
        - column: authors.settings
          type: { import: "../settings.js", name: "Settings", decode: "parseSettings" }
        - db_type: uuid
          type: { import: "../ids.js", name: "Uuid", decode: "toUuid", encode: "fromUuid" }
```

sqlc no longer forwards its own `overrides` and `rename` to plugins (fields 5
and 6 of `Settings` are reserved), so these are the plugin's own, spelled after
sqlc-gen-go's. An override that changes a type must name a `decode` function —
`(value: T) => U` from the default representation — because Flow cannot be
told that a string is a `Uuid` without either a check or an `any`, and uf does
not generate `any`. `encode` is needed when the override appears in a parameter
and the default encoder does not already accept it.

## What is not here

- Migrations. sqlc reads a schema; applying it is the application's migration
  tool's job.
- A connection pool, retries, or query logging. Those belong to the driver the
  application already chose.
- An ORM-shaped query builder. Applications use their database driver and
  sqlc-generated queries directly.
