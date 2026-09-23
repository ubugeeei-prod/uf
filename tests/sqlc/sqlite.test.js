// @flow
//
// The generated code for sqlc's SQLite examples, and uf's SQLite type case,
// run against a real SQLite, in memory, through the host's own driver:
// `node:sqlite` under Node and `bun:sqlite` under Bun (`databases.js`). A port
// of the scenarios in sqlc's own `db_test.go` for each example, plus the
// values a type claim is about.

import { describe, expect, it } from "@uniflowed/test";
import { SqlError } from "@uniflowed/sql";
import * as authors from "./cases/authors-sqlite/gen/query.sql.js";
import * as booktest from "./cases/booktest-sqlite/gen/query.sql.js";
import * as city from "./cases/ondeck-sqlite/gen/city.sql.js";
import * as venue from "./cases/ondeck-sqlite/gen/venue.sql.js";
import * as types from "./cases/types-sqlite/gen/query.sql.js";
import { openSqlite, sqliteAdapter } from "./databases.js";
import { transactionScenario } from "./transactions.js";

describe(`authors (${sqliteAdapter})`, () => {
  it("creates, reads, lists and deletes", async () => {
    const { db, close } = await openSqlite("authors-sqlite");
    const created = await authors.createAuthor(db, {
      name: "Brian Kernighan",
      bio: "Co-author of K&R",
    });
    expect(created.rowsAffected).toBe(1);
    const id = Number(created.lastInsertId);
    await expect(authors.getAuthor(db, { id })).resolves.toEqual({
      id,
      name: "Brian Kernighan",
      bio: "Co-author of K&R",
    });
    await authors.createAuthor(db, { name: "Anonymous", bio: null });
    const listed = await authors.listAuthors(db);
    expect(listed.map((author) => author.name)).toEqual(["Anonymous", "Brian Kernighan"]);
    expect(listed[0].bio).toBeNull();
    await authors.deleteAuthor(db, { id });
    await expect(authors.getAuthor(db, { id })).resolves.toBeNull();
    await close();
  });
});

describe(`transactions (${sqliteAdapter})`, () => {
  it("commits, rolls back, and nests as savepoints", async () => {
    const { db, close } = await openSqlite("authors-sqlite");
    await transactionScenario(
      db,
      (tx, name) => authors.createAuthor(tx, { name, bio: null }),
      async (tx) => (await authors.listAuthors(tx)).map((author) => author.name),
    );
    await close();
  });
});

describe(`booktest (${sqliteAdapter})`, () => {
  it("runs the example's whole scenario", async () => {
    const { db, close } = await openSqlite("booktest-sqlite");
    const author = await booktest.createAuthor(db, { name: "Unknown Master" });
    expect(author).not.toBeNull();
    const authorId = author?.authorId ?? 0;
    const book = (isbn: string, title: string, yr: number, tag: string) =>
      booktest.createBook(db, {
        authorId,
        isbn,
        bookType: "FICTION",
        title,
        yr,
        available: "2024-01-02 03:04:05",
        tag,
      });
    const first = await book("1", "my book title", 2016, "");
    await book("2", "the second book", 2016, "cool unique");
    await book("3", "the third book", 2001, "cool");
    expect(first?.available).toBe("2024-01-02 03:04:05");
    const bookId = first?.bookId ?? 0;
    await booktest.updateBook(db, { title: "changed second title", tag: "cool", bookId });
    await booktest.updateBookIsbn(db, {
      title: "never ever gonna finish",
      tag: "someday",
      bookId,
      isbn: "NEW ISBN",
    });
    const byYear = await booktest.booksByTitleYear(db, { title: "the third book", yr: 2001 });
    expect(byYear.map((row) => row.isbn)).toEqual(["3"]);
    const tagged = await booktest.booksByTags(db, { tags: ["cool", "someday"] });
    expect(tagged.map((row) => row.isbn).sort()).toEqual(["3", "NEW ISBN"]);
    expect(tagged.find((row) => row.isbn === "3")?.name).toBe("Unknown Master");
    // `sqlc.slice` with nothing in it matches nothing, rather than every row.
    await expect(booktest.booksByTags(db, { tags: [] })).resolves.toEqual([]);
    await booktest.deleteBook(db, { bookId });
    await expect(booktest.getBook(db, { bookId })).resolves.toBeNull();
    await close();
  });
});

describe(`ondeck (${sqliteAdapter})`, () => {
  it("creates cities and venues and counts them", async () => {
    const { db, close } = await openSqlite("ondeck-sqlite");
    await city.createCity(db, { name: "San Francisco", slug: "san-francisco" });
    await expect(city.getCity(db, { slug: "san-francisco" })).resolves.toEqual({
      slug: "san-francisco",
      name: "San Francisco",
    });
    const created = await venue.createVenue(db, {
      slug: "the-fillmore",
      name: "The Fillmore",
      city: "san-francisco",
      spotifyPlaylist: "spotify:uri",
      status: "open",
      statuses: "open,closed",
      tags: "rock,punk",
    });
    expect(created.rowsAffected).toBe(1);
    await venue.updateVenueName(db, { name: "The Fillmore West", slug: "the-fillmore" });
    const got = await venue.getVenue(db, { slug: "the-fillmore", city: "san-francisco" });
    expect(got?.name).toBe("The Fillmore West");
    expect(typeof got?.createdAt).toBe("string");
    await expect(venue.venueCountByCity(db)).resolves.toEqual([
      { city: "san-francisco", count: 1 },
    ]);
    await venue.deleteVenue(db, { slug: "the-fillmore", slug2: "the-fillmore" });
    await expect(venue.listVenues(db, { city: "san-francisco" })).resolves.toEqual([]);
    await city.updateCityName(db, { name: "SF", slug: "san-francisco" });
    await expect(city.listCities(db)).resolves.toEqual([{ slug: "san-francisco", name: "SF" }]);
    await close();
  });
});

