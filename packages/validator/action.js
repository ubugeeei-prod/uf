// @flow
//
// `@uniflowed/validator/action`: the steps that come ready-made.
//
// Every one of them is a call to `refine` or `transform` with a name, a
// message and a [`Constraint`] attached, and they are together because that is
// what they are: a catalogue, in the same sense as `@uniflowed/form`'s
// `rules.js`. Splitting it by the type each one refines would be splitting by
// argument rather than by subject, and would leave three modules that each say
// "this is a `check` with a better error message".
//
// The constraint is the part that is not decoration. `minLength(3)` records
// `{ kind: "minLength", value: 3 }`, which is what lets `json-schema.js` emit
// `"minLength": 3` rather than shrugging; a hand-written `check((text) =>
// text.length >= 3, …)` does the same thing at run time and exports as
// nothing, because a predicate has no spelling in any export format.
//
// # Messages
//
// Each carries a default that says what was wanted rather than what was found
// — "expected at least 3 characters" — because the value is already in the
// caller's hands and the expectation is the half it is missing. Where a
// message wants to be user-facing prose, `check(predicate, "Pick a longer
// name")` is the way to say it; these are for the boundary, not the label.
//
// # What is deliberately absent
//
// No `creditCard`, `emoji`, `mac`, `imei`, `cuid2` or the rest of the long
// tail. Each is a regular expression with a maintenance schedule attached —
// Unicode adds emoji, card issuers add prefixes — and a validator that ships a
// stale one is worse than an application that writes `check` with a rule it
// owns. The ones here are either structural (`length`, `min`) or defined by a
// specification that does not move under them (`uuid`, `isoDate`).

import type { Step } from "./pipe.js";
import { refine, transform } from "./pipe.js";
import type { Schema } from "./schema.js";

/** At least `value` characters. */
export function minLength(value: number): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => input.length >= value,
      "min_length",
      `expected at least ${String(value)} characters`,
      { kind: "minLength", value },
    );
}

/** At most `value` characters. */
export function maxLength(value: number): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => input.length <= value,
      "max_length",
      `expected at most ${String(value)} characters`,
      { kind: "maxLength", value },
    );
}

/** Exactly `value` characters. */
export function length(value: number): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => input.length === value,
      "length",
      `expected exactly ${String(value)} characters`,
      { kind: "length", value },
    );
}

/**
 * At least one character.
 *
 * Separate from `minLength(1)` because it is the check a form makes on every
 * required text field, and `nonEmpty()` says why at the call site.
 */
export function nonEmpty(): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(schema, (input: string) => input.length > 0, "non_empty", "expected a value", {
      kind: "minLength",
      value: 1,
    });
}

export function startsWith(value: string): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => input.startsWith(value),
      "starts_with",
      `expected prefix ${value}`,
      { kind: "pattern", source: `^${escapeRegExp(value)}` },
    );
}

export function endsWith(value: string): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => input.endsWith(value),
      "ends_with",
      `expected suffix ${value}`,
      { kind: "pattern", source: `${escapeRegExp(value)}$` },
    );
}

export function includes(value: string): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => input.includes(value),
      "includes",
      `expected ${value} somewhere in the value`,
      { kind: "pattern", source: escapeRegExp(value) },
    );
}

/** `value` with every regular-expression metacharacter made literal. */
function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * A string matching `pattern`.
 *
 * The pattern is tested against a reset `lastIndex` every time, because a
 * caller who reaches for `/g` would otherwise get a schema that alternates
 * between accepting and rejecting the same input.
 */
export function regex(pattern: RegExp, message?: string): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => {
        pattern.lastIndex = 0;
        return pattern.test(input);
      },
      "regex",
      message ?? `expected a match for ${String(pattern)}`,
      { kind: "pattern", source: pattern.source },
    );
}

/**
 * Something shaped like an email address.
 *
 * Deliberately loose. The grammar in RFC 5322 accepts addresses no mail server
 * will route and the regular expressions that implement it are famous for
 * rejecting real ones; the only test that proves an address exists is sending
 * something to it. This rejects the typos — a missing at-sign, a missing dot —
 * and gets out of the way.
 */
export function email(): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(input),
      "email",
      "expected email address",
      { kind: "format", name: "email" },
    );
}

