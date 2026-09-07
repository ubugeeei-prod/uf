// @flow
//
// `@uniflowed/i18n/format`: a parsed message, a locale and some arguments,
// into a string.
//
// # Why uf formats MF2 itself, and does not format numbers or dates
//
// `Intl.MessageFormat` is the obvious thing to build on and it does not exist.
// It is a TC39 proposal with no implementation in any shipping browser or
// runtime, so a package that used it would be a package that never ran, and
// one that fell back to its own formatter when the constructor was missing
// would be worse: two code paths, one of which nobody has executed, deciding
// what a user reads. The same catalogue would render differently in Safari and
// in Node, and nothing in a test suite would notice until a screenshot in
// another browser did.
//
// So the *algorithm* is here — parsing, declarations, selection, the variant
// sort — and none of the *data* is. Plural categories come from
// `Intl.PluralRules`, numerals from `Intl.NumberFormat`, dates from
// `Intl.DateTimeFormat`. That split is the whole design, and the reason is
// weight: the selection algorithm below is a few hundred lines, and CLDR's
// plural rules and number formats for the locales a browser already carries
// are megabytes. A library that shipped its own would be shipping a second,
// staler copy of something in every runtime uf targets, and the day the two
// disagreed the user would see a number formatted one way in a message and
// another way beside it.
//
// When `Intl.MessageFormat` does ship, the seam is `formatMessage` in this
// module rather than anything a caller wrote — which is the point of the
// catalogue being an API rather than a string table.
//
// `Intl.PluralRules` is optional in Flow's own library definition, and it is
// genuinely absent from a small-icu Node build. A `.match` on `:number` in
// that runtime raises rather than guessing English, because a message that
// silently pluralises Polish as if it were English is a bug that reaches
// production looking like a translation mistake.
//
// # Why the three constructors are declared here
//
// The same reason `@uniflowed/hooks`'s `timing.js` declares
// `Intl.RelativeTimeFormat`: the vendored `intl.js` has not caught up. Its
// `Intl$DateTimeFormatOptions` predates `dateStyle` and `timeStyle`, which are
// exactly what MF2 defines `:date` and `:time` in terms of — and its option
// types have invariant optional properties, so an options object built at run
// time from a message cannot be passed to them at all without listing every
// property ECMA-402 has ever had.
//
// So the three constructors this module calls are declared narrowly, with the
// options MF2 actually names. That is not a way around the checker: it is a
// *stricter* type than the libdef's, because MF2's `:number` takes
// `style=decimal` or `style=percent` and nothing else, and a message asking
// for `style=currency` should be refused rather than passed through. The
// declarations are as wide as what is called and no wider, and
// `Intl.PluralRules` stays optional so the small-icu branch above survives.
//
// # Why the Intl objects are cached, and on the catalogue
//
// Constructing an `Intl.NumberFormat` is the expensive part of formatting a
// number — enough that building one per call is the standard way an
// application's first render becomes slow — so they are memoised by locale and
// options.
//
// The cache lives on the catalogue rather than in a module-level `Map`,
// because a module-level cache is a leak with no owner: it is keyed by strings
// nobody frees, it is shared between a server's requests for different
// locales, and it makes two tests in one process affect each other. A
// catalogue is already the thing whose lifetime this data has.
//
// # Why the arguments arrive as `mixed` and are read through a `Map`
//
// Two reasons, and the second is the one that matters.
//
// Inside `defineCatalogue`'s generic body, `ArgsOf<TMessages[TKey]>` is a
// conditional type Flow has nothing to resolve it against yet, so it is
// `mixed` there. A dictionary parameter here would make the one place this
// package has to be generic the one place it cannot compile — and the
// checking it looks like it is buying already happened at the call, where Flow
// does know the message.
//
// And an argument object is data from outside. Copying it into a `Map` once
// means `{"__proto__": …}` and `{"hasOwnProperty": …}` are ordinary keys
// rather than a lookup that walks a prototype, which is the same rule
// `@uniflowed/validator`'s `plain-object.js` follows for the same reason.
//
// # What happens when formatting fails anyway
//
// Almost nothing should reach here: the argument type is checked by Flow at
// the call, and the message is checked against its parameters when it is
// declared. What is left is the case where a value arrived from outside the
// type system — a JSON response, an `any`, a server prop — and is not what the
// message needs.
//
// MF2 says to emit a fallback (`{$name}`) and signal an error. uf does both,
// and lets the catalogue decide what "signal" means: `onError` defaults to
// throwing, because a formatting error here means the types were bypassed and
// that is worth finding, and an application that would rather ship a fallback
// than a blank page passes an `onError` that records instead.

