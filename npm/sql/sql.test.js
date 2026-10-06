// @flow
//
// `@uniflowed/sql`: the codecs, the runtime helpers generated code calls, and
// the `node:sqlite` adapter against a real database.
//
// The codecs are where "exact types" is either true or not, so most of this
// file is the awkward values: the integer one past 2^53, a timestamp at a
// half-hour offset in 44 BC, an array element that is the string "NULL"
// rather than a NULL. `tests/sqlc` runs the generated code for sqlc's example
// schemas against SQLite and PostgreSQL; this file is the layer beneath it.

import { describe, expect, it } from "@uniflowed/test";
import type { Connection, QueryResult, Queryable, SqlParam, SyncQueryable } from "@uniflowed/sql";
import {
  SqlError,
  batch,
  batchSync,
  copyFrom,
  copyFromSync,
  execLastId,
  execSync,
  expand,
  many,
  one,
  oneSync,
  singleConnection,
  singleConnectionSync,
  slice,
  transactionOn,
  transactionOnSync,
} from "@uniflowed/sql";
import * as pg from "@uniflowed/sql/postgresql";
import * as mysql from "@uniflowed/sql/mysql";
import * as sqlite from "@uniflowed/sql/sqlite";
import { fromMysql2Pool, typeCast } from "@uniflowed/sql/mysql2";
import type { Mysql2Connection, Mysql2Field } from "@uniflowed/sql/mysql2";
import { fromNodeSqlite } from "@uniflowed/sql/node-sqlite";

function failure(run: () => mixed): mixed {
  try {
    run();
  } catch (error) {
    return error instanceof SqlError ? { kind: error.failure.kind } : error;
  }
  throw new Error("expected a SqlError");
}

