// @flow
//
// Checks every engine's codecs share. Each one either returns the value it was
// asked for or throws a `SqlError` that names what it expected — never a
// rounded number, a `NaN` or an `Invalid Date` handed on as if it were data.

import type { JsonValue } from "../index.js";
import { decodeError, encodeError } from "../index.js";

const INTEGER = /^[-+]?\d+$/;

/** A string of decimal digits as a `number`, which must be exactly representable. */
export function integerFromText(expected: string, value: mixed): number {
  if (typeof value === "number") {
    return safeInteger(expected, value);
  }
  if (typeof value === "bigint") {
    return safeInteger(expected, value);
  }
  if (typeof value !== "string" || !INTEGER.test(value)) {
    return decodeError(expected, value);
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) {
    return decodeError(`${expected} within ±2^53 (use the bigint representation)`, value);
  }
  return parsed;
}

/** A string of decimal digits, a `bigint` or a safe `number` as a `bigint`. */
export function bigintFrom(expected: string, value: mixed): bigint {
  if (typeof value === "bigint") {
    return value;
  }
  if (typeof value === "number") {
    // A number past 2^53 may already have been rounded by the driver, so it
    // cannot be trusted to be the integer the database holds.
    if (!Number.isSafeInteger(value)) {
      return decodeError(`${expected} (the driver returned a rounded number)`, value);
    }
    return BigInt(value);
  }
  if (typeof value !== "string" || !INTEGER.test(value)) {
    return decodeError(expected, value);
  }
  return BigInt(value);
}

/** A `number` or `bigint` that is an integer a `number` holds exactly. */
export function safeInteger(expected: string, value: number | bigint): number {
  if (typeof value === "bigint") {
    const asNumber = Number(value);
    if (!Number.isSafeInteger(asNumber)) {
      return decodeError(`${expected} within ±2^53 (use the bigint representation)`, value);
    }
    return asNumber;
  }
  if (!Number.isSafeInteger(value)) {
    return decodeError(`${expected} within ±2^53 (use the bigint representation)`, value);
  }
  return value;
}

/** Encode a `number` that must be an integer, for an integer column. */
export function integerParam(expected: string, value: mixed): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    return encodeError(expected, value);
  }
  return value;
}

/** Encode a `bigint`. */
export function bigintParam(expected: string, value: mixed): bigint {
  if (typeof value !== "bigint") {
    return encodeError(expected, value);
  }
  return value;
}

/** A string, unchanged. */
export function stringFrom(expected: string, value: mixed): string {
  if (typeof value !== "string") {
    return decodeError(expected, value);
  }
  return value;
}

/** Encode a string. */
export function stringParam(expected: string, value: mixed): string {
  if (typeof value !== "string") {
    return encodeError(expected, value);
  }
  return value;
}

/** Encode a finite or non-finite `number`. */
export function numberParam(expected: string, value: mixed): number {
  if (typeof value !== "number") {
    return encodeError(expected, value);
  }
  return value;
}

/** Parse a JSON document the database returned as text. */
export function jsonFromText(value: mixed): JsonValue {
  if (typeof value !== "string") {
    return decodeError("JSON text", value);
  }
  try {
    const parsed: JsonValue = JSON.parse(value);
    return parsed;
  } catch {
    return decodeError("JSON text", value);
  }
}

/** Serialise a JSON parameter. `undefined`, functions and symbols have no JSON. */
export function jsonParam(value: mixed): string {
  let text: string | void;
  try {
    text = JSON.stringify(value);
  } catch {
    // A cycle, or a `bigint`, which JSON has no spelling for.
    return encodeError("a JSON value", value);
  }
  if (text === undefined) {
    return encodeError("a JSON value", value);
  }
  return text;
}

/** Bytes, from a `Uint8Array` (a Node `Buffer` is one), an `ArrayBuffer` or an array of octets. */
export function bytesFrom(expected: string, value: mixed): Uint8Array {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (value instanceof ArrayBuffer) {
    return new Uint8Array(value);
  }
  if (Array.isArray(value)) {
    const out = new Uint8Array(value.length);
    for (let i = 0; i < value.length; i += 1) {
      const octet = value[i];
      if (typeof octet !== "number" || !Number.isInteger(octet) || octet < 0 || octet > 255) {
        return decodeError(expected, value);
      }
      out[i] = octet;
    }
    return out;
  }
  return decodeError(expected, value);
}

/** Encode bytes. */
export function bytesParam(expected: string, value: mixed): Uint8Array {
  if (!(value instanceof Uint8Array)) {
    return encodeError(expected, value);
  }
  return value;
}

/** One of an enum's labels. */
export function label<T extends string>(
  values: $ReadOnlyArray<T>,
  expected: string,
  value: mixed,
): T {
  const index = typeof value === "string" ? values.findIndex((item) => item === value) : -1;
  if (index < 0) {
    return decodeError(expected, value);
  }
  return values[index];
}

/** Encode one of an enum's labels, refusing a string that is not one. */
export function labelParam<T extends string>(
  values: $ReadOnlyArray<T>,
  expected: string,
  value: mixed,
): T {
  const index = typeof value === "string" ? values.findIndex((item) => item === value) : -1;
  if (index < 0) {
    return encodeError(expected, value);
  }
  return values[index];
}

/** How an enum is described in a message. */
export function enumName(values: $ReadOnlyArray<string>): string {
  return `one of ${values.map((value) => JSON.stringify(value)).join(", ")}`;
}