import type {
  MessageAnnotation,
  MessageExpression,
  MessageNode,
  MessageOperand,
  MessagePattern,
  MessageVariant,
} from "./syntax.js";

/** The four widths MF2 gives `:date` and `:time`. */
type DateWidth = "full" | "long" | "medium" | "short";

/** What MF2's `:date`, `:time` and `:datetime` may ask `Intl` for. */
type DateTimeOptions = {
  dateStyle?: DateWidth,
  timeStyle?: DateWidth,
  weekday?: "narrow" | "short" | "long",
  era?: "narrow" | "short" | "long",
  year?: "numeric" | "2-digit",
  month?: "numeric" | "2-digit" | "narrow" | "short" | "long",
  day?: "numeric" | "2-digit",
  hour?: "numeric" | "2-digit",
  minute?: "numeric" | "2-digit",
  second?: "numeric" | "2-digit",
  fractionalSecondDigits?: number,
  timeZoneName?: "short" | "long",
  hour12?: boolean,
  timeZone?: string,
  calendar?: string,
  numberingSystem?: string,
};

/**
 * What MF2's `:number` and `:integer` may ask `Intl` for.
 *
 * Narrower than ECMA-402 on purpose: `style` is `decimal` or `percent`,
 * because those are the two MF2's default registry defines. `currency` and
 * `unit` are in the *draft* registry and need an operand this package does not
 * accept, so a message asking for one is refused rather than handed to `Intl`
 * without the currency code it would then need.
 */
type NumberOptions = {
  style?: "decimal" | "percent",
  minimumIntegerDigits?: number,
  minimumFractionDigits?: number,
  maximumFractionDigits?: number,
  minimumSignificantDigits?: number,
  maximumSignificantDigits?: number,
  useGrouping?: boolean,
  signDisplay?: "auto" | "always" | "exceptZero" | "never",
  notation?: "standard" | "scientific" | "engineering" | "compact",
  compactDisplay?: "short" | "long",
  numberingSystem?: string,
};

declare class MessageNumberFormat {
  constructor(locale: string, options: NumberOptions): void;
  format(value: number): string;
}

declare class MessageDateTimeFormat {
  constructor(locale: string, options: DateTimeOptions): void;
  format(value: Date): string;
}

declare class MessagePluralRules {
  constructor(locale: string, options: { type: "cardinal" | "ordinal" }): void;
  select(value: number): string;
}

/** See the module header on why these are declared rather than imported. */
declare var Intl: {
  NumberFormat: Class<MessageNumberFormat>,
  DateTimeFormat: Class<MessageDateTimeFormat>,
  PluralRules?: Class<MessagePluralRules>,
  ...
};

/**
 * A value that could not be formatted, and the fallback that was used instead.
 *
 * Not thrown from this module directly: the catalogue's `onError` decides
 * whether it is thrown at all.
 */
export class MessageFormatError extends Error {
  /** The MF2 fallback for the expression that failed, such as `{$count}`. */
  fallback: string;

  constructor(message: string, fallback: string) {
    super(`@uniflowed/i18n: ${message}`);
    this.name = "MessageFormatError";
    this.fallback = fallback;
  }
}

/** What a catalogue hands `formatMessage` so that two calls can share work. */
export type FormatContext = {
  readonly locale: string,
  /**
   * U+2068/U+2069 around every placeholder, per MF2's default bidi strategy.
   *
   * Off here, and the specification allows that — `bidiIsolation` is a
   * formatting option with a `none` value for exactly this. The reason to
   * default the other way from the specification is that uf's output goes into
   * React children, where the DOM already isolates by direction from `dir` and
   * the Unicode algorithm, and two invisible code points per placeholder would
   * turn every `expect(t(…)).toBe("…")` in every application into a puzzle.
   *
   * A page that concatenates message output into a single text node with
   * right-to-left content in it should turn this on.
   */
  readonly bidiIsolation: boolean,
  readonly onError: (error: MessageFormatError) => void,
  readonly numberFormats: Map<string, MessageNumberFormat>,
  readonly dateFormats: Map<string, MessageDateTimeFormat>,
  readonly pluralRules: Map<string, MessagePluralRules>,
};