describe("postgresql codecs", () => {
  it("reads int8 exactly, past 2^53", () => {
    expect(pg.int8.decode("9007199254740993")).toBe(9007199254740993n);
    expect(pg.int8.decode("-9223372036854775808")).toBe(-9223372036854775808n);
    expect(pg.int8.encode(9007199254740993n)).toBe("9007199254740993");
  });

  it("refuses to round an int8 into a number", () => {
    expect(pg.int8AsNumber.decode("9007199254740991")).toBe(9007199254740991);
    expect(failure(() => pg.int8AsNumber.decode("9007199254740993"))).toMatchObject({
      kind: "decode",
    });
  });

  it("keeps int2 and int4 parameters inside their ranges", () => {
    expect(pg.int4.encode(2147483647)).toBe("2147483647");
    expect(failure(() => pg.int4.encode(2147483648))).toMatchObject({ kind: "encode" });
    expect(failure(() => pg.int2.encode(1.5))).toMatchObject({ kind: "encode" });
    expect(failure(() => pg.int4.decode("12abc"))).toMatchObject({ kind: "decode" });
  });

  it("round-trips the floats PostgreSQL can print", () => {
    expect(pg.float8.decode("NaN")).toBeNaN();
    expect(pg.float8.decode("-Infinity")).toBe(-Infinity);
    expect(pg.float8.decode("1.5e-07")).toBe(1.5e-7);
    expect(pg.float8.encode(Infinity)).toBe("Infinity");
  });

  it("keeps numeric as its exact decimal", () => {
    expect(pg.numeric.decode("12345678901234567890.123456789")).toBe(
      "12345678901234567890.123456789",
    );
    expect(failure(() => pg.numeric.encode("1,5"))).toMatchObject({ kind: "encode" });
  });

  it("reads bool, and nothing but t and f", () => {
    expect(pg.bool.decode("t")).toBe(true);
    expect(pg.bool.decode("f")).toBe(false);
    expect(failure(() => pg.bool.decode("true"))).toMatchObject({ kind: "decode" });
  });

  it("reads bytea in hex and in escape format", () => {
    expect(Array.from(pg.bytea.decode("\\x00ff10"))).toEqual([0, 255, 16]);
    expect(Array.from(pg.bytea.decode("a\\\\b\\000\\377"))).toEqual([97, 92, 98, 0, 255]);
    expect(pg.bytea.encode(new Uint8Array([0, 255, 16]))).toBe("\\x00ff10");
  });

  it("reads timestamptz at any offset as the same instant", () => {
    const instant = Date.UTC(2024, 0, 2, 3, 4, 5, 678);
    expect(pg.timestamptz.decode("2024-01-02 03:04:05.678+00").getTime()).toBe(instant);
    expect(pg.timestamptz.decode("2024-01-02 12:04:05.678+09").getTime()).toBe(instant);
    expect(pg.timestamptz.decode("2024-01-02 08:34:05.678+05:30").getTime()).toBe(instant);
    expect(pg.timestamptz.decode("2024-01-01 23:33:50.678-03:30:15").getTime()).toBe(instant);
    // Microseconds are cut to what a Date holds, not rounded up into the next millisecond.
    expect(pg.timestamptz.decode("2024-01-02 03:04:05.678999+00").getTime()).toBe(instant);
  });

  it("reads years a Date.UTC call would misread", () => {
    const year1 = pg.timestamptz.decode("0001-01-01 00:00:00+00");
    expect(year1.getUTCFullYear()).toBe(1);
    const bc = pg.timestamptz.decode("0044-03-15 12:00:00+00 BC");
    expect(bc.getUTCFullYear()).toBe(-43);
  });

  it("writes timestamptz in ISO DateStyle, including BC and year 10000", () => {
    const bc = pg.timestamptz.decode("0044-03-15 12:00:00+00 BC");
    expect(pg.timestamptz.encode(bc)).toBe("0044-03-15 12:00:00+00 BC");
    const year0 = pg.timestamptz.decode("0001-01-01 00:00:00+00 BC");
    expect(year0.getUTCFullYear()).toBe(0);
    expect(pg.timestamptz.encode(year0)).toBe("0001-01-01 00:00:00+00 BC");
    const year10000 = new Date(0);
    year10000.setUTCFullYear(10000, 0, 1);
    year10000.setUTCHours(0, 0, 0, 0);
    expect(pg.timestamptz.encode(year10000)).toBe("10000-01-01 00:00:00+00");
    const instant = new Date(Date.UTC(2024, 0, 2, 3, 4, 5, 678));
    expect(pg.timestamptz.encode(instant)).toBe("2024-01-02 03:04:05.678+00");
    expect(pg.timestamptz.decode(pg.timestamptz.encode(instant)).getTime()).toBe(instant.getTime());
    expect(pg.timestamptz.decode(pg.timestamptz.encode(bc)).getTime()).toBe(bc.getTime());
  });

  it("refuses infinity rather than handing on an Invalid Date", () => {
    expect(failure(() => pg.timestamptz.decode("infinity"))).toMatchObject({ kind: "decode" });
    expect(pg.timestamptzAsString.decode("infinity")).toBe("infinity");
    expect(failure(() => pg.timestamptz.encode(new Date(NaN)))).toMatchObject({ kind: "encode" });
  });

  it("leaves date and timestamp as the text the server printed", () => {
    expect(pg.date.decode("2024-02-29")).toBe("2024-02-29");
    expect(pg.timestamp.decode("2024-02-29 23:59:59.999999")).toBe("2024-02-29 23:59:59.999999");
  });

  it("parses arrays, quoting and all", () => {
    const text = pg.array(pg.string);
    expect(text.decode('{a,"b c","d\\"e","f\\\\g","NULL"}')).toEqual([
      "a",
      "b c",
      'd"e',
      "f\\g",
      "NULL",
    ]);
    expect(text.decode("{}")).toEqual([]);
    expect(pg.array(pg.int4).decode("[0:2]={1,2,3}")).toEqual([1, 2, 3]);
    expect(pg.array(pg.array(pg.int4)).decode("{{1,2},{3,4}}")).toEqual([
      [1, 2],
      [3, 4],
    ]);
    expect(pg.array(pg.array(pg.int4)).decode("{}")).toEqual([]);
    const boxes = pg.array(pg.text("box"));
    expect(boxes.decode("{(1,1),(0,0);(3,3),(2,2)}")).toEqual(["(1,1),(0,0)", "(3,3),(2,2)"]);
    expect(boxes.encode(["(1,1),(0,0)", "(3,3),(2,2)"])).toBe('{"(1,1),(0,0)";"(3,3),(2,2)"}');
    const grid = pg.array(boxes);
    expect(grid.decode("{{(1,1),(0,0);(2,2),(1,1)};{(3,3),(2,2);(4,4),(3,3)}}")).toEqual([
      ["(1,1),(0,0)", "(2,2),(1,1)"],
      ["(3,3),(2,2)", "(4,4),(3,3)"],
    ]);
    expect(
      grid.encode([
        ["(1,1),(0,0)", "(2,2),(1,1)"],
        ["(3,3),(2,2)", "(4,4),(3,3)"],
      ]),
    ).toBe('{{"(1,1),(0,0)";"(2,2),(1,1)"};{"(3,3),(2,2)";"(4,4),(3,3)"}}');
  });

  it("refuses a NULL element instead of typing it away", () => {
    expect(failure(() => pg.array(pg.int4).decode("{1,NULL}"))).toMatchObject({ kind: "decode" });
    expect(failure(() => pg.array(pg.int4).decode("{{1},{2}}"))).toMatchObject({ kind: "decode" });
  });

  it("writes array literals the server reads back", () => {
    expect(pg.array(pg.string).encode(["a", 'b"c', "d\\e", "NULL"])).toBe(
      '{"a","b\\"c","d\\\\e","NULL"}',
    );
    expect(pg.array(pg.array(pg.int8)).encode([[1n], [2n]])).toBe('{{"1"},{"2"}}');
    expect(pg.array(pg.bytea).encode([new Uint8Array([1])])).toBe('{"\\\\x01"}');
  });

  it("checks enum labels both ways", () => {
    const mood = pg.enumeration("mood", ["sad", "ok"]);
    expect(mood.decode("ok")).toBe("ok");
    expect(failure(() => mood.decode("happy"))).toMatchObject({ kind: "decode" });
    expect(pg.array(mood).decode("{sad,ok}")).toEqual(["sad", "ok"]);
  });

  it("parses and serialises JSON", () => {
    expect(pg.json.decode('{"a": [1, true, null]}')).toEqual({ a: [1, true, null] });
    expect(pg.json.encode({ a: [1] })).toBe('{"a":[1]}');
    expect(failure(() => pg.json.decode("{"))).toMatchObject({ kind: "decode" });
  });

  it("maps an override through the application's functions, inside arrays too", () => {
    const upper = pg.map(
      pg.string,
      (value) => value.toUpperCase(),
      (value) => value.toLowerCase(),
    );
    expect(upper.decode("abc")).toBe("ABC");
    expect(upper.encode("ABC")).toBe("abc");
    expect(pg.array(upper).decode("{a,b}")).toEqual(["A", "B"]);
    expect(pg.array(upper).encode(["A"])).toBe('{"a"}');
  });

  it("reads NULL through nullable and writes it through param", () => {
    expect(pg.nullable(pg.int4, null)).toBeNull();
    expect(pg.param(pg.int4, null)).toBeNull();
    expect(pg.param(pg.int4, 3)).toBe("3");
  });
});