/** A URL the platform's own parser accepts, so the parse is the check. */
export function url(): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => {
        try {
          return new URL(input) != null;
        } catch {
          return false;
        }
      },
      "url",
      "expected a URL",
      { kind: "format", name: "uri" },
    );
}

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

/** A UUID with a version and a variant, as RFC 9562 defines them. */
export function uuid(): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(schema, (input: string) => UUID.test(input), "uuid", "expected a UUID", {
      kind: "format",
      name: "uuid",
    });
}

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

/**
 * A calendar date as `YYYY-MM-DD`.
 *
 * The shape is not enough: `2026-02-30` matches the pattern and is not a day.
 * The value is round-tripped through `Date` and compared back, which rejects
 * every month that is shorter than the payload thought.
 */
export function isoDate(): Step<string, string> {
  return <TInput>(schema: Schema<string, TInput>): Schema<string, TInput> =>
    refine(
      schema,
      (input: string) => {
        const parts = ISO_DATE.exec(input);
        if (parts == null) {
          return false;
        }
        const when = new Date(`${input}T00:00:00Z`);
        return Number.isFinite(when.getTime()) && when.toISOString().slice(0, 10) === input;
      },
      "iso_date",
      "expected a date as YYYY-MM-DD",
      { kind: "format", name: "date" },
    );
}

/** Whitespace off both ends, before whatever comes next in the pipeline. */
export function trim(): Step<string, string> {
  return transform((input: string) => input.trim());
}

export function toLowerCase(): Step<string, string> {
  return transform((input: string) => input.toLowerCase());
}

export function toUpperCase(): Step<string, string> {
  return transform((input: string) => input.toUpperCase());
}

/** At least `value`. */
export function min(value: number): Step<number, number> {
  return <TInput>(schema: Schema<number, TInput>): Schema<number, TInput> =>
    refine(schema, (input: number) => input >= value, "min", `expected at least ${String(value)}`, {
      kind: "min",
      value,
    });
}

/** At most `value`. */
export function max(value: number): Step<number, number> {
  return <TInput>(schema: Schema<number, TInput>): Schema<number, TInput> =>
    refine(schema, (input: number) => input <= value, "max", `expected at most ${String(value)}`, {
      kind: "max",
      value,
    });
}

export function integer(): Step<number, number> {
  return <TInput>(schema: Schema<number, TInput>): Schema<number, TInput> =>
    refine(schema, (input: number) => Number.isInteger(input), "integer", "expected an integer", {
      kind: "integer",
    });
}

/**
 * A multiple of `value`.
 *
 * The remainder is compared with a tolerance rather than against zero, because
 * `0.3 % 0.1` is `0.09999999999999998` and a step of `0.1` on a price field is
 * the reason anybody asks for this.
 */
export function multipleOf(value: number): Step<number, number> {
  return <TInput>(schema: Schema<number, TInput>): Schema<number, TInput> =>
    refine(
      schema,
      (input: number) => {
        const remainder = Math.abs(input % value);
        return remainder < 1e-9 || Math.abs(remainder - Math.abs(value)) < 1e-9;
      },
      "multiple_of",
      `expected a multiple of ${String(value)}`,
      { kind: "multipleOf", value },
    );
}

/** At least `value` items. Arrays, where `minLength` is for strings. */
export function minItems<TItem>(value: number): Step<$ReadOnlyArray<TItem>, $ReadOnlyArray<TItem>> {
  return <TInput>(
    schema: Schema<$ReadOnlyArray<TItem>, TInput>,
  ): Schema<$ReadOnlyArray<TItem>, TInput> =>
    refine(
      schema,
      (input: $ReadOnlyArray<TItem>) => input.length >= value,
      "min_items",
      `expected at least ${String(value)} items`,
      { kind: "minItems", value },
    );
}

/** At most `value` items. */
export function maxItems<TItem>(value: number): Step<$ReadOnlyArray<TItem>, $ReadOnlyArray<TItem>> {
  return <TInput>(
    schema: Schema<$ReadOnlyArray<TItem>, TInput>,
  ): Schema<$ReadOnlyArray<TItem>, TInput> =>
    refine(
      schema,
      (input: $ReadOnlyArray<TItem>) => input.length <= value,
      "max_items",
      `expected at most ${String(value)} items`,
      { kind: "maxItems", value },
    );
}