/** The options one annotation was given, already resolved to values. */
type OptionBag = Map<string, mixed>;

/** A value with the annotation that was applied to it, if any. */
type Resolved = {
  readonly value: mixed,
  readonly functionName: string | null,
  readonly options: OptionBag,
  /** The MF2 fallback text for the expression this came from. */
  readonly fallback: string,
};

type Scope = Map<string, Resolved>;

/** The six functions of the MF2 default registry. Nothing else is accepted. */
const KNOWN_FUNCTIONS: $ReadOnlyArray<string> = [
  "string",
  "number",
  "integer",
  "date",
  "time",
  "datetime",
];

/** The three of those that MF2 allows in a `.match`. */
const SELECTOR_FUNCTIONS: $ReadOnlyArray<string> = ["string", "number", "integer"];

export function isKnownFunction(name: string): boolean {
  return KNOWN_FUNCTIONS.includes(name);
}

/** MF2's fallback representation of an expression that could not be resolved. */
function fallbackFor(expression: MessageExpression): string {
  const operand = expression.operand;
  if (operand != null && operand.kind === "variable") return `{$${operand.name}}`;
  if (operand != null) return `{|${operand.value}|}`;
  const annotation = expression.annotation;
  return annotation == null ? "{}" : `{:${annotation.name}}`;
}

/**
 * A cache key for one formatter.
 *
 * Built from the option bag rather than from the object handed to `Intl`,
 * because the bag is what differs: `:date` and `:time` produce different
 * objects from the same bag, which is what `prefix` is for.
 */
function formatterKey(prefix: string, locale: string, options: OptionBag): string {
  const names = Array.from(options.keys()).sort();
  let key = `${prefix} ${locale}`;
  for (const name of names) {
    key += ` ${name} ${String(options.get(name))}`;
  }
  return key;
}

/**
 * A whole number an option was given, or `null` if it was not a number at all.
 *
 * Option values arrive as literals — always strings, because that is what MF2
 * source is — or as variables, which may already be numbers. Both spellings
 * have to reach `Intl` as a number, and neither may reach it as `NaN`.
 */
function asInteger(value: mixed): number | null {
  if (typeof value === "number") return Number.isFinite(value) ? Math.trunc(value) : null;
  if (typeof value !== "string") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? Math.trunc(parsed) : null;
}

/** A number, from the three spellings MF2 says `:number` accepts. */
function asNumber(value: mixed): number | null {
  if (typeof value === "number") return Number.isNaN(value) ? null : value;
  if (typeof value === "bigint") return Number(value);
  if (typeof value === "string" && value.trim() !== "") {
    const parsed = Number(value);
    return Number.isNaN(parsed) ? null : parsed;
  }
  return null;
}

/** An instant, from a `Date`, epoch milliseconds, or an ISO 8601 string. */
function asDate(value: mixed): Date | null {
  if (value instanceof Date) return Number.isNaN(value.getTime()) ? null : value;
  if (typeof value === "number") return Number.isNaN(value) ? null : new Date(value);
  if (typeof value === "string") {
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? null : parsed;
  }
  return null;
}

function asWidth(value: mixed): DateWidth | null {
  if (value === "full" || value === "long" || value === "medium" || value === "short") return value;
  return null;
}

/** The value of an operand, looked up in the scope if it is a variable. */
function resolveOperand(
  operand: MessageOperand,
  scope: Scope,
  args: Map<string, mixed>,
): { readonly value: mixed, readonly known: boolean } {
  if (operand.kind === "literal") return { value: operand.value, known: true };
  const declared = scope.get(operand.name);
  if (declared != null) return { value: declared.value, known: true };
  if (args.has(operand.name)) return { value: args.get(operand.name), known: true };
  return { value: undefined, known: false };
}

function resolveOptions(
  annotation: MessageAnnotation,
  scope: Scope,
  args: Map<string, mixed>,
): OptionBag {
  const resolved: OptionBag = new Map();
  for (const option of annotation.options) {
    resolved.set(option.name, resolveOperand(option.value, scope, args).value);
  }
  return resolved;
}

