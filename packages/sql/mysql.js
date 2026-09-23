// @flow
//
// `@uniflowed/sql/mysql`: MySQL's text representation, both ways.
//
// The `mysql2` adapter reads every column as the string MySQL sent (binary
// types as bytes) and sends parameters as strings, which MySQL converts to the
// column's type — the conversion it applies to any literal, and the one that
// keeps a `BIGINT UNSIGNED` or a `DECIMAL(65,30)` exact on the way in and out.
//
// `DATETIME` and `TIMESTAMP` stay strings. A `TIMESTAMP` is stored in UTC and
// printed in the session's time zone, so the wall-clock text is the only
// representation that does not depend on a setting of a connection the
// generated code never sees.

import type { JsonValue } from "./index.js";
import { decodeError, encodeError } from "./index.js";
import {
  bigintFrom,
  bytesFrom,
  bytesParam,
  enumName,
  integerFromText,
  jsonFromText,
  jsonParam,
  label,
  labelParam,
  stringFrom,
  stringParam,
} from "./internal/shared.js";

/** What a MySQL parameter is sent as. */
export type MysqlParam = null | string | Uint8Array;

/** How one MySQL type becomes a Flow value and back. */
export type Codec<T> = {|
  readonly sqlType: string,
  readonly decode: (value: mixed) => T,
  readonly encode: (value: T) => MysqlParam,
|};

/** `TINYINT` through `INT`, signed or unsigned, and `YEAR`: all fit a `number`. */
export function integer(sqlType: string): Codec<number> {
  return {
    sqlType,
    decode: (value) => integerFromText(sqlType, value),
    encode: (value) => {
      if (!Number.isSafeInteger(value)) {
        return encodeError(`${sqlType} (an integer)`, value);
      }
      return String(value);
    },
  };
}

/** `BIGINT`, signed or unsigned, exactly. */
export const bigint: Codec<bigint> = {
  sqlType: "bigint",
  decode: (value) => bigintFrom("bigint", value),
  encode: (value) => {
    if (typeof value !== "bigint") {
      return encodeError("bigint (a bigint)", value);
    }
    return String(value);
  },
};

/** `BIGINT` as a `number`, for `int8: "number"`; throws past 2^53. */
export const bigintAsNumber: Codec<number> = integer("bigint");

/** `BIGINT` as its digits, for `int8: "string"`. */
export const bigintAsString: Codec<string> = {
  sqlType: "bigint",
  decode: (value) => {
    if (typeof value === "string" && /^[-+]?\d+$/.test(value)) {
      return value;
    }
    return decodeError("bigint", value);
  },
  encode: (value) => stringParam("bigint (a string of digits)", value),
};

/** `TINYINT(1)` and `BOOL`, which MySQL spells `1` and `0`. */
export const boolean: Codec<boolean> = {
  sqlType: "tinyint(1)",
  decode: (value) => {
    if (value === "0" || value === 0) {
      return false;
    }
    if (typeof value === "string" && /^-?\d+$/.test(value)) {
      // MySQL's own truth test: any non-zero `TINYINT(1)` is true.
      return true;
    }
    if (typeof value === "number" && Number.isInteger(value)) {
      return true;
    }
    return decodeError("tinyint(1)", value);
  },
  encode: (value) => {
    if (typeof value !== "boolean") {
      return encodeError("tinyint(1) (a boolean)", value);
    }
    return value ? "1" : "0";
  },
};

const DECIMAL = /^[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?$/;

/** `DECIMAL`/`NUMERIC` as its exact decimal string. */
export const decimal: Codec<string> = {
  sqlType: "decimal",
  decode: (value) => {
    if (typeof value === "string" && DECIMAL.test(value)) {
      return value;
    }
    return decodeError("decimal", value);
  },
  encode: (value) => {
    if (typeof value !== "string" || !DECIMAL.test(value)) {
      return encodeError("decimal (a decimal string)", value);
    }
    return value;
  },
};

/** `FLOAT`, `DOUBLE`, and `DECIMAL` for `numeric: "number"`. */
export function float(sqlType: string): Codec<number> {
  return {
    sqlType,
    decode: (value) => {
      if (typeof value === "number") {
        return value;
      }
      if (typeof value === "string" && DECIMAL.test(value)) {
        return Number(value);
      }
      return decodeError(sqlType, value);
    },
    encode: (value) => {
      if (typeof value !== "number" || !Number.isFinite(value)) {
        // MySQL has no NaN or infinity.
        return encodeError(`${sqlType} (a finite number)`, value);
      }
      return String(value);
    },
  };
}

/** Character types, `DATE`, `DATETIME`, `TIMESTAMP`, `TIME`, `SET`. */
export function text(sqlType: string): Codec<string> {
  return {
    sqlType,
    decode: (value) => stringFrom(sqlType, value),
    encode: (value) => stringParam(`${sqlType} (a string)`, value),
  };
}

/** The character types. */
export const string: Codec<string> = text("varchar");

/** `BLOB`, `BINARY`, `VARBINARY`, `BIT`. */
export function bytes(sqlType: string): Codec<Uint8Array> {
  return {
    sqlType,
    decode: (value) => bytesFrom(sqlType, value),
    encode: (value) => bytesParam(`${sqlType} (a Uint8Array)`, value),
  };
}

/** `JSON`. */
export const json: Codec<JsonValue> = {
  sqlType: "json",
  decode: (value) => jsonFromText(value),
  encode: (value) => jsonParam(value),
};

/** A result sqlc could not type. */
export const unknown: Codec<mixed> = {
  sqlType: "unknown",
  decode: (value) => value,
  encode: (value) => {
    if (value === null || typeof value === "string" || value instanceof Uint8Array) {
      return value;
    }
    if (typeof value === "number" || typeof value === "bigint") {
      return String(value);
    }
    if (typeof value === "boolean") {
      return value ? "1" : "0";
    }
    return jsonParam(value);
  },
};

/** An `ENUM(…)` column, from its values in declaration order. */
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
export function param<T>(codec: Codec<T>, value: T | null): MysqlParam {
  return value === null ? null : codec.encode(value);
}
