// @flow
//
// The generated code for sqlc's MySQL examples and uf's MySQL type case,
// through the `mysql2` adapter, against the MySQL server `UF_SQLC_MYSQL_URL`
// names (a `mysql://` URL for a user that may create databases).
//
// There is no in-process MySQL to fall back on, so without the variable these
// suites are reported skipped — except in CI, where the service container
// provides one and a missing variable is a broken job, not a reason to skip.

import { describe, expect, it } from "@uniflowed/test";
import type { Queryable } from "@uniflowed/sql";
import { fromMysql2Pool } from "@uniflowed/sql/mysql2";
import * as authors from "./cases/authors-mysql/gen/query.sql.js";
import * as booktest from "./cases/booktest-mysql/gen/query.sql.js";
import * as types from "./cases/types-mysql/gen/query.sql.js";
import { schemaOf } from "./databases.js";
import { transactionScenario } from "./transactions.js";
import { createDatabase, dropDatabase, pool as poolOn } from "./mysql.mjs";

const url = process.env.UF_SQLC_MYSQL_URL;
if ((url === undefined || url === "") && process.env.CI !== undefined) {
  throw new Error("UF_SQLC_MYSQL_URL is not set; CI's MySQL service should provide it");
}
const suite = url === undefined || url === "" ? describe.skip : describe;
const SLOW = { timeout: 30_000 };

let databases = 0;

/** A fresh database with the case's schema, and a pool on it. */
async function open(caseName: string): Promise<{| db: Queryable, close: () => Promise<void> |}> {
  const name = `uf_${caseName.replace(/\W/g, "_")}_${process.pid}_${databases++}`;
  await createDatabase(String(url), name, schemaOf(caseName));
  const pool = poolOn(String(url), name);
  return {
    db: fromMysql2Pool(pool),
    close: async () => {
      await pool.end();
      await dropDatabase(String(url), name);
    },
  };
}

suite("authors (mysql2)", () => {
  it(
    "creates, reads, lists and deletes",
    async () => {
      const { db, close } = await open("authors-mysql");
      const created = await authors.createAuthor(db, {
        name: "Brian Kernighan",
        bio: "Co-author of K&R",
      });
      expect(created.rowsAffected).toBe(1);
      const id = created.lastInsertId ?? 0n;
      await expect(authors.getAuthor(db, { id })).resolves.toEqual({
        id,
        name: "Brian Kernighan",
        bio: "Co-author of K&R",
      });
      await authors.deleteAuthor(db, { id });
      await expect(authors.listAuthors(db)).resolves.toEqual([]);
      await close();
    },
    SLOW,
  );
});

suite("transactions (mysql2)", () => {
  it(
    "commits, rolls back, and nests as savepoints",
    async () => {
      const { db, close } = await open("authors-mysql");
      await transactionScenario(
        db,
        (tx, name) => authors.createAuthor(tx, { name, bio: null }),
        async (tx) => (await authors.listAuthors(tx)).map((author) => author.name),
      );
      await close();
    },
    SLOW,
  );
});

suite("booktest (mysql2)", () => {
  it(
    "runs the example's scenario",
    async () => {
      const { db, close } = await open("booktest-mysql");
      const author = await booktest.createAuthor(db, { name: "Unknown Master" });
      const authorId = Number(author.lastInsertId);
      const created = await booktest.createBook(db, {
        authorId,
        isbn: "1",
        bookType: "FICTION",
        title: "my book title",
        yr: 2016,
        available: "2024-01-02 03:04:05",
        tags: "",
      });
      const bookId = Number(created.lastInsertId);
      const book = await booktest.getBook(db, { bookId });
      expect(book).toMatchObject({
        isbn: "1",
        bookType: "FICTION",
        available: "2024-01-02 03:04:05",
      });
      await booktest.updateBookIsbn(db, { title: "renamed", tags: "cool", isbn: "NEW", bookId });
      await expect(booktest.booksByTags(db, { tags: "cool" })).resolves.toEqual([
        { bookId, title: "renamed", name: "Unknown Master", isbn: "NEW", tags: "cool" },
      ]);
      await close();
    },
    SLOW,
  );
});

suite("types (mysql2)", () => {
  it(
    "reads MySQL's types exactly and binds slices and named parameters",
    async () => {
      const { db, close } = await open("types-mysql");
      await db.query(
        "INSERT INTO people (id, name, age, flag, tiny, price, ratio, born, at, ts, t, y, data, raw, bits, mood, opts, big) " +
          "VALUES (18446744073709551615, 'Ada', 36, 1, -5, '12345678.90', 1.5, '1815-12-10', '2024-01-02 03:04:05.123456', '2024-01-02 03:04:05', '12:34:56', 2024, '{\"a\": [1]}', X'000102', b'10100101', 'happy', 'a,b', -9223372036854775808)",
        [],
        "exec",
      );
      const ada = await types.getPerson(db, { id: 18446744073709551615n });
      expect(ada).toMatchObject({
        id: 18446744073709551615n,
        name: "Ada",
        age: 36,
        flag: true,
        tiny: -5,
        price: "12345678.90",
        ratio: 1.5,
        born: "1815-12-10",
        at: "2024-01-02 03:04:05.123456",
        ts: "2024-01-02 03:04:05",
        t: "12:34:56",
        y: 2024,
        data: { a: [1] },
        mood: "happy",
        opts: "a,b",
        big: -9223372036854775808n,
      });
      expect(Array.from(ada?.raw ?? [])).toEqual([0, 1, 2]);
      expect(Array.from(ada?.bits ?? [])).toEqual([0b10100101]);
      await expect(
        types.byIds(db, { ids: [18446744073709551615n, 1n], skip: "nobody" }),
      ).resolves.toEqual([{ id: 18446744073709551615n, name: "Ada" }]);
      await expect(types.byIds(db, { ids: [], skip: "nobody" })).resolves.toEqual([]);
      await expect(types.nar(db, { min: 30, limit: 1 })).resolves.toEqual([18446744073709551615n]);
      await expect(types.either(db, { n: "Ada" })).resolves.toEqual([18446744073709551615n]);
      await close();
    },
    SLOW,
  );

  it(
    "runs :execlastid, :execrows, :execresult and :copyfrom",
    async () => {
      const { db, close } = await open("types-mysql");
      const id = await types.create(db, { name: "Grace", ratio: 2, mood: "ok" });
      expect(typeof id).toBe("bigint");
      await expect(types.rename(db, { newName: "G", id })).resolves.toBe(1);
      await expect(
        types.copyPets(db, [
          { ownerId: id, name: "Rex" },
          { ownerId: id, name: "Tom" },
        ]),
      ).resolves.toBe(2);
      const rows = await types.embed(db);
      expect(rows.map((row) => row.pets.name)).toEqual(["Rex", "Tom"]);
      await types.touch(db, { id });
      await expect(types.remove(db, { id })).resolves.toMatchObject({ rowsAffected: 1 });
      await close();
    },
    SLOW,
  );
});