/**
 * One `{…}`, resolved but not yet turned into text.
 *
 * Split from formatting because a `.match` selector needs the resolved value
 * and its annotation without ever formatting it, and because an `.input`
 * declaration stores exactly this.
 */
function resolveExpression(
  expression: MessageExpression,
  scope: Scope,
  args: Map<string, mixed>,
  context: FormatContext,
): Resolved {
  const fallback = fallbackFor(expression);
  const annotation = expression.annotation;
  const operand = expression.operand;

  let value: mixed = undefined;
  let inherited: string | null = null;
  let inheritedOptions: OptionBag = new Map();
  if (operand != null) {
    const found = resolveOperand(operand, scope, args);
    if (!found.known) {
      const named = operand.kind === "variable" ? `$${operand.name}` : "a value";
      context.onError(
        new MessageFormatError(`the message reads ${named}, which was not given to it`, fallback),
      );
      return { value: undefined, functionName: null, options: new Map(), fallback };
    }
    value = found.value;
    if (operand.kind === "variable") {
      // `.input {$n :number}` then `{$n}` formats as a number: an annotation
      // put on a name by a declaration travels with the name. This is what
      // makes `.input` worth having over repeating the annotation.
      const declared = scope.get(operand.name);
      if (declared != null) {
        inherited = declared.functionName;
        inheritedOptions = declared.options;
      }
    }
  }

  if (annotation == null) {
    return { value, functionName: inherited, options: inheritedOptions, fallback };
  }
  const options: OptionBag = new Map(inheritedOptions);
  for (const [name, given] of resolveOptions(annotation, scope, args)) {
    options.set(name, given);
  }
  return { value, functionName: annotation.name, options, fallback };
}

/**
 * MF2's `:number` options translated into `Intl.NumberFormat`'s.
 *
 * Written out one property at a time rather than copied across, and that is
 * what makes the narrow `NumberOptions` above real: an option MF2 does not
 * define, or a value outside the set it allows, is dropped here rather than
 * reaching `Intl` — where it would either throw a `RangeError` at a user or,
 * worse, be silently ignored.
 *
 * `:integer` is `:number` with the fraction digits pinned, which is how the
 * specification defines it rather than as a function of its own.
 */
function numberOptions(options: OptionBag, integer: boolean): NumberOptions {
  const out: NumberOptions = {};

  const style = options.get("style");
  if (style === "decimal" || style === "percent") out.style = style;

  const signDisplay = options.get("signDisplay");
  if (
    signDisplay === "auto" ||
    signDisplay === "always" ||
    signDisplay === "exceptZero" ||
    signDisplay === "never"
  ) {
    out.signDisplay = signDisplay;
  }

  const notation = options.get("notation");
  if (
    notation === "standard" ||
    notation === "scientific" ||
    notation === "engineering" ||
    notation === "compact"
  ) {
    out.notation = notation;
  }

  const compactDisplay = options.get("compactDisplay");
  if (compactDisplay === "short" || compactDisplay === "long") out.compactDisplay = compactDisplay;

  const numberingSystem = options.get("numberingSystem");
  if (typeof numberingSystem === "string") out.numberingSystem = numberingSystem;

  const grouping = options.get("useGrouping");
  if (grouping != null) out.useGrouping = grouping !== "false" && grouping !== false;

  const minimumIntegerDigits = asInteger(options.get("minimumIntegerDigits"));
  if (minimumIntegerDigits != null) out.minimumIntegerDigits = minimumIntegerDigits;

  const minimumSignificantDigits = asInteger(options.get("minimumSignificantDigits"));
  if (minimumSignificantDigits != null) out.minimumSignificantDigits = minimumSignificantDigits;

  const maximumSignificantDigits = asInteger(options.get("maximumSignificantDigits"));
  if (maximumSignificantDigits != null) out.maximumSignificantDigits = maximumSignificantDigits;

  if (integer) {
    out.minimumFractionDigits = 0;
    out.maximumFractionDigits = 0;
    return out;
  }

  const minimumFractionDigits = asInteger(options.get("minimumFractionDigits"));
  if (minimumFractionDigits != null) out.minimumFractionDigits = minimumFractionDigits;

  const maximumFractionDigits = asInteger(options.get("maximumFractionDigits"));
  if (maximumFractionDigits != null) out.maximumFractionDigits = maximumFractionDigits;

  return out;
}

