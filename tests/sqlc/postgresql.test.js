// @flow
//
// The generated code for sqlc's PostgreSQL examples, and uf's PostgreSQL type
// and options cases, run against a real PostgreSQL behind each adapter:
// PGlite directly, and a `pg` client, a `pg` pool and postgres.js over the
// wire protocol to a PGlite (see `bridge.mjs`). Every scenario runs under all
// four, because the point of the text-format contract is that the answer does
// not depend on the driver.

import { describe, expect, it } from "@uniflowed/test";
import { SqlError } from "@uniflowed/sql";
import * as authors from "./cases/authors-postgresql/gen/query.sql.js";
import * as batch from "./cases/batch-postgresql/gen/query.sql.js";
import * as booktest from "./cases/booktest-postgresql/gen/query.sql.js";
import * as jets from "./cases/jets-postgresql/gen/query-building.sql.js";
import * as city from "./cases/ondeck-postgresql/gen/city.sql.js";
import * as venue from "./cases/ondeck-postgresql/gen/venue.sql.js";
import * as options from "./cases/options-postgresql/gen/query.sql.js";
import * as types from "./cases/types-postgresql/gen/query.sql.js";
import { toExternalId } from "./cases/options-postgresql/types.js";
import { postgresAdapters } from "./databases.js";
import { transactionScenario } from "./transactions.js";

// Each case opens a fresh PostgreSQL (PGlite starts in about a second), and
// the wire-protocol adapters add a round trip per statement through the bridge.
const SLOW = { timeout: 30_000 };