describe("sqlite codecs", () => {
  it("reads INTEGER as a number only while it is exact", () => {
    expect(sqlite.integer.decode(42n)).toBe(42);
    expect(sqlite.integer.decode(42)).toBe(42);
    expect(failure(() => sqlite.integer.decode(9007199254740993n))).toMatchObject({
      kind: "decode",
    });
    expect(sqlite.integerAsBigint.decode(9007199254740993n)).toBe(9007199254740993n);
    // A number past 2^53 from a driver without safe integers has already been rounded.
    expect(failure(() => sqlite.integerAsBigint.decode(2 ** 60))).toMatchObject({
      kind: "decode",
    });
  });

  it("reads BOOLEAN from 0 and 1 only", () => {
    expect(sqlite.boolean.decode(1n)).toBe(true);
    expect(sqlite.boolean.decode(0)).toBe(false);
    expect(failure(() => sqlite.boolean.decode(2))).toMatchObject({ kind: "decode" });
    expect(sqlite.boolean.encode(true)).toBe(1);
  });

  it("reads a BLOB whichever way the driver hands it over", () => {
    expect(Array.from(sqlite.blob.decode(new Uint8Array([1, 2])))).toEqual([1, 2]);
    expect(Array.from(sqlite.blob.decode(new Uint8Array([3]).buffer))).toEqual([3]);
    expect(Array.from(sqlite.blob.decode([4, 5]))).toEqual([4, 5]);
    expect(failure(() => sqlite.blob.decode([256]))).toMatchObject({ kind: "decode" });
  });

  it("refuses the wrong storage class", () => {
    expect(failure(() => sqlite.string.decode(1))).toMatchObject({ kind: "decode" });
    expect(failure(() => sqlite.real("REAL").decode("1.5"))).toMatchObject({ kind: "decode" });
  });
});