/** MF2's `:date`, `:time` and `:datetime` options, the same way. */
function dateTimeOptions(functionName: string, options: OptionBag): DateTimeOptions {
  const out: DateTimeOptions = {};

  const timeZone = options.get("timeZone");
  if (typeof timeZone === "string") out.timeZone = timeZone;
  const calendar = options.get("calendar");
  if (typeof calendar === "string") out.calendar = calendar;
  const numberingSystem = options.get("numberingSystem");
  if (typeof numberingSystem === "string") out.numberingSystem = numberingSystem;
  const hour12 = options.get("hour12");
  if (hour12 != null) out.hour12 = hour12 !== "false" && hour12 !== false;

  if (functionName === "date") {
    out.dateStyle = asWidth(options.get("style")) ?? "medium";
    return out;
  }
  if (functionName === "time") {
    out.timeStyle = asWidth(options.get("style")) ?? "short";
    return out;
  }

  // `:datetime` takes the field options directly. It only defaults when it was
  // given none of them, because `{$at :datetime year=numeric}` gaining a month
  // and a day the author did not ask for is a worse surprise than a bare
  // `{$at :datetime}` having to pick something.
  const dateStyle = asWidth(options.get("dateStyle"));
  if (dateStyle != null) out.dateStyle = dateStyle;
  const timeStyle = asWidth(options.get("timeStyle"));
  if (timeStyle != null) out.timeStyle = timeStyle;

  const weekday = options.get("weekday");
  if (weekday === "narrow" || weekday === "short" || weekday === "long") out.weekday = weekday;
  const era = options.get("era");
  if (era === "narrow" || era === "short" || era === "long") out.era = era;
  const year = options.get("year");
  if (year === "numeric" || year === "2-digit") out.year = year;
  const month = options.get("month");
  if (
    month === "numeric" ||
    month === "2-digit" ||
    month === "narrow" ||
    month === "short" ||
    month === "long"
  ) {
    out.month = month;
  }
  const day = options.get("day");
  if (day === "numeric" || day === "2-digit") out.day = day;
  const hour = options.get("hour");
  if (hour === "numeric" || hour === "2-digit") out.hour = hour;
  const minute = options.get("minute");
  if (minute === "numeric" || minute === "2-digit") out.minute = minute;
  const second = options.get("second");
  if (second === "numeric" || second === "2-digit") out.second = second;
  const fractionalSecondDigits = asInteger(options.get("fractionalSecondDigits"));
  if (fractionalSecondDigits != null) out.fractionalSecondDigits = fractionalSecondDigits;
  const timeZoneName = options.get("timeZoneName");
  if (timeZoneName === "short" || timeZoneName === "long") out.timeZoneName = timeZoneName;

  if (
    out.dateStyle == null &&
    out.timeStyle == null &&
    out.weekday == null &&
    out.era == null &&
    out.year == null &&
    out.month == null &&
    out.day == null &&
    out.hour == null &&
    out.minute == null &&
    out.second == null
  ) {
    out.dateStyle = "medium";
    out.timeStyle = "short";
  }
  return out;
}

function numberFormat(context: FormatContext, options: NumberOptions, key: string) {
  const cached = context.numberFormats.get(key);
  if (cached != null) return cached;
  const made = new Intl.NumberFormat(context.locale, options);
  context.numberFormats.set(key, made);
  return made;
}

function dateFormat(context: FormatContext, options: DateTimeOptions, key: string) {
  const cached = context.dateFormats.get(key);
  if (cached != null) return cached;
  const made = new Intl.DateTimeFormat(context.locale, options);
  context.dateFormats.set(key, made);
  return made;
}

function pluralRules(context: FormatContext, type: "cardinal" | "ordinal") {
  const cached = context.pluralRules.get(type);
  if (cached != null) return cached;
  const constructor = Intl.PluralRules;
  if (constructor == null) {
    // See the module header: guessing English here would be a translation bug
    // that only shows up in the languages uf is least able to check.
    throw new MessageFormatError(
      "this runtime has no Intl.PluralRules, so a .match on a number cannot choose a variant; " +
        "a small-icu Node build is the usual cause",
      "",
    );
  }
  const made = new constructor(context.locale, { type });
  context.pluralRules.set(type, made);
  return made;
}

