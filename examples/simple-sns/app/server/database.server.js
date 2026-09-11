// @flow

import fs from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { ensuring, runSync, sync, trySync } from "@uniflowed/effect";

/** Scalar SQLite row values, converted to explicit DTOs by the repository. */
export type Row = { readonly [string]: string | number | null | void, ... };

type Value = string | number | null;

type Statement = {|
  all: (...values: Array<Value>) => Array<Row>,
  get: (...values: Array<Value>) => Row | void,
  run: (...values: Array<Value>) => mixed,
|};

type Database = {|
  close: () => void,
  exec: (sql: string) => void,
  prepare: (sql: string) => Statement,
|};
const SQLite: Class<Database> = DatabaseSync;
let instance: Database | null = null;

/**
 * Open the process-local SQLite connection lazily and initialize its schema and fixtures.
 * The database location belongs to the application cwd, not the bundled module path.
 */
export function database(): Database {
  if (instance != null) {
    return instance;
  }
  // Bundling changes import.meta.url. Persistence belongs to the application cwd.
  const file = process.env.UF_SIMPLE_SNS_DB ?? path.resolve(".uf", "commonplace.sqlite");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const db = new SQLite(file);
  db.exec("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;");
  db.exec(`
    CREATE TABLE IF NOT EXISTS members (
      id TEXT PRIMARY KEY, name TEXT NOT NULL, handle TEXT NOT NULL UNIQUE,
      bio TEXT NOT NULL DEFAULT '', email TEXT NOT NULL DEFAULT '',
      password_hash TEXT
    );
    CREATE TABLE IF NOT EXISTS entries (
      id TEXT PRIMARY KEY, author_id TEXT NOT NULL REFERENCES members(id),
      body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 500),
      topic TEXT NOT NULL CHECK(topic IN ('design','release','runtime','community')),
      created_at TEXT NOT NULL, request_id TEXT NOT NULL, UNIQUE(author_id, request_id)
    );
    CREATE INDEX IF NOT EXISTS entries_newest ON entries(created_at DESC, id DESC);
    CREATE TABLE IF NOT EXISTS reactions (
      entry_id TEXT NOT NULL REFERENCES entries(id), member_id TEXT NOT NULL REFERENCES members(id),
      PRIMARY KEY(entry_id, member_id)
    );
    CREATE TABLE IF NOT EXISTS conversations (id TEXT PRIMARY KEY);
    CREATE TABLE IF NOT EXISTS participants (
      conversation_id TEXT NOT NULL REFERENCES conversations(id), member_id TEXT NOT NULL REFERENCES members(id),
      PRIMARY KEY(conversation_id, member_id)
    );
    CREATE TABLE IF NOT EXISTS notes (
      id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id),
      author_id TEXT NOT NULL REFERENCES members(id), body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 2000),
      created_at TEXT NOT NULL, request_id TEXT NOT NULL, UNIQUE(author_id, request_id)
    );
    CREATE INDEX IF NOT EXISTS notes_conversation ON notes(conversation_id, created_at, id);
    CREATE TABLE IF NOT EXISTS sessions (token_hash TEXT PRIMARY KEY, member_id TEXT NOT NULL REFERENCES members(id), expires_at INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS auth_attempts (handle TEXT PRIMARY KEY, attempts INTEGER NOT NULL, resets_at INTEGER NOT NULL);
  `);
  seed(db);
  instance = db;

  return db;
}

/** Release the connection at an explicit application or test lifecycle boundary. */
export function closeDatabase(): void {
  instance?.close();
  instance = null;
}

/**
 * Keep a SQLite write synchronous from BEGIN through COMMIT. Effect guarantees
 * rollback on a failed body or commit; the original exception is rethrown so
 * the mutation adapter can still distinguish InputError from a database defect.
 * Callbacks must not return promises or open nested transactions.
 */
export function transaction<T>(body: (Database) => T): T {
  const db = database();
  let open = false;

  return runSync(
    ensuring(
      trySync({
        try: () => {
          db.exec("BEGIN IMMEDIATE");
          open = true;

          const value = body(db);
          db.exec("COMMIT");
          open = false;

          return value;
        },
        catch: (error) => error,
      }),
      () =>
        sync(() => {
          if (open) {
            db.exec("ROLLBACK");
          }
        }),
    ),
  );
}

function seed(db: Database): void {
  // No password: fixture authors cannot be signed into.
  for (const [handle, name, bio] of [
    ["mika", "Mika Tan", "Product engineering"],
    ["ren", "Ren Ito", "Design systems"],
    ["sora", "Sora Lin", "Developer tools"],
    ["niko", "Niko Reyes", "Community"],
  ])
    db.prepare("INSERT OR IGNORE INTO members (id, name, handle, bio) VALUES (?, ?, ?, ?)").run(
      `seed-${handle}`,
      name,
      handle,
      bio,
    );
  for (const [id, author, body, topic, date] of [
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
  ])
    db.prepare("INSERT OR IGNORE INTO entries VALUES (?, ?, ?, ?, ?, ?)").run(
      id,
      `seed-${author}`,
      body,
      topic,
      date,
      id,
    );
  for (const [post, member] of [
    ["field-notes", "ren"],
    ["field-notes", "sora"],
    ["quiet-release", "mika"],
    ["good-boundaries", "ren"],
  ])
    db.prepare("INSERT OR IGNORE INTO reactions VALUES (?, ?)").run(post, `seed-${member}`);
}