describe("mysql codecs", () => {
  it("reads what the text protocol sends", () => {
    expect(mysql.bigint.decode("18446744073709551615")).toBe(18446744073709551615n);
    expect(mysql.integer("int").decode("-5")).toBe(-5);
    expect(mysql.boolean.decode("1")).toBe(true);
    expect(mysql.boolean.decode("0")).toBe(false);
    expect(mysql.decimal.decode("0.10")).toBe("0.10");
    expect(mysql.float("double").decode("1.5e30")).toBe(1.5e30);
    expect(mysql.json.decode("[1]")).toEqual([1]);
  });

  it("sends parameters as text MySQL converts exactly", () => {
    expect(mysql.bigint.encode(18446744073709551615n)).toBe("18446744073709551615");
    expect(mysql.boolean.encode(false)).toBe("0");
    expect(failure(() => mysql.float("double").encode(NaN))).toMatchObject({ kind: "encode" });
  });
});

/** A `Queryable` that records statements and answers from a script. */
function recorder(engine: "postgresql" | "mysql" | "sqlite", maxParams: number = 100) {
  const statements: Array<{| text: string, params: $ReadOnlyArray<SqlParam> |}> = [];
  let transactions = 0;
  const answer = (text: string, params: $ReadOnlyArray<SqlParam>): Promise<QueryResult> => {
    statements.push({ text, params });
    return Promise.resolve({ rows: [], rowsAffected: params.length, lastInsertId: null });
  };
  const db: Queryable = {
    engine,
    maxParams,
    query: answer,
    transaction: async <T>(body: (tx: Queryable) => Promise<T>): Promise<T> => {
      transactions += 1;
      return body(db);
    },
  };
  return { db, statements, transactions: () => transactions };
}

describe("sqlc.slice", () => {
  it("expands each slice where its marker was", () => {
    expect(
      expand(["a IN (", ") AND b = ? AND c IN (", ")"], [slice([1, 2]), "x", slice([3])]),
    ).toEqual({
      text  : "a IN (?, ?) AND b = ? AND c IN (?)",
      params: [1, 2, "x", 3],
    });
  });

  it("matches nothing for an empty slice, as sqlc does", () => {
    expect(expand(["a IN (", ")"], [slice([])])).toEqual({ text: "a IN (NULL)", params: [] });
  });
});

describe(":copyfrom", () => {
  const plan = {
    head : "INSERT INTO t (a, b) VALUES ",
    tuple: ["(", ", ", ")"],
    refs : [0, 1],
    tail : "",
  };

  it("numbers PostgreSQL placeholders across rows", async () => {
    const { db, statements } = recorder("postgresql");
    await expect(
      copyFrom(db, "Copy", plan, [
        ["1", "a"],
        ["2", "b"],
      ]),
    ).resolves.toBe(4);
    expect(statements).toEqual([
      { text: "INSERT INTO t (a, b) VALUES ($1, $2), ($3, $4)", params: ["1", "a", "2", "b"] },
    ]);
  });

  it("chunks under maxParams, inside one transaction", async () => {
    const { db, statements, transactions } = recorder("sqlite", 5);
    const rows = [
      [1, "a"],
      [2, "b"],
      [3, "c"],
      [4, "d"],
      [5, "e"],
    ];
    await copyFrom(db, "Copy", plan, rows);
    expect(statements.map((statement) => statement.params.length)).toEqual([4, 4, 2]);
    expect(statements[0].text).toBe("INSERT INTO t (a, b) VALUES (?, ?), (?, ?)");
    expect(transactions()).toBe(1);
  });

  it("throws when one row needs more parameters than the adapter allows", async () => {
    const wide = {
      head : "INSERT INTO t (a, b, c) VALUES ",
      tuple: ["(", ", ", ", ", ")"],
      refs : [0, 1, 2],
      tail : "",
    };
    const { db, statements } = recorder("sqlite", 2);
    await expect(copyFrom(db, "Copy", wide, [[1, 2, 3]])).rejects.toMatchObject({
      failure: { kind: "params", width: 3, max: 2 },
    });
    expect(statements).toEqual([]);
  });

  it("throws when a row is shorter than an index the statement reads", async () => {
    const { db, statements } = recorder("postgresql");
    await expect(copyFrom(db, "Copy", plan, [["only"]])).rejects.toMatchObject({
      failure: { kind: "row", index: 1, length: 1 },
    });
    expect(statements).toEqual([]);
  });

  it("binds null and ignores fields the statement does not read", async () => {
    const { db, statements } = recorder("postgresql");
    await copyFrom(db, "Copy", plan, [[null, "a", "extra"]]);
    expect(statements).toEqual([
      { text: "INSERT INTO t (a, b) VALUES ($1, $2)", params: [null, "a"] },
    ]);
  });

  it("chunks a synchronous copy inside one transaction", () => {
    const statements: Array<number> = [];
    let transactions = 0;
    const db: SyncQueryable = {
      engine   : "sqlite",
      maxParams: 5,
      querySync: (_text, params) => {
        statements.push(params.length);
        return { rows: [], rowsAffected: params.length, lastInsertId: null };
      },
      transactionSync: (body) => {
        transactions += 1;
        return body(db);
      },
    };
    const rows = [
      [1, "a"],
      [2, "b"],
      [3, "c"],
      [4, "d"],
      [5, "e"],
    ];
    expect(copyFromSync(db, "Copy", plan, rows)).toBe(10);
    expect(statements).toEqual([4, 4, 2]);
    expect(transactions).toBe(1);
  });
});