/**
 * The function that applies when none was written.
 *
 * MF2 leaves an unannotated placeholder to the implementation, and the choice
 * here is the one that makes `Hello, {$name}!` and `You have {$count}` both do
 * what their author meant: format by the runtime type of the value. The
 * alternative — everything is a string — renders 1234567 identically in every
 * locale, which is the bug this package exists to prevent.
 */
function impliedFunction(value: mixed): string {
  if (typeof value === "number" || typeof value === "bigint") return "number";
  if (value instanceof Date) return "datetime";
  return "string";
}

/** A value named the way an error message should name it. */
function describe(value: mixed): string {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  if (typeof value === "string") return `the string ${JSON.stringify(value)}`;
  return `a ${typeof value}`;
}

function formatValue(resolved: Resolved, context: FormatContext): string {
  const name = resolved.functionName ?? impliedFunction(resolved.value);
  const value = resolved.value;

  if (name === "string") {
    if (value == null) {
      context.onError(
        new MessageFormatError("a placeholder was given null or undefined", resolved.fallback),
      );
      return resolved.fallback;
    }
    return String(value);
  }

  if (name === "number" || name === "integer") {
    const numeric = asNumber(value);
    if (numeric == null) {
      context.onError(
        new MessageFormatError(
          `:${name} was given ${describe(value)}, which is not a number`,
          resolved.fallback,
        ),
      );
      return resolved.fallback;
    }
    const integer = name === "integer";
    const key = formatterKey(integer ? "integer" : "number", context.locale, resolved.options);
    return numberFormat(context, numberOptions(resolved.options, integer), key).format(numeric);
  }

  const instant = asDate(value);
  if (instant == null) {
    context.onError(
      new MessageFormatError(
        `:${name} was given ${describe(value)}, which is not a date`,
        resolved.fallback,
      ),
    );
    return resolved.fallback;
  }
  const key = formatterKey(name, context.locale, resolved.options);
  return dateFormat(context, dateTimeOptions(name, resolved.options), key).format(instant);
}

/**
 * The keys of `keys` this value selects, best first.
 *
 * MF2 gives each selector function its own `selectKey`, and the two that exist
 * here differ in one thing: `:number` tries the literal value before it tries
 * the plural category, so `0 {{No messages}}` beats `other` for zero in a
 * language where zero is `other`. That ordering is what makes a special case
 * for one number expressible without a second message.
 */
function selectKeys(
  resolved: Resolved,
  keys: $ReadOnlyArray<string>,
  context: FormatContext,
): $ReadOnlyArray<string> {
  const name = resolved.functionName ?? impliedFunction(resolved.value);
  if (!SELECTOR_FUNCTIONS.includes(name)) {
    throw new MessageFormatError(
      `:${name} cannot be a .match selector; only :string, :number and :integer can`,
      resolved.fallback,
    );
  }

  if (name === "string") {
    const exact = String(resolved.value);
    return keys.filter((key) => key === exact);
  }

  const numeric = asNumber(resolved.value);
  if (numeric == null) {
    context.onError(
      new MessageFormatError(
        `a .match on :${name} was given ${describe(resolved.value)}`,
        resolved.fallback,
      ),
    );
    return [];
  }

  const matched: Array<string> = [];
  for (const key of keys) {
    const asValue = Number(key);
    if (key.trim() !== "" && !Number.isNaN(asValue) && asValue === numeric) matched.push(key);
  }

  const select = resolved.options.get("select");
  if (select === "exact") return matched;

  const category = pluralRules(context, select === "ordinal" ? "ordinal" : "cardinal").select(
    numeric,
  );
  for (const key of keys) {
    if (key === category && !matched.includes(key)) matched.push(key);
  }
  return matched;
}

/**
 * MF2's variant selection, in the order the specification gives it.
 *
 * Written as three passes — resolve the preferences, filter, sort — rather
 * than the obvious single scan for the best row, because with two selectors
 * "best" is not a property of a row on its own. `.match $count $gender` with
 * `one female`, `one *`, `* female` and `* *` has to prefer `one female`, then
 * `one *`, then `* female`: the *later* selector breaks ties within the
 * earlier one, which is why the sort runs from the last selector to the first.
 * A single scan gets that wrong in a way that only shows up in the messages
 * with two selectors, which are the ones nobody writes a test for.
 */
