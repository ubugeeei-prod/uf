// @flow
//
// `@uniflowed/sql/pglite`: PGlite, PostgreSQL compiled to WebAssembly.
//
//   import { PGlite } from "@electric-sql/pglite";
//   import { fromPGlite } from "@uniflowed/sql/pglite";
//
//   const db = fromPGlite(await PGlite.create());
//
// PGlite parses and serialises values itself, by type. This adapter turns both
// off — an identity parser and serialiser for every type it knows — so rows
// arrive as the server's text and parameters leave as the codecs wrote them,
// exactly as with every other PostgreSQL adapter. `:copyfrom` uses PGlite's
// `/dev/blob` device, which is how its `COPY FROM` reads a payload.

import type { Queryable, SqlParam } from "../index.js";
import { singleConnection } from "../index.js";

/** The part of a `PGlite` instance this uses. */
export interface PGliteDatabase {
  query(
    text  : string,
    params: $ReadOnlyArray<SqlParam>,
    options: {|
      readonly rowMode    : "array",
      readonly parsers    : { readonly [string]: (value: string) => mixed },
      readonly serializers: { readonly [string]: (value: mixed) => mixed },
      readonly blob?      : Blob,
    |},
  ): Promise<{ readonly rows: $ReadOnlyArray<mixed>, readonly affectedRows?: number, ... }>;
  readonly parsers    : { readonly [string]: mixed };
  readonly serializers: { readonly [string]: mixed };
}

const identity = (value: mixed) => value;

function identities(table: { readonly [string]: mixed }): {
  readonly [string]: (value: mixed) => mixed,
} {
  const out: { [string]: (value: mixed) => mixed } = {};
  for (const key of Object.keys(table)) {
    out[key] = identity;
  }
  return out;
}

/** A [`Queryable`] over one PGlite database. */
export function fromPGlite(database: PGliteDatabase): Queryable {
  // Rebuilt when PGlite learns a type (an extension, an array type it
  // refreshes), which is rare; every other query reuses the same two tables.
  let known = -1;
  let parsers: { readonly [string]: (value: mixed) => mixed } = {};
  let serializers: { readonly [string]: (value: mixed) => mixed } = {};
  return singleConnection({
    engine   : "postgresql",
    maxParams: 65535,
    copy: async (statement, payload) => {
      // `COPY … FROM STDIN` is the wire form. PGlite reads the payload from
      // the virtual file named here instead of a frontend COPY stream.
      const text = statement.replace(" FROM STDIN", " FROM '/dev/blob'");
      const result = await database.query(text, [], {
        rowMode    : "array",
        parsers    : {},
        serializers: {},
        blob       : new Blob([payload]),
      });
      if (typeof result.affectedRows === "number") {
        return result.affectedRows;
      }
      let rows = 0;
      for (let i = 0; i < payload.length; i += 1) {
        if (payload[i] === "\n") {
          rows += 1;
        }
      }
      return rows;
    },
    run: async (text, params) => {
      const size = Object.keys(database.parsers).length + Object.keys(database.serializers).length;
      if (size !== known) {
        known = size;
        parsers = identities(database.parsers);
        serializers = identities(database.serializers);
      }
      const result = await database.query(text, params, { rowMode: "array", parsers, serializers });
      const rows: Array<$ReadOnlyArray<mixed>> = [];
      for (const row of result.rows) {
        if (Array.isArray(row)) {
          rows.push(row);
        }
      }
      return { rows, rowsAffected: result.affectedRows ?? 0, lastInsertId: null };
    },
  });
}