describe(":batch*", () => {
  it("runs every item in order, in one transaction", async () => {
    const { db, statements, transactions } = recorder("postgresql");
    const results = await batch(db, ["a", "b", "c"], async (q, item) => {
      await q.query(item, [], "exec");
      return item.toUpperCase();
    });
    expect(results).toEqual(["A", "B", "C"]);
    expect(statements.map((statement) => statement.text)).toEqual(["a", "b", "c"]);
    expect(transactions()).toBe(1);
  });

  it("runs a synchronous batch in order, in one transaction", () => {
    const seen: Array<string> = [];
    let transactions = 0;
    const db: SyncQueryable = {
      engine   : "sqlite",
      maxParams: 10,
      querySync: (text) => {
        seen.push(text);
        return { rows: [], rowsAffected: 0, lastInsertId: null };
      },
      transactionSync: (body) => {
        transactions += 1;
        return body(db);
      },
    };
    const results = batchSync(db, ["a", "b"], (q, item) => {
      q.querySync(item, [], "exec");
      return item.toUpperCase();
    });
    expect(results).toEqual(["A", "B"]);
    expect(seen).toEqual(["a", "b"]);
    expect(transactions).toBe(1);
  });
});

describe("running a query", () => {
  const rows = (result: $ReadOnlyArray<$ReadOnlyArray<mixed>>): Queryable => ({
    engine   : "postgresql",
    maxParams: 100,
    query    : () => Promise.resolve({ rows: result, rowsAffected: 0, lastInsertId: null }),
  });

  it("names the query in a decode failure", async () => {
    const error = await many(rows([["x"]]), "ListThings", "", [], 1, (row) =>
      pg.int4.decode(row[0]),
    )
      .then(() => null)
      .catch((caught) => caught);
    expect(error).toBeInstanceOf(SqlError);
    expect(String(error?.message)).toMatch(/^ListThings: expected int4/);
  });

  it("refuses rows of the wrong width, which is how a stale schema shows", async () => {
    const error = await one(rows([[1, 2]]), "GetThing", "", [], 1, (row) => row[0])
      .then(() => null)
      .catch((caught) => (caught instanceof SqlError ? caught.failure : caught));
    expect(error).toEqual({ kind: "shape", expected: 1, actual: 2 });
  });

  it("resolves :one to null when there is no row", async () => {
    await expect(one(rows([]), "GetThing", "", [], 1, (row) => row[0])).resolves.toBeNull();
  });

  it("refuses :execlastid on PostgreSQL rather than invent an id", async () => {
    const error = await execLastId(rows([]), "Insert", "", [])
      .then(() => null)
      .catch((caught) => (caught instanceof SqlError ? caught.failure.kind : caught));
    expect(error).toBe("unsupported");
  });
});