function selectVariant(
  selectors: $ReadOnlyArray<Resolved>,
  variants: $ReadOnlyArray<MessageVariant>,
  context: FormatContext,
): MessageVariant {
  const preferences: Array<$ReadOnlyArray<string>> = [];
  for (let index = 0; index < selectors.length; index += 1) {
    const keys: Array<string> = [];
    for (const variant of variants) {
      const key = variant.keys[index];
      if (key != null && key.kind === "literal" && !keys.includes(key.value)) keys.push(key.value);
    }
    preferences.push(selectKeys(selectors[index], keys, context));
  }

  let remaining: $ReadOnlyArray<MessageVariant> = variants;
  for (let index = selectors.length - 1; index >= 0; index -= 1) {
    const matched = preferences[index];
    remaining = remaining.filter((variant) => {
      const key = variant.keys[index];
      return key == null || key.kind === "catch-all" || matched.includes(key.value);
    });
  }

  let sorted: Array<MessageVariant> = remaining.slice();
  for (let index = selectors.length - 1; index >= 0; index -= 1) {
    const matched = preferences[index];
    // A catch-all sorts after every literal that matched, which is what makes
    // `*` the last resort rather than a tie with the worst real match.
    const rank = (variant: MessageVariant): number => {
      const key = variant.keys[index];
      if (key == null || key.kind === "catch-all") return matched.length;
      const found = matched.indexOf(key.value);
      return found < 0 ? matched.length : found;
    };
    // `sort` is stable in every runtime uf targets, which is load-bearing:
    // the pass for selector *i* must not disturb the order the pass for
    // selector *i + 1* established.
    sorted = sorted.sort((left, right) => rank(left) - rank(right));
  }

  const winner = sorted[0];
  if (winner == null) {
    // `syntax.js` refuses a matcher with no catch-all, so this is unreachable
    // from a parsed message and is kept as an assertion rather than removed:
    // the day a caller builds a `MessageNode` by hand, it should say so.
    throw new MessageFormatError("no variant matched and the message has no * variant", "");
  }
  return winner;
}

const ISOLATE_FIRST = "⁨";
const ISOLATE_POP = "⁩";

function formatPattern(
  pattern: MessagePattern,
  scope: Scope,
  args: Map<string, mixed>,
  context: FormatContext,
): string {
  let out = "";
  for (const part of pattern) {
    if (part.kind === "text") {
      out += part.value;
      continue;
    }
    const text = formatValue(resolveExpression(part, scope, args, context), context);
    out += context.bidiIsolation ? ISOLATE_FIRST + text + ISOLATE_POP : text;
  }
  return out;
}

/** The argument object as a `Map`. See the module header on why. */
function argumentsOf(args: mixed): Map<string, mixed> {
  if (args == null || typeof args !== "object") return new Map();
  return new Map(Object.entries(args));
}

/**
 * Format one message.
 *
 * The declarations run first and in order, because a `.local` may read what an
 * earlier one produced; the body then sees a scope in which every declared
 * name is already annotated.
 */
export function formatMessage(node: MessageNode, args: mixed, context: FormatContext): string {
  const bag = argumentsOf(args);
  const scope: Scope = new Map();
  for (const declaration of node.declarations) {
    scope.set(declaration.name, resolveExpression(declaration.expression, scope, bag, context));
  }

  const body = node.body;
  if (body.kind === "pattern") return formatPattern(body.pattern, scope, bag, context);

  const selectors = body.selectors.map((selector) => {
    const declared = scope.get(selector.name);
    if (declared != null) return declared;
    // A selector that no declaration annotated is still resolvable — `.match
    // $count` with a plain number argument is the common shape — so it is
    // resolved here as a bare variable expression rather than refused.
    return resolveExpression(
      { kind: "expression", operand: selector, annotation: null, at: 0 },
      scope,
      bag,
      context,
    );
  });

  return formatPattern(
    selectVariant(selectors, body.variants, context).pattern,
    scope,
    bag,
    context,
  );
}
