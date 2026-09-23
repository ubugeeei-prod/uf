// @flow
//
// Open each case's schema in a real database, behind each adapter.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { Queryable } from "@uniflowed/sql";
import { fromBunSqlite } from "@uniflowed/sql/bun-sqlite";
import { fromNodeSqlite } from "@uniflowed/sql/node-sqlite";
import { fromPGlite } from "@uniflowed/sql/pglite";
import { fromPgClient, fromPgPool } from "@uniflowed/sql/pg";
import { fromPostgres } from "@uniflowed/sql/postgres";
import { PGlite } from "@electric-sql/pglite";
import pg from "pg";
import postgres from "postgres";
import { sockets } from "./bridge.mjs";

const cases = path.join(path.dirname(fileURLToPath(String(import.meta.url))), "cases");

/** A case's schema: `schema.sql`, or every file under `schema/` in name order. */
export function schemaOf(name: string): string {
  const file = path.join(cases, name, "schema.sql");
  if (fs.existsSync(file)) {
    return fs.readFileSync(file, "utf8");
  }
  const dir = path.join(cases, name, "schema");
  return (
    fs
      .readdirSync(dir)
      .filter((entry) => entry.endsWith(".sql"))
      .sort()
      .map((entry) => fs.readFileSync(path.join(dir, entry), "utf8"))
      // Migration files, one per statement group; not every one ends in `;`.
      .join(";\n")
  );
}

/** An open database, and how to close it. */
export type Open = {| readonly db: Queryable, readonly close: () => Promise<void> |};

/** Which SQLite adapter [`openSqlite`] uses on this host. */
export const sqliteAdapter: "bun:sqlite" | "node:sqlite" =
  process.versions.bun === undefined ? "node:sqlite" : "bun:sqlite";

/**
 * An in-memory SQLite with the case's schema applied, through the host's own
 * driver: `bun:sqlite` under Bun (`uf test --host bun`, which CI runs), and
 * `node:sqlite` everywhere else.
 */
export async function openSqlite(name: string): Promise<Open> {
  if (sqliteAdapter === "bun:sqlite") {
    const { Database } = await import("bun:sqlite");
    const database = new Database(":memory:");
    database.exec(schemaOf(name));
    return { db: fromBunSqlite(database), close: async () => database.close() };
  }
  const { DatabaseSync } = await import("node:sqlite");
  const database = new DatabaseSync(":memory:");
  database.exec(schemaOf(name));
  return { db: fromNodeSqlite(database), close: async () => database.close() };
}

/** The PostgreSQL adapters, each over its own fresh PGlite. */
export const postgresAdapters: $ReadOnlyArray<{|
  readonly name: string,
  readonly open: (caseName: string) => Promise<Open>,
|}> = [
  {
    name: "PGlite",
    open: async (caseName) => {
      const database = await PGlite.create();
      await database.exec(schemaOf(caseName));
      return { db: fromPGlite(database), close: () => database.close() };
    },
  },
  {
    name: "pg",
    open: async (caseName) => {
      const database = await PGlite.create();
      await database.exec(schemaOf(caseName));
      const stream = sockets(database);
      const client = new pg.Client({ stream, user: "postgres", database: "postgres" });
      await client.connect();
      return {
        db: fromPgClient(client),
        close: async () => {
          // `end()` waits for a socket `close` the duplex pair never sends.
          client.on("error", () => {});
          client.connection.stream.destroy();
          await stream.settled();
          await database.close();
        },
      };
    },
  },
  {
    // One connection, because PGlite is one session: a second pooled
    // connection would share its transaction rather than have its own.
    name: "pg pool",
    open: async (caseName) => {
      const database = await PGlite.create();
      await database.exec(schemaOf(caseName));
      const socket = sockets(database);
      const opened = [];
      const stream = () => {
        const connection = socket();
        opened.push(connection);
        return connection;
      };
      const pool = new pg.Pool({ stream, user: "postgres", database: "postgres", max: 1 });
      pool.on("error", () => {});
      return {
        db: fromPgPool(pool),
        close: async () => {
          // `end()` waits for a socket `close` the duplex pair never sends,
          // as with the client above.
          for (const connection of opened) {
            connection.destroy();
          }
          await socket.settled();
          await database.close();
        },
      };
    },
  },
  {
    name: "postgres",
    open: async (caseName) => {
      const database = await PGlite.create();
      await database.exec(schemaOf(caseName));
      const socket = sockets(database);
      const sql = postgres({ socket, user: "postgres", database: "postgres", max: 1 });
      return {
        db: fromPostgres(sql),
        close: async () => {
          await sql.end({ timeout: 0 });
          await socket.settled();
          await database.close();
        },
      };
    },
  },
];
