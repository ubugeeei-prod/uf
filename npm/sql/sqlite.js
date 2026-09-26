// @flow
//
// `@uniflowed/sql/sqlite`: SQLite's storage classes, both ways.
//
// SQLite has five: NULL, INTEGER, REAL, TEXT and BLOB. Every SQLite adapter
// turns safe integers on, so an INTEGER arrives as a `bigint` (or, from a
// driver that cannot, a `number`) and nothing past 2^53 is rounded before a
// codec sees it. A declared column type is only an affinity in SQLite, so a
// codec accepts the storage classes that affinity can produce and refuses the
// rest — a `TEXT` in an `INTEGER` column is an error here, not a string.

import type { JsonValue, SqlParam } from "./index.js";
import { decodeError, encodeError } from "./index.js";
import {
  bigintFrom,
  bigintParam,
  bytesFrom,
  bytesParam,
  enumName,
  integerParam,
  jsonFromText,
  jsonParam,
  label,
  labelParam,
  numberParam,
  safeInteger,
  stringFrom,
  stringParam,
} from "./internal/shared.js";

/** How one declared type becomes a Flow value and back. */
export type Codec<T> = {|
  readonly sqlType: string,
  readonly decode: (value: mixed) => T,
  readonly encode: (value: T) => SqlParam,
|};

/** `INTEGER` as a `number`, which throws past 2^53 instead of rounding. */
export const integer: Codec<number> = {
  sqlType: "INTEGER",
  decode: (value) => {
    if (typeof value === "number" || typeof value === "bigint") {
      return safeInteger("INTEGER", value);
    }
    return decodeError("INTEGER", value);
  },
  encode: (value) => integerParam("INTEGER (an integer within ±2^53)", value),
};

/** `INTEGER` as a `bigint`, for `sqliteInteger: "bigint"`. */
export const integerAsBigint: Codec<bigint> = {
  sqlType: "INTEGER",
  decode: (value) => bigintFrom("INTEGER", value),
  encode: (value) => bigintParam("INTEGER (a bigint)", value),
};

/** `REAL`, `DOUBLE`, `FLOAT`, and `NUMERIC`/`DECIMAL`, which SQLite stores as one or an INTEGER. */
export function real(sqlType: string): Codec<number> {
  return {
    sqlType,
    decode: (value) => {
      if (typeof value === "number") {
        return value;
      }
      if (typeof value === "bigint") {
        return safeInteger(sqlType, value);
      }
      return decodeError(sqlType, value);
    },
    encode: (value) => numberParam(`${sqlType} (a number)`, value),
  };
}

/** `BOOLEAN`: SQLite stores 0 and 1. */
export const boolean: Codec<boolean> = {
  sqlType: "BOOLEAN",
  decode: (value) => {
    if (value === 0 || value === 0n) {
      return false;
    }
    if (value === 1 || value === 1n) {
      return true;
    }
    return decodeError("BOOLEAN (0 or 1)", value);
  },
  encode: (value) => {
    if (typeof value !== "boolean") {
      return encodeError("BOOLEAN (a boolean)", value);
    }
    return value ? 1 : 0;
  },
};

/** `TEXT` and the types SQLite stores as text: `VARCHAR`, `DATE`, `DATETIME`, `TIMESTAMP`. */
export function text(sqlType: string): Codec<string> {
  return {
    sqlType,
    decode: (value) => stringFrom(sqlType, value),
    encode: (value) => stringParam(`${sqlType} (a string)`, value),
  };
}

/** `TEXT`. */
export const string: Codec<string> = text("TEXT");

/** `BLOB`. */
export const blob: Codec<Uint8Array> = {
  sqlType: "BLOB",
  decode: (value) => bytesFrom("BLOB", value),
  encode: (value) => bytesParam("BLOB (a Uint8Array)", value),
};

/** `JSON` and `JSONB` (sqlc reads the latter through `json()`), stored as text. */
export const json: Codec<JsonValue> = {
  sqlType: "JSON",
  decode: (value) => jsonFromText(value),
  encode: (value) => jsonParam(value),
};

/** A declared type with no affinity uf can type, or a result sqlc could not. */
export const unknown: Codec<mixed> = {
  sqlType: "unknown",
  decode: (value) => value,
  encode: (value) => {
    if (
      value === null ||
      typeof value === "string" ||
      typeof value === "number" ||
      typeof value === "bigint" ||
      value instanceof Uint8Array
    ) {
      return value;
    }
    if (typeof value === "boolean") {
      return value ? 1 : 0;
    }
    return jsonParam(value);
  },
};

/** A `CHECK (x IN (…))`-style enum sqlc was told about through an override. */
export function enumeration<T extends string>(
  sqlType: string,
  values: $ReadOnlyArray<T>,
): Codec<T> {
  const expected = `${sqlType} (${enumName(values)})`;
  return {
    sqlType,
    decode: (value) => label(values, expected, value),
    encode: (value) => labelParam(values, expected, value),
  };
}

/**
 * An override's codec: `decode` narrows what `codec` read, `encode` widens a
 * value back to what `codec` writes.
 */
export function map<T, U>(
  codec: Codec<T>,
  decode: (value: T) => U,
  encode: (value: U) => T,
): Codec<U> {
  return {
    sqlType: codec.sqlType,
    decode: (value) => decode(codec.decode(value)),
    encode: (value) => codec.encode(encode(value)),
  };
}

/** Decode a nullable column. */
export function nullable<T>(codec: Codec<T>, value: mixed): T | null {
  return value === null ? null : codec.decode(value);
}

/** Encode a nullable parameter. */
export function param<T>(codec: Codec<T>, value: T | null): SqlParam {
  return value === null ? null : codec.encode(value);
}
