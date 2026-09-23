// @flow
//
// What the `options-postgresql` case's overrides point at: a settings type
// with a checked decoder, and an opaque id that only `toExternalId` makes.

import type { JsonValue } from "@uniflowed/sql";

export type Settings = {| readonly theme: "light" | "dark" |};

/** Narrow a stored JSON document to `Settings`, refusing anything else. */
export function parseSettings(value: JsonValue): Settings {
  if (
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    (value.theme === "light" || value.theme === "dark")
  ) {
    return { theme: value.theme };
  }
  throw new TypeError(`not account settings: ${JSON.stringify(value)}`);
}

export opaque type ExternalId: string = string;

/** Check a UUID's shape and make it an `ExternalId`. */
export function toExternalId(value: string): ExternalId {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value)) {
    throw new TypeError(`not a UUID: ${value}`);
  }
  return value.toLowerCase();
}

/** An `ExternalId` is already the string the column stores. */
export function fromExternalId(value: ExternalId): string {
  return value;
}