describe(`types (${sqliteAdapter})`, () => {
  async function seeded() {
    const open = await openSqlite("types-sqlite");
    await open.db.query(
      "INSERT INTO people (id, name, age, big, flag, price, dec, ratio, dbl, born, at, ts, data, jb, raw, vc, weird) " +
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, jsonb(?), ?, ?, ?)",
      [
        1,
        "Ada",
        36,
        9007199254740991n,
        1,
        12.5,
        0.1,
        1.5,
        2.25,
        "1815-12-10",
        "2024-01-02 03:04:05",
        "2024-01-02T03:04:05Z",
        '{"a":[1,2]}',
        '{"b":true}',
        new Uint8Array([1, 2, 3]),
        "short",
        "anything",
      ],
      "exec",
    );
    await open.db.query("INSERT INTO people (id, name) VALUES (?, ?)", [2, "Grace"], "exec");
    return open;
  }

  it("reads every declared type as its Flow type", async () => {
    const { db, close } = await seeded();
    const ada = await types.getPerson(db, { id: 1 });
    expect(ada).toEqual({
      id: 1,
      name: "Ada",
      age: 36,
      big: 9007199254740991,
      flag: true,
      price: 12.5,
      dec: 0.1,
      ratio: 1.5,
      dbl: 2.25,
      born: "1815-12-10",
      at: "2024-01-02 03:04:05",
      ts: "2024-01-02T03:04:05Z",
      data: { a: [1, 2] },
      jb: { b: true },
      raw: new Uint8Array([1, 2, 3]),
      vc: "short",
      weird: "anything",
    });
    const grace = await types.getPerson(db, { id: 2 });
    expect(grace?.age).toBeNull();
    expect(grace?.data).toBeNull();
    await close();
  });

  it("refuses an INTEGER a number cannot hold, instead of rounding it", async () => {
    const { db, close } = await seeded();
    await db.query("UPDATE people SET big = ? WHERE id = 1", [9007199254740993n], "exec");
    const error = await types
      .getPerson(db, { id: 1 })
      .then(() => null)
      .catch((caught) => caught);
    expect(error).toBeInstanceOf(SqlError);
    expect(String(error?.message)).toMatch(/^GetPerson: expected INTEGER within ±2\^53/);
    await close();
  });

  it("binds sqlc.slice, sqlc.narg and SQLite's numbered placeholders by what sqlc meant", async () => {
    const { db, close } = await seeded();
    await expect(types.byIds(db, { ids: [1, 2], skip: "Grace" })).resolves.toEqual([
      { id: 1, name: "Ada" },
    ]);
    // `age > ?2 LIMIT ?`: SQLite would bind that bare `?` as parameter 3.
    await expect(types.nar(db, { min: 30, limit: 5 })).resolves.toEqual([1]);
    await expect(types.nar(db, { min: null, limit: 5 })).resolves.toEqual([]);
    // `name = ?2 AND age = ?1`.
    await expect(types.num(db, { age: 36, name: "Ada" })).resolves.toBe(1);
    await expect(
      types.mixed(db, { ids: [1, 2], name: "Ada", ages: [36], big: 9007199254740991 }),
    ).resolves.toEqual([1]);
    await expect(types.either(db, { n: "short" })).resolves.toEqual([1]);
    await close();
  });

  it("nests sqlc.embed rows", async () => {
    const { db, close } = await seeded();
    await types.ret(db, { ownerId: 1, name: "Rex" });
    const rows = await types.embed(db);
    expect(rows).toHaveLength(1);
    expect(rows[0].people.name).toBe("Ada");
    expect(rows[0].pets).toEqual({ id: 1, ownerId: 1, name: "Rex" });
    await close();
  });

  it("runs the exec commands, :copyfrom and :batchmany", async () => {
    const { db, close } = await seeded();
    const id = await types.create(db, { name: "Linus" });
    expect(id).toBe(3n);
    await expect(types.rename(db, { newName: "Ada L.", id: 1 })).resolves.toBe(1);
    await expect(types.remove(db, { id: 2 })).resolves.toEqual({
      rowsAffected: 1,
      lastInsertId: 3n,
    });
    await types.touch(db);
    await expect(
      types.copyPets(db, [
        { ownerId: 1, name: "a" },
        { ownerId: 1, name: "b" },
        { ownerId: 3, name: "c" },
      ]),
    ).resolves.toBe(3);
    await expect(
      types.petNames(db, [{ ownerId: 1 }, { ownerId: 3 }, { ownerId: 9 }]),
    ).resolves.toEqual([["a", "b"], ["c"], []]);
    const counted = await types.count(db);
    expect(counted).toEqual({ n: 2, oldest: 36n, one: 1 });
    await close();
  });
});
