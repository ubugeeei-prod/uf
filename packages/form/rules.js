// @flow
//
// `@uniflowed/form/rules`: what a built-in rule means.
//
// The rules a form declares inline — `register("age", { required: true, min: 18 })`
// — and nothing else. No React, no store, no DOM: [`runRules`] is a function
// from a value and a rule set to an error or `null`, which is why it is
// testable on its own and why the same rules run identically during a submit,
// during a keystroke, and on a server.
//
// The other source of truth for whether a value is acceptable is a schema,
// and that lives in `resolver.js`. The two are exclusive by design and the
// reason is in that module's header.
//
// # Why an empty value only meets `required` and `validate`
//
// `minLength: 8` on an empty password reports "must be at least 8 characters",
// which is true and useless: the user has not typed anything, and the message
// they need is that the field is required. So every rule except `required` and
// `validate` is skipped for a value that is blank.
//
// `validate` is the exception because it is the caller's own function and the
// caller may well have written it *for* the empty case — "at least one of
// these three fields" is a real rule and it fires precisely when a field is
// empty.
//
// # Why the messages have defaults
//
// `{ required: true }` with no message is the shortest thing to write and the
// most common thing to write, and a library that answers it with an empty
// string has produced an error nobody can render. The defaults below are
// English and are meant to be replaced — every rule takes
// `{ value, message }` — but a form that has not been through a copy review
// still says something true.

import type { FieldValues } from "./internal/field-path.js";

/** A rule given either bare or with the message it should report. */
export type Rule<TLimit> = TLimit | {| readonly value: TLimit, readonly message: string |};

/**
 * A caller's own check.
 *
 * Returning `true` (or nothing) accepts; returning `false` rejects with the
 * default message; returning a string rejects with that string. A promise is
 * awaited, which is what makes "is this username taken" an ordinary rule
 * rather than a reason to leave the library.
 */
export type Validate = (
  value: mixed,
  values: FieldValues,
) => boolean | string | void | Promise<boolean | string | void>;

/** What a field is checked against, and how its raw value is read. */
export type ValidationRules = {|
  readonly required?: boolean | string | {| readonly value: boolean, readonly message: string |},
  readonly min?: Rule<number | string>,
  readonly max?: Rule<number | string>,
  readonly minLength?: Rule<number>,
  readonly maxLength?: Rule<number>,
  readonly pattern?: Rule<RegExp>,
  /** One check, or several keyed by name so the error says which one failed. */
  readonly validate?: Validate | { readonly [string]: Validate, ... },
  /** Read the control's text as a number. */
  readonly valueAsNumber?: boolean,
  /** Read the control's text as a `Date`. */
  readonly valueAsDate?: boolean,
  /** Convert the raw value however the caller likes; runs last. */
  readonly setValueAs?: (value: mixed) => mixed,
  /**
   * Other fields whose errors are re-checked when this one changes.
   *
   * For the rules that are about a pair: a confirmation that must match a
   * password, an end date that must follow a start date. Without it the second
   * field keeps the error it earned before the first one was corrected.
   */
  readonly deps?: string | $ReadOnlyArray<string>,
|};

/** One field's error: what failed, and what to show for it. */
export type FieldError = {|
  /** The rule that rejected the value: `"required"`, `"pattern"`, a
   * `validate` key, or whatever a resolver reported. */
  readonly type: string,
  readonly message: string,
|};

function limitOf<TLimit>(rule: Rule<TLimit>): TLimit {
  return rule != null && typeof rule === "object" && "value" in rule
    ? (rule as $FlowFixMe).value
    : (rule as $FlowFixMe);
}

function messageOf<TLimit>(rule: Rule<TLimit>, fallback: string): string {
  return rule != null && typeof rule === "object" && "message" in rule
    ? String((rule as $FlowFixMe).message)
    : fallback;
}

