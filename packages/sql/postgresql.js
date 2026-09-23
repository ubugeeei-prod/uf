// @flow
//
// `@uniflowed/sql/postgresql`: PostgreSQL's text format, both ways.
//
// Every PostgreSQL adapter returns column values exactly as the server printed
// them — `pg` with its type parsers switched off, `postgres` through `.raw()`,
// PGlite with identity parsers — and sends parameters as text with no declared
// type, so the server infers each one from the statement. That makes this
// module the single place a value's representation is decided, and the same
// for every driver.
//
// The text format assumes the server's defaults for `DateStyle` (ISO) and
// `bytea_output` (hex, though escape is read too). `IntervalStyle` does not
// matter: an interval stays a string.

import type { JsonValue } from "./index.js";
import { decodeError, encodeError } from "./index.js";
import {
  bigintFrom,
  enumName,
  integerFromText,
  jsonFromText,
  jsonParam,
  label,
  labelParam,
  stringParam,
} from "./internal/shared.js";

/** A node of a parsed array literal: an element's text, `NULL`, or a sub-array. */
export type ArrayNode = string | null | $ReadOnlyArray<ArrayNode>;

/**
 * How one SQL type becomes a Flow value and back.
 *
 * `decode` reads a column; `encode` writes a parameter. `element` and
 * `literal` are the same two directions for a value inside an array literal,
 * which is how [`array`] nests without re-parsing.
 */
export type Codec<T> = {|
  /** For messages: `int8`, `text[]`. */
  readonly sqlType: string,
  readonly decode: (value: mixed) => T,
  readonly encode: (value: T) => string,
  readonly element: (node: ArrayNode) => T,
  /** The value as it appears inside `{…}`: quoted, or a nested literal. */
  readonly literal: (value: T) => string,
  /** How many array dimensions this codec reads; 0 for a scalar. */
  readonly depth: number,
|};

function quote(text: string): string {
  return `"${text.replace(/[\\"]/g, "\\$&")}"`;
}

/** A scalar codec from its two text conversions. */
export function scalar<T>(
  sqlType: string,
  fromText: (text: string) => T,
  toText: (value: T) => string,
): Codec<T> {
  const decode = (value: mixed): T => {
    if (typeof value !== "string") {
      return decodeError(sqlType, value);
    }
    return fromText(value);
  };
  return {
    sqlType,
    decode,
    encode: toText,
    element: (node) => {
      if (node === null) {
        return decodeError(`a non-null ${sqlType} array element`, node);
      }
      if (typeof node !== "string") {
        return decodeError(`${sqlType}, not a nested array`, node);
      }
      return fromText(node);
    },
    literal: (value) => quote(toText(value)),
    depth: 0,
  };
}

// ---------------------------------------------------------------------------
// Numbers.

const INT_RANGE = {
  int2: [-32768, 32767],
  int4: [-2147483648, 2147483647],
};

function boundedInteger(sqlType: "int2" | "int4"): Codec<number> {
  const [min, max] = INT_RANGE[sqlType];
  return scalar(
    sqlType,
    (text) => integerFromText(sqlType, text),
    (value) => {
      if (!Number.isInteger(value) || value < min || value > max) {
        return encodeError(`${sqlType} (an integer from ${min} to ${max})`, value);
      }
      return String(value);
    },
  );
}

/** `smallint`, `smallserial`. */
export const int2: Codec<number> = boundedInteger("int2");

/** `integer`, `serial`, `oid` and the other 32-bit integers. */
export const int4: Codec<number> = boundedInteger("int4");

/** `oid`, `xid`, `cid` and the `reg*` types: unsigned 32-bit. */
export const oid: Codec<number> = scalar(
  "oid",
  (text) => integerFromText("oid", text),
  (value) => {
    if (!Number.isInteger(value) || value < 0 || value > 4294967295) {
      return encodeError("oid (an integer from 0 to 4294967295)", value);
    }
    return String(value);
  },
);

/** `bigint`, `bigserial`: every value, exactly. */
export const int8: Codec<bigint> = scalar(
  "int8",
  (text) => bigintFrom("int8", text),
  (value) => {
    if (typeof value !== "bigint") {
      return encodeError("int8 (a bigint)", value);
    }
    return String(value);
  },
);