describe("transactions", () => {
  function connection(): {| log: Array<string>, connection: Connection |} {
    const log: Array<string> = [];
    return {
      log,
      connection: {
        engine   : "sqlite",
        maxParams: 100,
        run: (text: string): Promise<QueryResult> => {
          log.push(text);
          return Promise.resolve({ rows: [], rowsAffected: 0, lastInsertId: null });
        },
      },
    };
  }

  it("commits what resolves and rolls back what throws", async () => {
    const { log, connection: conn } = connection();
    await transactionOn(conn, (tx) => tx.query("a", [], "exec"));
    const thrown = await transactionOn<void>(conn, async (tx) => {
      await tx.query("b", [], "exec");
      throw new Error("no");
    }).catch((error) => error.message);
    expect(thrown).toBe("no");
    expect(log).toEqual(["BEGIN", "a", "COMMIT", "BEGIN", "b", "ROLLBACK"]);
  });

  it("nests as savepoints, and an inner failure leaves the outer going", async () => {
    const { log, connection: conn } = connection();
    await transactionOn<void>(conn, async (tx) => {
      const inner = tx.transaction;
      if (inner === undefined) {
        throw new Error("a transaction can open a savepoint");
      }
      await inner<void>(async (sp) => {
        await sp.query("x", [], "exec");
        throw new Error("inner");
      }).catch(() => {});
      await tx.query("y", [], "exec");
    });
    expect(log).toEqual([
      "BEGIN",
      "SAVEPOINT uf_sp_1",
      "x",
      "ROLLBACK TO SAVEPOINT uf_sp_1",
      "RELEASE SAVEPOINT uf_sp_1",
      "y",
      "COMMIT",
    ]);
  });

  it("refuses a statement on a transaction that has ended", async () => {
    const { connection: conn } = connection();
    let leaked: ?Queryable = null;
    await transactionOn(conn, async (tx) => {
      leaked = tx;
    });
    const error = await leaked
      ?.query("late", [], "exec")
      .catch((caught) => (caught instanceof SqlError ? caught.failure.kind : caught));
    expect(error).toBe("closed");
  });

  it("commits and rolls back a synchronous transaction, nesting savepoints", () => {
    const log: Array<string> = [];
    const runSync = (text: string): QueryResult => {
      log.push(text);
      return { rows: [], rowsAffected: 0, lastInsertId: null };
    };
    const connection: Connection = {
      engine   : "sqlite",
      maxParams: 100,
      run      : async (text) => runSync(text),
      runSync,
    };
    transactionOnSync(connection, (tx) => {
      tx.querySync("a", [], "exec");
    });
    expect(() =>
      transactionOnSync(connection, (tx) => {
        const inner = tx.transactionSync;
        if (inner === undefined) {
          throw new Error("a synchronous transaction can open a savepoint");
        }
        expect(() =>
          inner((sp) => {
            sp.querySync("x", [], "exec");
            throw new Error("inner");
          }),
        ).toThrow("inner");
        tx.querySync("y", [], "exec");
        throw new Error("outer");
      }),
    ).toThrow("outer");
    expect(log).toEqual([
      "BEGIN",
      "a",
      "COMMIT",
      "BEGIN",
      "SAVEPOINT uf_sp_1",
      "x",
      "ROLLBACK TO SAVEPOINT uf_sp_1",
      "RELEASE SAVEPOINT uf_sp_1",
      "y",
      "ROLLBACK",
    ]);
  });

  it("refuses a synchronous statement on a transaction that has ended", () => {
    const connection: Connection = {
      engine   : "sqlite",
      maxParams: 100,
      run      : async () => ({ rows: [], rowsAffected: 0, lastInsertId: null }),
      runSync  : () => ({ rows: [], rowsAffected: 0, lastInsertId: null }),
    };
    let leaked: ?SyncQueryable = null;
    transactionOnSync(connection, (tx) => {
      leaked = tx;
    });
    expect(failure(() => leaked?.querySync("late", [], "exec"))).toEqual({ kind: "closed" });
  });

  it("rolls back when COMMIT fails, and rethrows that error", async () => {
    const log: Array<string> = [];
    const connection: Connection = {
      engine   : "sqlite",
      maxParams: 100,
      run: async (text) => {
        log.push(text);
        if (text === "COMMIT") {
          throw new Error("busy");
        }
        return { rows: [], rowsAffected: 0, lastInsertId: null };
      },
    };
    const thrown = await transactionOn(connection, (tx) => tx.query("a", [], "exec")).catch(
      (error) => error.message,
    );
    expect(thrown).toBe("busy");
    expect(log).toEqual(["BEGIN", "a", "COMMIT", "ROLLBACK"]);
  });

  it("rolls a savepoint back when RELEASE fails", async () => {
    const log: Array<string> = [];
    const connection: Connection = {
      engine   : "sqlite",
      maxParams: 100,
      run: async (text) => {
        log.push(text);
        if (text.startsWith("RELEASE")) {
          throw new Error("busy");
        }
        return { rows: [], rowsAffected: 0, lastInsertId: null };
      },
    };
    const thrown = await transactionOn<void>(connection, async (tx) => {
      const inner = tx.transaction;
      if (inner === undefined) {
        throw new Error("a transaction can open a savepoint");
      }
      await inner((sp) => sp.query("x", [], "exec"));
    }).catch((error) => error.message);
    expect(thrown).toBe("busy");
    expect(log).toEqual([
      "BEGIN",
      "SAVEPOINT uf_sp_1",
      "x",
      "RELEASE SAVEPOINT uf_sp_1",
      "ROLLBACK TO SAVEPOINT uf_sp_1",
      "RELEASE SAVEPOINT uf_sp_1",
      "ROLLBACK",
    ]);
  });

  it("rolls back a synchronous transaction when COMMIT fails", () => {
    const log: Array<string> = [];
    const connection: Connection = {
      engine   : "sqlite",
      maxParams: 100,
      run      : async () => ({ rows: [], rowsAffected: 0, lastInsertId: null }),
      runSync: (text) => {
        log.push(text);
        if (text === "COMMIT" || text.startsWith("RELEASE")) {
          throw new Error("busy");
        }
        return { rows: [], rowsAffected: 0, lastInsertId: null };
      },
    };
    expect(() => transactionOnSync(connection, (tx) => tx.querySync("a", [], "exec"))).toThrow(
      "busy",
    );
    expect(() =>
      transactionOnSync(connection, (tx) => {
        const inner = tx.transactionSync;
        if (inner === undefined) {
          throw new Error("a synchronous transaction can open a savepoint");
        }
        inner((sp) => sp.querySync("x", [], "exec"));
      }),
    ).toThrow("busy");
    expect(log).toEqual([
      "BEGIN",
      "a",
      "COMMIT",
      "ROLLBACK",
      "BEGIN",
      "SAVEPOINT uf_sp_1",
      "x",
      "RELEASE SAVEPOINT uf_sp_1",
      "ROLLBACK TO SAVEPOINT uf_sp_1",
      "RELEASE SAVEPOINT uf_sp_1",
      "ROLLBACK",
    ]);
  });

  it("holds other statements back while a single connection is in a transaction", async () => {
    const { log, connection: conn } = connection();
    const db = singleConnection(conn);
    const transaction = db.transaction;
    if (transaction === undefined) {
      throw new Error("a single connection has transactions");
    }
    let release: () => void = () => {};
    const inside = transaction(async (tx) => {
      await tx.query("in-1", [], "exec");
      await new Promise<void>((resolve) => {
        release = resolve;
      });
      await tx.query("in-2", [], "exec");
    });
    await Promise.resolve();
    const outside = db.query("outside", [], "exec");
    await Promise.resolve();
    release();
    await Promise.all([inside, outside]);
    expect(log).toEqual(["BEGIN", "in-1", "in-2", "COMMIT", "outside"]);
  });

  it("refuses to mix a synchronous call into an open asynchronous transaction", async () => {
    const runSync = (): QueryResult => ({ rows: [], rowsAffected: 0, lastInsertId: null });
    const db = singleConnectionSync({
      engine   : "sqlite",
      maxParams: 100,
      run      : async () => runSync(),
      runSync,
    });
    const transaction = db.transaction;
    if (transaction === undefined) {
      throw new Error("a single connection has transactions");
    }
    let release: () => void = () => {};
    let markEntered: () => void = () => {};
    const entered = new Promise<void>((resolve) => {
      markEntered = () => resolve();
    });
    const inside = transaction(async (tx) => {
      await tx.query("in", [], "exec");
      markEntered();
      await new Promise<void>((resolve) => {
        release = resolve;
      });
    });
    await entered;
    expect(failure(() => db.querySync("outside", [], "exec"))).toEqual({ kind: "unsupported" });
    release();
    await inside;
  });

  it("refuses the outer handle while a synchronous transaction is open", () => {
    const db = singleConnectionSync({
      engine   : "sqlite",
      maxParams: 100,
      run      : async () => ({ rows: [], rowsAffected: 0, lastInsertId: null }),
      runSync  : () => ({ rows: [], rowsAffected: 0, lastInsertId: null }),
    });
    const transactionSync = db.transactionSync;
    if (transactionSync === undefined) {
      throw new Error("a synchronous connection has transactions");
    }
    transactionSync((tx) => {
      tx.querySync("in", [], "exec");
      expect(failure(() => db.querySync("out", [], "exec"))).toEqual({ kind: "unsupported" });
      expect(failure(() => db.transactionSync?.(() => 1))).toEqual({ kind: "unsupported" });
    });
    execSync(db, "After", "after", []);
  });
});