/**
 * Whether a value counts as "not filled in".
 *
 * `false` is deliberately absent: an unchecked checkbox is a real answer to a
 * yes/no question, and `{ required: true }` on one means "you must tick this",
 * which is exactly what `false` failing gives. `NaN` is present because that is
 * what an empty `valueAsNumber` control reads as.
 */
function isBlank(value: mixed): boolean {
  if (value == null || value === "") {
    return true;
  }
  if (typeof value === "number") {
    return Number.isNaN(value);
  }
  if (Array.isArray(value)) {
    return value.length === 0;
  }
  if (typeof value === "object" && typeof (value as $FlowFixMe).length === "number") {
    // A `FileList`, which is the one array-like a form field holds.
    return (value as $FlowFixMe).length === 0;
  }
  return false;
}

/**
 * Compare against a limit that may be a number, a date, or a date string.
 *
 * `min` on `<input type="date">` is written as `min: "2026-01-01"`, and the
 * value read out of that control is a string too. Comparing them as strings
 * happens to work for ISO dates and stops working the moment either side is a
 * `Date`, so both sides are put on the same scale first.
 */
function scaleOf(value: mixed): number {
  if (typeof value === "number") {
    return value;
  }
  if (value instanceof Date) {
    return value.getTime();
  }
  const text = String(value ?? "");
  const asNumber = Number(text);
  if (text.trim() !== "" && !Number.isNaN(asNumber)) {
    return asNumber;
  }
  return new Date(text).getTime();
}

function lengthOf(value: mixed): number {
  if (typeof value === "string") {
    return value.length;
  }
  if (Array.isArray(value)) {
    return value.length;
  }
  return String(value ?? "").length;
}

/**
 * Continue with `next`, whether the answer arrived or was promised.
 *
 * Every check in a form is one of two shapes: a comparison that answers now,
 * and a request that answers later. Awaiting both would be simpler to write and
 * would put a microtask between every keystroke and the error that keystroke
 * cleared — which is a render the form did not need, in the mode where renders
 * are the thing being avoided. So the synchronous case stays synchronous all
 * the way up to the store, and this is the joint it turns on.
 *
 * It lives here rather than in a module of loose helpers because this is where
 * the two shapes first meet: `validate` is the first thing in the package that
 * a caller may write either way.
 */
export function whenSettled<TValue, TNext>(
  value: TValue | Promise<TValue>,
  next: (value: TValue) => TNext,
): TNext | Promise<TNext> {
  if (value != null && typeof (value as $FlowFixMe).then === "function") {
    return (value as $FlowFixMe).then(next);
  }
  return next(value as $FlowFixMe);
}

/**
 * Check `value` against `rules`, in the order a reader would.
 *
 * `required` first, because "this is empty" outranks every other complaint
 * about an empty value. Then the limits, then the pattern, then the caller's
 * own checks — so a field that is both too short and malformed reports being
 * too short, which is the problem the user has to fix first.
 *
 * Returns the first failure. A form that reported every failing rule for one
 * field would have to choose which to show anyway, and choosing here means the
 * choice is documented rather than implicit in a render.
 *
 * Synchronous unless the caller's own `validate` is not — see [`whenSettled`].
 */