/** `bigint` as a `number`, for a table that opted in with `int8: "number"`; throws past 2^53. */
export const int8AsNumber: Codec<number> = scalar(
  "int8",
  (text) => integerFromText("int8", text),
  (value) => {
    if (!Number.isSafeInteger(value)) {
      return encodeError("int8 (an integer within ±2^53)", value);
    }
    return String(value);
  },
);

const DIGITS = /^[-+]?\d+$/;

/** `bigint` as its digits, for `int8: "string"`. */
export const int8AsString: Codec<string> = scalar(
  "int8",
  (text) => text,
  (value) => {
    if (typeof value !== "string" || !DIGITS.test(value)) {
      return encodeError("int8 (a string of digits)", value);
    }
    return value;
  },
);

const FLOAT = /^(?:[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?|NaN|-?Infinity)$/;

function floatCodec(sqlType: string): Codec<number> {
  return scalar(
    sqlType,
    (text) => {
      if (!FLOAT.test(text)) {
        return decodeError(sqlType, text);
      }
      return Number(text);
    },
    (value) => {
      if (typeof value !== "number") {
        return encodeError(`${sqlType} (a number)`, value);
      }
      return String(value);
    },
  );
}

/** `real`. */
export const float4: Codec<number> = floatCodec("float4");

/** `double precision`. */
export const float8: Codec<number> = floatCodec("float8");

const NUMERIC = /^(?:[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?|NaN|-?Infinity)$/;

/** `numeric`/`decimal` as its exact decimal string. */
export const numeric: Codec<string> = scalar(
  "numeric",
  (text) => text,
  (value) => {
    if (typeof value !== "string" || !NUMERIC.test(value)) {
      return encodeError("numeric (a decimal string)", value);
    }
    return value;
  },
);

/** `numeric` as a `number`, for `numeric: "number"`. Rounds; that is what opting in means. */
export const numericAsNumber: Codec<number> = floatCodec("numeric");

/** `money`, in the server's `lc_monetary` format (`$1,234.50`), unchanged. */
export const money: Codec<string> = scalar(
  "money",
  (text) => text,
  (value) => stringParam("money (a string)", value),
);

// ---------------------------------------------------------------------------
// Everything else scalar.

/** `boolean`. */
export const bool: Codec<boolean> = scalar(
  "bool",
  (text) => {
    if (text === "t") {
      return true;
    }
    if (text === "f") {
      return false;
    }
    return decodeError("bool", text);
  },
  (value) => {
    if (typeof value !== "boolean") {
      return encodeError("bool (a boolean)", value);
    }
    return value ? "t" : "f";
  },
);

/**
 * Any type whose text is the value: `text`, `varchar`, `uuid`, `inet`,
 * `interval`, `time`, ranges, and types uf has no better representation for.
 */
export function text(sqlType: string): Codec<string> {
  return scalar(
    sqlType,
    (value) => value,
    (value) => stringParam(`${sqlType} (a string)`, value),
  );
}

/** `text` and the character types. */
export const string: Codec<string> = text("text");

const HEX = /^[0-9a-fA-F]*$/;

function bytesFromText(value: string): Uint8Array {
  if (value.startsWith("\\x")) {
    const hex = value.slice(2);
    if (hex.length % 2 !== 0 || !HEX.test(hex)) {
      return decodeError("bytea in hex format", value);
    }
    const out = new Uint8Array(hex.length / 2);
    for (let i = 0; i < out.length; i += 1) {
      out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return out;
  }
  // `bytea_output = escape`: printable bytes as themselves, a backslash as two,
  // and everything else as a backslash and three octal digits.
  const out: Array<number> = [];
  for (let i = 0; i < value.length; i += 1) {
    const code = value.charCodeAt(i);
    if (code === 92) {
      if (value[i + 1] === "\\") {
        out.push(92);
        i += 1;
        continue;
      }
      const octal = value.slice(i + 1, i + 4);
      if (!/^[0-3][0-7][0-7]$/.test(octal)) {
        return decodeError("bytea in escape format", value);
      }
      out.push(parseInt(octal, 8));
      i += 3;
      continue;
    }
    if (code > 255) {
      return decodeError("bytea", value);
    }
    out.push(code);
  }
  return Uint8Array.from(out);
}

function bytesToText(value: Uint8Array): string {
  if (!(value instanceof Uint8Array)) {
    return encodeError("bytea (a Uint8Array)", value);
  }
  let hex = "\\x";
  for (let i = 0; i < value.length; i += 1) {
    hex += value[i].toString(16).padStart(2, "0");
  }
  return hex;
}

/** `bytea`. */
export const bytea: Codec<Uint8Array> = scalar("bytea", bytesFromText, bytesToText);

/** `json` and `jsonb`. */
export const json: Codec<JsonValue> = scalar(
  "json",
  (value) => jsonFromText(value),
  (value) => jsonParam(value),
);

/** `date`: `YYYY-MM-DD`, or `infinity`. A calendar date has no time zone, so it is not a `Date`. */
export const date: Codec<string> = text("date");

/** `timestamp without time zone`: the wall-clock time as the server printed it. */
export const timestamp: Codec<string> = text("timestamp");

/** `timestamptz` as the server printed it, microseconds and all, for `timestamptz: "string"`. */
export const timestamptzAsString: Codec<string> = text("timestamptz");

const TIMESTAMPTZ =
  /^(\d{4,})-(\d\d)-(\d\d) (\d\d):(\d\d):(\d\d)(?:\.(\d{1,6}))?([+-])(\d\d)(?::?(\d\d))?(?::?(\d\d))?( BC)?$/;

function instantFromText(text: string): Date {
  const parts = TIMESTAMPTZ.exec(text);
  if (parts === null) {
    return decodeError(
      text === "infinity" || text === "-infinity"
        ? 'a finite timestamptz (use timestamptz: "string" to read infinity)'
        : "timestamptz in ISO DateStyle",
      text,
    );
  }
  const year = Number(parts[1]);
  const fraction = parts[7] ?? "";
  const millis = fraction === "" ? 0 : Number(fraction.padEnd(3, "0").slice(0, 3));
  const sign = parts[8] === "-" ? -1 : 1;
  const offsetSeconds =
    sign * (Number(parts[9]) * 3600 + Number(parts[10] ?? "0") * 60 + Number(parts[11] ?? "0"));
  const instant = new Date(0);
  // `setUTCFullYear` rather than `Date.UTC`, which reads years 0 to 99 as
  // 1900 to 1999. Year 1 BC is year 0 in ISO 8601.
  instant.setUTCFullYear(
    parts[12] === undefined ? year : 1 - year,
    Number(parts[2]) - 1,
    Number(parts[3]),
  );
  instant.setUTCHours(Number(parts[4]), Number(parts[5]), Number(parts[6]), millis);
  const time = instant.getTime() - offsetSeconds * 1000;
  if (!Number.isFinite(time)) {
    return decodeError("timestamptz within the range of a Date", text);
  }
  return new Date(time);
}

function instantToText(value: Date): string {
  if (!(value instanceof Date) || Number.isNaN(value.getTime())) {
    return encodeError("timestamptz (a valid Date)", value);
  }
  return value.toISOString();
}

/** `timestamp with time zone`: an instant. Millisecond precision, which is a `Date`'s. */
export const timestamptz: Codec<Date> = scalar("timestamptz", instantFromText, instantToText);

/** A value sqlc could not type (`any`, `anyelement`, …): whatever the server printed. */
export const unknown: Codec<mixed> = {
  sqlType: "unknown",
  decode: (value) => value,
  encode: (value) => {
    if (typeof value === "string") {
      return value;
    }
    if (typeof value === "number" || typeof value === "bigint" || typeof value === "boolean") {
      return String(value);
    }
    if (value instanceof Date) {
      return instantToText(value);
    }
    return jsonParam(value);
  },
  element: (node) => node,
  literal: (value) => (value === null ? "NULL" : quote(String(value))),
  depth: 0,
};

// ---------------------------------------------------------------------------
// Enums and arrays.

/** A codec for a `CREATE TYPE … AS ENUM`, from its labels in declaration order. */
export function enumeration<T extends string>(
  sqlType: string,
  values: $ReadOnlyArray<T>,
): Codec<T> {
  const expected = `${sqlType} (${enumName(values)})`;
  return scalar(
    sqlType,
    (value) => label(values, expected, value),
    (value) => labelParam(values, expected, value),
  );
}

/**
 * Parse an array literal: `{1,2,NULL}`, `{{a,b},{c,d}}`, `{"a b","x\"y"}`.
 *
 * A literal with non-default bounds starts with a dimension decoration
 * (`[0:2]={…}`); the bounds are not part of the value and are dropped.
 */
export function parseArray(literal: string): $ReadOnlyArray<ArrayNode> {
  let at = 0;
  if (literal.startsWith("[")) {
    const equals = literal.indexOf("=");
    if (equals < 0) {
      return decodeError("an array literal", literal);
    }
    at = equals + 1;
  }
  const fail = (): empty => decodeError("an array literal", literal);
  const parseLevel = (): $ReadOnlyArray<ArrayNode> => {
    if (literal.charAt(at) !== "{") {
      return fail();
    }
    at += 1;
    const items: Array<ArrayNode> = [];
    if (literal.charAt(at) === "}") {
      at += 1;
      return items;
    }
    for (;;) {
      while (literal.charAt(at) === " ") {
        at += 1;
      }
      const head = literal.charAt(at);
      if (head === "{") {
        items.push(parseLevel());
      } else if (head === '"') {
        at += 1;
        let value = "";
        for (;;) {
          const ch = literal.charAt(at);
          if (at >= literal.length) {
            return fail();
          }
          if (ch === "\\") {
            value += literal.charAt(at + 1);
            at += 2;
          } else if (ch === '"') {
            at += 1;
            break;
          } else {
            value += ch;
            at += 1;
          }
        }
        items.push(value);
      } else {
        const start = at;
        while (at < literal.length && literal.charAt(at) !== "," && literal.charAt(at) !== "}") {
          at += 1;
        }
        const raw = literal.slice(start, at).trim();
        if (raw === "") {
          return fail();
        }
        items.push(raw.toUpperCase() === "NULL" ? null : raw);
      }
      while (literal.charAt(at) === " ") {
        at += 1;
      }
      if (literal.charAt(at) === ",") {
        at += 1;
      } else if (literal.charAt(at) === "}") {
        at += 1;
        return items;
      } else {
        return fail();
      }
    }
    return fail();
  };
  const root = parseLevel();
  if (at !== literal.length) {
    return fail();
  }
  return root;
}

/**
 * An array of `element`. Nest it for more dimensions: `array(array(int4))` is
 * `int4[][]`.
 *
 * A `NULL` inside the array throws. sqlc does not tell a plugin whether a
 * column's elements may be null, and typing them as `T | null` everywhere would
 * make every `text[] NOT NULL` of tags awkward to use to guard against a case
 * most schemas never produce; typing them `T` and returning `null` would be a
 * lie. So the type is `T`, and the lie is refused at the boundary.
 */
export function array<T>(element: Codec<T>): Codec<$ReadOnlyArray<T>> {
  const sqlType = `${element.sqlType}[]`;
  const fromNode = (node: ArrayNode): $ReadOnlyArray<T> => {
    if (node === null || typeof node === "string") {
      return decodeError(`${sqlType} with ${element.depth + 1} dimension(s)`, node);
    }
    return node.map(element.element);
  };
  const toLiteral = (value: $ReadOnlyArray<T>): string => {
    if (!Array.isArray(value)) {
      return encodeError(`${sqlType} (an array)`, value);
    }
    return `{${value.map(element.literal).join(",")}}`;
  };
  return {
    sqlType,
    decode: (value) => {
      if (typeof value !== "string") {
        return decodeError(sqlType, value);
      }
      const root = parseArray(value);
      // An empty array has no dimensions at all, whatever the column says.
      return root.length === 0 ? [] : fromNode(root);
    },
    encode: toLiteral,
    element: (node) => (Array.isArray(node) && node.length === 0 ? [] : fromNode(node)),
    literal: toLiteral,
    depth: element.depth + 1,
  };
}

/**
 * An override's codec: `decode` narrows what `codec` read, `encode` widens a
 * value back to what `codec` writes. Generated for `overrides` in the plugin
 * options, where `decode` is the application's own check.
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
    element: (node) => decode(codec.element(node)),
    literal: (value) => codec.literal(encode(value)),
    depth: codec.depth,
  };
}

/** A `NULL`able value: the generated code's `x === null ? null : codec.decode(x)`, for arrays of codecs that need it. */
export function nullable<T>(codec: Codec<T>, value: mixed): T | null {
  return value === null ? null : codec.decode(value);
}

/** Encode a nullable parameter. */
export function param<T>(codec: Codec<T>, value: T | null): string | null {
  return value === null ? null : codec.encode(value);
}