describe("node:sqlite", () => {
  async function open(): Promise<Queryable & SyncQueryable> {
    const { DatabaseSync } = await import("node:sqlite");
    const database = new DatabaseSync(":memory:");
    database.exec(
      "CREATE TABLE t (id INTEGER PRIMARY KEY, big INTEGER, name TEXT NOT NULL, data BLOB)",
    );
    return fromNodeSqlite(database);
  }

  it("returns rows positionally, with safe integers", async () => {
    const db = await open();
    const inserted = await db.query(
      "INSERT INTO t (big, name, data) VALUES (?, ?, ?)",
      [9007199254740993n, "a", new Uint8Array([1])],
      "exec",
    );
    expect(inserted).toEqual({ rows: [], rowsAffected: 1, lastInsertId: 1n });
    const { rows } = await db.query("SELECT id, big, name, data, id FROM t", [], "rows");
    expect(rows[0][1]).toBe(9007199254740993n);
    expect(rows[0]).toHaveLength(5);
    expect(sqlite.integerAsBigint.decode(rows[0][1])).toBe(9007199254740993n);
  });

  it("rolls a failed transaction back", async () => {
    const db = await open();
    const transaction = db.transaction;
    if (transaction === undefined) {
      throw new Error("node:sqlite has transactions");
    }
    await transaction(async (tx) => {
      await tx.query("INSERT INTO t (name) VALUES (?)", ["kept"], "exec");
    });
    await transaction<void>(async (tx) => {
      await tx.query("INSERT INTO t (name) VALUES (?)", ["dropped"], "exec");
      throw new Error("no");
    }).catch(() => {});
    const { rows } = await db.query("SELECT name FROM t", [], "rows");
    expect(rows).toEqual([["kept"]]);
  });

  it("reads and rolls back through the synchronous API", async () => {
    const db = await open();
    const transactionSync = db.transactionSync;
    if (transactionSync === undefined) {
      throw new Error("node:sqlite has synchronous transactions");
    }
    transactionSync((tx) => {
      tx.querySync("INSERT INTO t (name) VALUES (?)", ["kept"], "exec");
    });
    expect(() =>
      transactionSync((tx) => {
        tx.querySync("INSERT INTO t (name) VALUES (?)", ["dropped"], "exec");
        throw new Error("no");
      }),
    ).toThrow("no");
    expect(oneSync(db, "GetName", "SELECT name FROM t", [], 1, (row) => row[0])).toBe("kept");
    const names = db.querySync("SELECT name FROM t", [], "rows").rows;
    expect(names).toEqual([["kept"]]);
  });
});