for (const adapter of postgresAdapters) {
  describe(`authors (${adapter.name})`, () => {
    it(
      "creates, reads, lists and deletes",
      async () => {
        const { db, close } = await adapter.open("authors-postgresql");
        const created = await authors.createAuthor(db, {
          name: "Brian Kernighan",
          bio: "Co-author of K&R",
        });
        expect(created).toEqual({ id: 1n, name: "Brian Kernighan", bio: "Co-author of K&R" });
        await authors.createAuthor(db, { name: "Anonymous", bio: null });
        const listed = await authors.listAuthors(db);
        expect(listed.map((author) => author.name)).toEqual(["Anonymous", "Brian Kernighan"]);
        await authors.deleteAuthor(db, { id: 1n });
        await expect(authors.getAuthor(db, { id: 1n })).resolves.toBeNull();
        await close();
      },
      SLOW,
    );
  });

  describe(`transactions (${adapter.name})`, () => {
    it(
      "commits, rolls back, and nests as savepoints",
      async () => {
        const { db, close } = await adapter.open("authors-postgresql");
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

  describe(`booktest (${adapter.name})`, () => {
    it(
      "runs the example's whole scenario",
      async () => {
        const { db, close } = await adapter.open("booktest-postgresql");
        const author = await booktest.createAuthor(db, { name: "Unknown Master" });
        const authorId = author?.authorId ?? 0;
        const available = new Date("2024-01-02T03:04:05.678Z");
        const book = (isbn: string, title: string, year: number, tags: $ReadOnlyArray<string>) =>
          booktest.createBook(db, {
            authorId,
            isbn,
            bookType: "FICTION",
            title,
            year,
            available,
            tags,
          });
        const first = await book("1", "my book title", 2016, []);
        await book("2", "the second book", 2016, ["cool", "unique"]);
        await book("3", "the third book", 2001, ["cool", 'quoted "tag", with a comma']);
        expect(first?.available.getTime()).toBe(available.getTime());
        expect(first?.bookType).toBe("FICTION");
        const bookId = first?.bookId ?? 0;
        await booktest.updateBook(db, { title: "changed", tags: ["cool", "sub"], bookId });
        await booktest.updateBookIsbn(db, {
          title: "renamed",
          tags: ["x"],
          bookId,
          isbn: "NEW ISBN",
        });
        const tagged = await booktest.booksByTags(db, { column1: ["cool"] });
        expect(tagged.map((row) => row.isbn).sort()).toEqual(["2", "3"]);
        expect(tagged.find((row) => row.isbn === "3")?.tags).toEqual([
          "cool",
          'quoted "tag", with a comma',
        ]);
        const byYear = await booktest.booksByTitleYear(db, {
          title: "the second book",
          year: 2016,
        });
        expect(byYear[0].tags).toEqual(["cool", "unique"]);
        await expect(booktest.sayHello(db, { s: "world" })).resolves.toEqual({
          sayHello: "hello world",
        });
        await booktest.deleteBook(db, { bookId });
        await expect(booktest.getBook(db, { bookId })).resolves.toBeNull();
        await close();
      },
      SLOW,
    );
  });

  describe(`ondeck (${adapter.name})`, () => {
    it(
      "round-trips enum arrays and counts",
      async () => {
        const { db, close } = await adapter.open("ondeck-postgresql");
        await city.createCity(db, { name: "San Francisco", slug: "san-francisco" });
        const id = await venue.createVenue(db, {
          slug: "the-fillmore",
          name: "The Fillmore",
          city: "san-francisco",
          spotifyPlaylist: "spotify:uri",
          status: "op!en",
          statuses: ["op!en", "clo@sed"],
          tags: ["rock", "punk"],
        });
        expect(id).toBe(1);
        const got = await venue.getVenue(db, { slug: "the-fillmore", city: "san-francisco" });
        expect(got?.statuses).toEqual(["op!en", "clo@sed"]);
        expect(got?.createdAt).toMatch(/^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d/);
        await expect(venue.venueCountByCity(db)).resolves.toEqual([
          { city: "san-francisco", count: 1n },
        ]);
        await expect(
          venue.updateVenueName(db, { slug: "the-fillmore", name: "West" }),
        ).resolves.toBe(1);
        await venue.deleteVenue(db, { slug: "the-fillmore" });
        await expect(venue.listVenues(db, { city: "san-francisco" })).resolves.toEqual([]);
        await expect(city.listCities(db)).resolves.toEqual([
          { slug: "san-francisco", name: "San Francisco" },
        ]);
        await close();
      },
      SLOW,
    );
  });

  describe(`batch (${adapter.name})`, () => {
    it(
      "runs :batchone, :batchmany and :batchexec in order",
      async () => {
        const { db, close } = await adapter.open("batch-postgresql");
        const author = await batch.createAuthor(db, { name: "Unknown Master" });
        const authorId = author?.authorId ?? 0;
        const available = new Date("2020-02-02T00:00:00Z");
        const books = await batch.createBook(
          db,
          ["1", "2", "3"].map((isbn) => ({
            authorId,
            isbn,
            bookType: "NONFICTION",
            title: `book ${isbn}`,
            year: isbn === "3" ? 2001 : 2016,
            available,
            tags: [isbn],
          })),
        );
        expect(books.map((book) => book?.isbn)).toEqual(["1", "2", "3"]);
        const years = await batch.booksByYear(db, [{ year: 2016 }, { year: 2001 }, { year: 1999 }]);
        expect(years.map((rows) => rows.length)).toEqual([2, 1, 0]);
        await batch.updateBook(db, [
          { title: "changed", tags: ["a"], bookId: books[0]?.bookId ?? 0 },
        ]);
        await batch.deleteBook(db, [{ bookId: books[1]?.bookId ?? 0 }]);
        await expect(
          batch.deleteBookExecResult(db, { bookId: books[2]?.bookId ?? 0 }),
        ).resolves.toEqual({
          rowsAffected: 1,
          lastInsertId: null,
        });
        await expect(batch.getBiography(db, [{ authorId }])).resolves.toEqual([
          { biography: null },
        ]);
        await close();
      },
      SLOW,
    );

    it(
      "rolls the whole batch back when one item fails",
      async () => {
        const { db, close } = await adapter.open("batch-postgresql");
        const author = await batch.createAuthor(db, { name: "A" });
        const authorId = author?.authorId ?? 0;
        const item = (isbn: string): batch.CreateBookArgs => ({
          authorId,
          isbn,
          bookType: "FICTION",
          title: isbn,
          year: 2000,
          available: new Date(0),
          tags: [],
        });
        // The second insert repeats a unique ISBN.
        const failed = await batch
          .createBook(db, [item("x"), item("x")])
          .then(() => false)
          .catch(() => true);
        expect(failed).toBe(true);
        await expect(batch.booksByYear(db, [{ year: 2000 }])).resolves.toEqual([[]]);
        await close();
      },
      SLOW,
    );
  });

  describe(`jets (${adapter.name})`, () => {
    it(
      "counts, lists and deletes",
      async () => {
        const { db, close } = await adapter.open("jets-postgresql");
        await db.query(
          "INSERT INTO pilots (id, name) VALUES (1, 'Amelia'), (2, 'Bessie')",
          [],
          "exec",
        );
        await expect(jets.countPilots(db)).resolves.toBe(2n);
        await jets.deletePilot(db, { id: 2 });
        await expect(jets.listPilots(db)).resolves.toEqual([{ id: 1, name: "Amelia" }]);
        await close();
      },
      SLOW,
    );
  });

  describe(`types (${adapter.name})`, () => {
    it(
      "reads every type the text format carries as its Flow type",
      async () => {
        const { db, close } = await adapter.open("types-postgresql");
        await db.query(
          "INSERT INTO people (name, nick, age, small, score, ratio, active, born, wake, created, local_at, data, raw, tags, grid, feeling, feelings, uid, addr) " +
            "VALUES ('Ada', 'ada', 36, -32768, 12345678.12, 'NaN', false, '1815-12-10', '04:05:06', '2024-01-02 12:04:05.678+09', '2024-01-02 03:04:05.123456', '{\"a\": [1, 2]}', '\\x000102', '{\"x y\",\"a\\\\b\",\"q\\\"\"}', '{{1,2},{3,4}}', 'happy', '{sad,ok}', 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', '192.168.0.1/24')",
          [],
          "exec",
        );
        await db.query("SELECT setval('people_id_seq', 9007199254740992)", [], "rows");
        await db.query("INSERT INTO people (name) VALUES ('Big')", [], "exec");
        const ada = await types.getPerson(db, { id: 1n });
        expect(ada).toMatchObject({
          id: 1n,
          name: "Ada",
          nick: "ada",
          age: 36,
          small: -32768,
          score: "12345678.12",
          active: false,
          born: "1815-12-10",
          wake: "04:05:06",
          localAt: "2024-01-02 03:04:05.123456",
          data: { a: [1, 2] },
          raw: new Uint8Array([0, 1, 2]),
          tags: ["x y", "a\\b", 'q"'],
          grid: [
            [1, 2],
            [3, 4],
          ],
          feeling: "happy",
          feelings: ["sad", "ok"],
          uid: "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
          addr: "192.168.0.1/24",
        });
        expect(ada?.ratio).toBeNaN();
        expect(ada?.created.toISOString()).toBe("2024-01-02T03:04:05.678Z");
        // One past 2^53, exactly.
        const big = await types.listByIds(db, { ids: [9007199254740993n] });
        expect(big).toEqual([{ id: 9007199254740993n, name: "Big" }]);
        await close();
      },
      SLOW,
    );

    it(
      "sends every type the same way back",
      async () => {
        const { db, close } = await adapter.open("types-postgresql");
        const created = await types.createPerson(db, {
          name: "Grace",
          tags: ["", "NULL", "{braces}", 'a "quote"', "back\\slash"],
          feeling: "sad",
        });
        expect(created?.id).toBe(1n);
        const person = await types.getPerson(db, { id: 1n });
        expect(person?.tags).toEqual(["", "NULL", "{braces}", 'a "quote"', "back\\slash"]);
        expect(person?.feeling).toBe("sad");
        expect(person?.nick).toBeNull();
        await expect(types.search(db, { pattern: "gr%", minAge: null })).resolves.toEqual([
          { id: 1n, name: "Grace", feeling: "sad" },
        ]);
        await expect(types.search(db, { pattern: "gr%", minAge: 1 })).resolves.toEqual([]);
        await close();
      },
      SLOW,
    );

    it(
      "runs the exec commands, :copyfrom, sqlc.embed and :batch*",
      async () => {
        const { db, close } = await adapter.open("types-postgresql");
        await types.createPerson(db, { name: "A", tags: [], feeling: "ok" });
        await types.createPerson(db, { name: "B", tags: [], feeling: "ok" });
        await expect(
          types.copyPets(db, [
            { ownerId: 1n, name: "Rex" },
            { ownerId: 2n, name: "Tom" },
          ]),
        ).resolves.toBe(2);
        const embedded = await types.peopleWithPets(db);
        expect(embedded.map((row) => [row.people.name, row.pets.name])).toEqual([
          ["A", "Rex"],
          ["B", "Tom"],
        ]);
        await expect(types.rename(db, { newName: "Z", id: 1n })).resolves.toBe(1);
        await expect(types.rename(db, { newName: "Z", id: 99n })).resolves.toBe(0);
        await types.touch(db);
        await expect(types.batchOne(db, [{ name: "Z" }, { name: "nobody" }])).resolves.toEqual([
          1n,
          null,
        ]);
        await expect(types.batchNames(db, [{ age: null }])).resolves.toEqual([[]]);
        await types.batchDel(db, [{ id: 1 }, { id: 2 }]);
        await expect(types.peopleWithPets(db)).resolves.toEqual([]);
        await expect(types.remove(db, { id: 2n })).resolves.toEqual({
          rowsAffected: 1,
          lastInsertId: null,
        });
        const counted = await types.countPeople(db);
        expect(counted?.total).toBe(1n);
        await expect(types.scalar(db, { id: 1n })).resolves.toBe("Z");
        await close();
      },
      SLOW,
    );

    it(
      "keeps a transaction's work together and a failed one's apart",
      async () => {
        const { db, close } = await adapter.open("types-postgresql");
        const transaction = db.transaction;
        if (transaction === undefined) {
          throw new Error(`${adapter.name} has transactions`);
        }
        await transaction(async (tx) => {
          await types.createPerson(tx, { name: "kept", tags: [], feeling: "ok" });
          const inner = tx.transaction;
          if (inner === undefined) {
            throw new Error("a transaction nests");
          }
          await inner<void>(async (sp) => {
            await types.createPerson(sp, { name: "savepoint", tags: [], feeling: "ok" });
            throw new Error("undo the savepoint only");
          }).catch(() => {});
        });
        await transaction<void>(async (tx) => {
          await types.createPerson(tx, { name: "dropped", tags: [], feeling: "ok" });
          throw new Error("undo it all");
        }).catch(() => {});
        const names = await types.search(db, { pattern: "%", minAge: null });
        expect(names.map((row) => row.name)).toEqual(["kept"]);
        await close();
      },
      SLOW,
    );

    it(
      "names the query and the value when a decoder refuses",
      async () => {
        const { db, close } = await adapter.open("types-postgresql");
        await db.query("INSERT INTO people (name, tags) VALUES ('n', '{a,NULL}')", [], "exec");
        const error = await types
          .getPerson(db, { id: 1n })
          .then(() => null)
          .catch((caught) => caught);
        expect(error).toBeInstanceOf(SqlError);
        expect(String(error?.message)).toMatch(
          /^GetPerson: expected a non-null text array element/,
        );
        await close();
      },
      SLOW,
    );
  });

  describe(`options (${adapter.name})`, () => {
    it(
      "decodes overrides through the application's functions",
      async () => {
        const { db, close } = await adapter.open("options-postgresql");
        const externalId = toExternalId("A0EEBC99-9C0B-4EF8-BB6D-6BB9BD380A11");
        const created = await options.createAccount(db, {
          externalId,
          settings: { theme: "dark" },
          spotifyURL: null,
        });
        expect(created).toMatchObject({
          id: 1,
          externalId: "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11",
          settings: { theme: "dark" },
          spotifyURL: null,
          balance: "0.00",
        });
        expect(created?.createdAt).toMatch(/^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d+[+-]\d\d/);
        await expect(options.findByExternal(db, { externalId })).resolves.toMatchObject({ id: 1 });
        await expect(options.byExternals(db, { ids: [externalId] })).resolves.toEqual([1]);
        await db.query('UPDATE accounts SET settings = \'{"theme": "sepia"}\'', [], "exec");
        const refused = await options
          .getAccount(db, { id: 1 })
          .then(() => null)
          .catch((caught) => String(caught?.message));
        expect(refused).toMatch(/not account settings/);
        await close();
      },
      SLOW,
    );
  });
}
