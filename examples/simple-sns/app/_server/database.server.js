// @flow

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { fromNodeSqlite } from "@uniflowed/sql/node-sqlite";
import type { SyncQueryable } from "@uniflowed/sql";

import { seedEntry, seedMember, seedReaction } from "./db/query.sql.js";

let raw: null | { close(): void, ... } = null;
let handle: SyncQueryable | null = null;

/**
 * The schema sqlc reads. Beside this module when Node loads the source, and
 * under the project directory when a bundle has replaced `import.meta.url`.
 */

function schemaSql(): string {
  const beside = fileURLToPath(new URL("./db/schema.sql", import.meta.url));
  if (fs.existsSync(beside)) {
    return fs.readFileSync(beside, "utf8");
  }

  return fs.readFileSync(path.resolve("app/_server/db/schema.sql"), "utf8");
}

/**
 * Open the process-local SQLite connection lazily and initialize its schema and fixtures.
 * The database location belongs to the application cwd, not the bundled module path.
 */

export function database(): SyncQueryable {
  if (handle != null) {
    return handle;
  }
  // Bundling changes import.meta.url. Persistence belongs to the application cwd.
  const file = process.env.UF_SIMPLE_SNS_DB ?? path.resolve(".uf", "commonplace.sqlite");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const db = new DatabaseSync(file);
  db.exec("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;");
  db.exec(schemaSql());
  const connection = fromNodeSqlite(db, { begin: "BEGIN IMMEDIATE" });
  seed(connection);
  raw = db;
  handle = connection;

  return connection;
}

/** Release the connection at an explicit application or test lifecycle boundary. */

export function closeDatabase(): void {
  raw?.close();
  raw = null;
  handle = null;
}

/**
 * Keep a SQLite write on this thread from BEGIN IMMEDIATE through COMMIT.
 * A throw rolls the transaction back and is rethrown, so the mutation adapter
 * can still distinguish InputError from a database defect. The body receives
 * the transaction and must not reach back to `database()`.
 */

export function transaction<T>(body: (SyncQueryable) => T): T {
  const run = database().transactionSync;
  if (run == null) {
    throw new Error("the SQLite connection cannot open a synchronous transaction");
  }

  return run(body);
}

function seed(db: SyncQueryable): void {
  // No password: fixture authors cannot be signed into.
  for (const [handle, name, bio] of [
    ["mika", "Mika Tan", "Product engineering"],
    ["ren", "Ren Ito", "Design systems"],
    ["sora", "Sora Lin", "Developer tools"],
    ["niko", "Niko Reyes", "Community"],
  ]) {
    seedMember(db, { id: `seed-${handle}`, name, handle, bio });
  }
  for (const [id, author, body, topic, createdAt] of [
    [
      "field-notes",
      "mika",
      "Removed the company-size question from signup. We never used the answer, and it was the most common place people dropped off.\n\nThe new flow is in staging if anyone has five minutes to try it.",
      "design",
      "2026-09-11T09:40:00.000Z",
    ],
    [
      "quiet-release",
      "ren",
      "Reading view is live. J / K moves between notes, and the text-size setting now carries across devices.\n\nStill fixing a selection bug in Safari. Please send me a recording if you hit it.",
      "release",
      "2026-09-11T08:15:00.000Z",
    ],
    [
      "good-boundaries",
      "sora",
      "Found the slow query. We were loading every message in a workspace just to show the inbox preview. Down from 840 ms to 46 ms after adding the index and limiting the result.\n\nQuery plan is in the engineering notes.",
      "runtime",
      "2026-09-11T07:30:00.000Z",
    ],
    [
      "weekend-reading",
      "niko",
      "Anyone using a split keyboard? Considering a Corne, but six keys per thumb seems like a lot to learn at once. Curious how long the adjustment took.",
      "community",
      "2026-09-10T16:20:00.000Z",
    ],
    [
      "tiny-interactions",
      "ren",
      "Updated the icon set to a 20 px grid. The inbox icon was sitting a pixel too low next to the label. Small change, surprisingly visible at 100% zoom.",
      "design",
      "2026-09-10T13:10:00.000Z",
    ],
    [
      "build-together",
      "mika",
      "Friday demo starts at 15:00. Bring something you are working on; rough versions are fine. I will go first with the new search.",
      "community",
      "2026-09-10T09:00:00.000Z",
    ],
  ]) {
    seedEntry(db, {
      id,
      authorId: `seed-${author}`,
      body,
      topic,
      createdAt,
      requestId: id,
    });
  }
  for (const [entryId, member] of [
    ["field-notes", "ren"],
    ["field-notes", "sora"],
    ["quiet-release", "mika"],
    ["good-boundaries", "ren"],
  ]) {
    seedReaction(db, { entryId, memberId: `seed-${member}` });
  }
}