// The real driver runs against MySQL in `tests/sqlc/mysql.test.js`, where CI
// has a server. These pin the two things that went wrong there first, without
// one.
describe("mysql2 adapter", () => {
  function field(type: string, text: string): Mysql2Field {
    return {
      type,
      string: (encoding) => (encoding === undefined || encoding === "utf8" ? text : null),
    };
  }

  it("reads JSON and DECIMAL columns as text, whatever the pool was opened with", () => {
    const parsed = () => ({ parsed: true });
    expect(typeCast(field("JSON", '{"a":1}'), parsed)).toBe('{"a":1}');
    expect(typeCast(field("NEWDECIMAL", "1.10"), parsed)).toBe("1.10");
    expect(typeCast(field("LONGLONG", "1"), parsed)).toEqual({ parsed: true });
  });

  it("sends a parameterless statement run for effect over the text protocol", async () => {
    const calls: Array<string> = [];
    const answer =
      (protocol: string): Mysql2Connection["execute"] =>
      async (options) => {
        calls.push(`${protocol} ${options.sql}`);
        return [{ affectedRows: 0, insertId: 0 }, undefined];
      };
    const connection = { execute: answer("execute"), query: answer("query") };
    const db = fromMysql2Pool({
      ...connection,
      getConnection: async () => ({ ...connection, release: () => {} }),
    });
    const transaction = db.transaction;
    if (transaction === undefined) {
      throw new Error("mysql2 pools have transactions");
    }
    await transaction(async (tx) => {
      await tx.query("UPDATE t SET a = ?", ["1"], "exec");
      const savepoint = tx.transaction;
      if (savepoint !== undefined) {
        await savepoint(async () => {});
      }
    });
    await db.query("SELECT 1", [], "rows");
    expect(calls).toEqual([
      "query BEGIN",
      "execute UPDATE t SET a = ?",
      "query SAVEPOINT uf_sp_1",
      "query RELEASE SAVEPOINT uf_sp_1",
      "query COMMIT",
      "execute SELECT 1",
    ]);
  });
});