export function runRules(
  rules: ValidationRules,
  value: mixed,
  values: FieldValues,
): FieldError | null | Promise<FieldError | null> {
  const blank = isBlank(value);

  if (rules.required != null && rules.required !== false) {
    const required = rules.required;
    const message =
      typeof required === "string"
        ? required
        : typeof required === "object"
          ? String(required.message)
          : "This field is required";
    const demanded = typeof required === "object" ? required.value !== false : true;
    if (demanded && blank) {
      return { type: "required", message };
    }
  }

  if (!blank) {
    if (rules.min != null) {
      const limit = limitOf(rules.min);
      if (scaleOf(value) < scaleOf(limit)) {
        return { type: "min", message: messageOf(rules.min, `Must be at least ${String(limit)}`) };
      }
    }

    if (rules.max != null) {
      const limit = limitOf(rules.max);
      if (scaleOf(value) > scaleOf(limit)) {
        return { type: "max", message: messageOf(rules.max, `Must be at most ${String(limit)}`) };
      }
    }

    if (rules.minLength != null) {
      const limit = limitOf(rules.minLength);
      if (lengthOf(value) < limit) {
        return {
          type: "minLength",
          message: messageOf(rules.minLength, `Must be at least ${String(limit)} characters`),
        };
      }
    }

    if (rules.maxLength != null) {
      const limit = limitOf(rules.maxLength);
      if (lengthOf(value) > limit) {
        return {
          type: "maxLength",
          message: messageOf(rules.maxLength, `Must be at most ${String(limit)} characters`),
        };
      }
    }

    if (rules.pattern != null) {
      const pattern = limitOf(rules.pattern);
      // A global regular expression carries `lastIndex` between calls, so the
      // same value tests true, then false, then true. Testing a copy without
      // the flag makes the rule a function of its input, which is what every
      // caller assumed it already was.
      const stateless = pattern.global
        ? new RegExp(pattern.source, pattern.flags.replace("g", ""))
        : pattern;
      if (!stateless.test(String(value ?? ""))) {
        return {
          type: "pattern",
          message: messageOf(rules.pattern, "This is not in the right format"),
        };
      }
    }
  }

  if (rules.validate != null) {
    return runValidate(rules.validate, value, values);
  }

  return null;
}

function runValidate(
  validate: Validate | { readonly [string]: Validate, ... },
  value: mixed,
  values: FieldValues,
): FieldError | null | Promise<FieldError | null> {
  if (typeof validate === "function") {
    return whenSettled(validate(value, values), (result) => interpret("validate", result));
  }
  return runValidateEntries(Object.keys(validate), validate, value, values, 0);
}

/**
 * Run named checks in order, stopping at the first failure.
 *
 * Recursive rather than a loop because a loop would need `await`, and awaiting
 * turns a set of synchronous checks into a set of microtasks. The recursion is
 * bounded by the number of checks on one field.
 */
function runValidateEntries(
  keys: $ReadOnlyArray<string>,
  validate: { readonly [string]: Validate, ... },
  value: mixed,
  values: FieldValues,
  at: number,
): FieldError | null | Promise<FieldError | null> {
  if (at >= keys.length) {
    return null;
  }
  const key = keys[at];
  return whenSettled(validate[key](value, values), (result) => {
    const failure = interpret(key, result);
    return failure ?? runValidateEntries(keys, validate, value, values, at + 1);
  });
}

/**
 * A `validate` result as an error, or `null`.
 *
 * `undefined` accepts, so a check written as a series of `if` statements with
 * no trailing `return true` still passes — which is how people write them, and
 * treating the missing return as a failure would reject every valid value in
 * the form.
 */
function interpret(type: string, result: boolean | string | void): FieldError | null {
  if (result === false) {
    return { type, message: "This value is not valid" };
  }
  if (typeof result === "string") {
    return { type, message: result };
  }
  return null;
}

/** The fields whose errors should be re-checked when `name` changes. */
export function dependenciesOf(rules: ValidationRules): $ReadOnlyArray<string> {
  const deps = rules.deps;
  if (deps == null) {
    return [];
  }
  return typeof deps === "string" ? [deps] : deps;
}

/** The subset of a rule set that says how to read the control's raw value. */
export function transformOf(rules: ValidationRules): {|
  readonly valueAsNumber?: boolean,
  readonly valueAsDate?: boolean,
  readonly setValueAs?: (value: mixed) => mixed,
|} {
  return {
    valueAsNumber: rules.valueAsNumber,
    valueAsDate: rules.valueAsDate,
    setValueAs: rules.setValueAs,
  };
}
