// @flow
//
// `@uniflowed/core/temporal`: Temporal, on a host that has it and on one that
// does not.
//
// `Date` is the wrong thing to build a date component on, and it is worth being
// precise about why rather than repeating that it is old. It is mutable, so a
// value handed to a component can be changed by the component. It has one time
// zone — the host's — so a server in UTC and a reader in Tokyo cannot both be
// described by one object, which is exactly the pair a hydrating page consists
// of. It cannot represent "a date with no time" at all, so "the 4th of
// September" becomes midnight somewhere, and which midnight depends on the
// machine. And its arithmetic is milliseconds, so "one month later" is a
// question it cannot answer.
//
// Every one of those is a wrong date waiting for a reader in the wrong zone.
// Temporal fixes them in the type: an `Instant` is a point in time and carries
// no zone, a `ZonedDateTime` carries the zone explicitly, and a `PlainDate` is a
// date that never had a time to lose. So `@uniflowed/web`'s `Time` is built on
// Temporal, and this module is what makes Temporal something a uf application
// can rely on today rather than something it waits for.
//
// # What "Lite" means
//
// `globalThis.Temporal` is used when the host has it. That is not deference for
// its own sake — a native implementation has the full calendar and time-zone
// database behind it, and nothing written here could match it. Where it is
// absent, this module supplies the five types `@uniflowed/temporal` has always
// named — `Instant`, `ZonedDateTime`, `PlainDate`, `PlainTime` and `Duration` —
// implemented over `Intl.DateTimeFormat`, which is where a JavaScript host keeps
// its zone data whether or not it has Temporal.
//
// The subset is the contract. Code written against what is documented here runs
// unchanged on both, because everything here is spec-shaped and native Temporal
// is a superset of it. What is deliberately *not* here, so that nobody
// discovers it by having it throw:
//
//   - **Non-ISO calendars.** Japanese, Hebrew and Islamic dates need the CLDR
//     calendar tables, which are the reason the native implementation exists.
//     Lite is ISO 8601 only, and `@uniflowed/temporal` raises
//     `NativeRuntimeRequiredError` for the calendar surface rather than
//     pretending — the same contract every other uf declaration keeps.
//   - **Nanoseconds.** The seam in `@uniflowed/core/clock` is milliseconds,
//     because that is what a host can actually promise, so `epochNanoseconds`
//     is absent rather than offered as a number with three invented zeros.
//   - **`round`, `with`, `PlainDateTime`, `PlainYearMonth`, `PlainMonthDay`.**
//     Nothing uf ships needs them, and a half-implemented calendar type is
//     worse than an absent one: absent is a name error at build time, and
//     half-implemented is a wrong date in production.
//
// # `Now` is uf's, always
//
// The one place this module does not defer to the host is the clock. Native
// `Temporal.Now` reads the system clock, and a `Temporal.Now.instant()` inside a
// render is the bug this whole area exists to fix — the server's instant and the
// browser's are different, and React compares the markup. So `Now` here reads
// `@uniflowed/core/clock`, whichever implementation is underneath it, and a test
// or a server can move it. Deferring for `Now` would have made the correctness
// of a page depend on whether the browser had shipped Temporal yet, which is the
// opposite of what a polyfill is for.

import { currentClock } from "./clock.js";

// Interfaces rather than object types, and the reason is the one Flow gives
// when it refuses the other spelling: a class instance is not a subtype of an
// object type. The Lite implementations below are classes, native Temporal's
// values are classes, and an interface is the only shape both can satisfy —
// which is exactly what these five are for.

/** A point in time, with no zone and no calendar. */
export interface Instant {
  readonly epochMilliseconds: number;
  toZonedDateTimeISO(timeZone: string): ZonedDateTime;
  add(duration: Duration | DurationLike | string): Instant;
  subtract(duration: Duration | DurationLike | string): Instant;
  since(other: Instant): Duration;
  until(other: Instant): Duration;
  equals(other: Instant): boolean;
  toString(): string;
  toJSON(): string;
}

/** A date and a time in a named zone: what a reader actually sees on a clock. */
export interface ZonedDateTime {
  readonly epochMilliseconds: number;
  readonly timeZoneId: string;
  readonly year: number;
  readonly month: number;
  readonly day: number;
  readonly hour: number;
  readonly minute: number;
  readonly second: number;
  readonly millisecond: number;
  readonly dayOfWeek: number;
  readonly offset: string;
  toInstant(): Instant;
  toPlainDate(): PlainDate;
  toPlainTime(): PlainTime;
  toString(): string;
  toJSON(): string;
  toLocaleString(locales?: string, options?: DateTimeFormatOptions): string;
  equals(other: ZonedDateTime): boolean;
}

/** A calendar date that never had a time to lose. */
export interface PlainDate {
  readonly year: number;
  readonly month: number;
  readonly day: number;
  add(duration: Duration | DurationLike | string): PlainDate;
  subtract(duration: Duration | DurationLike | string): PlainDate;
  equals(other: PlainDate): boolean;
  toString(): string;
  toJSON(): string;
}

/** A wall-clock time with no date attached. */
export interface PlainTime {
  readonly hour: number;
  readonly minute: number;
  readonly second: number;
  readonly millisecond: number;
  equals(other: PlainTime): boolean;
  toString(): string;
  toJSON(): string;
}

/** A length of time, in the units it was written in. */
export interface Duration {
  readonly years: number;
  readonly months: number;
  readonly weeks: number;
  readonly days: number;
  readonly hours: number;
  readonly minutes: number;
  readonly seconds: number;
  readonly milliseconds: number;
  readonly blank: boolean;
  negated(): Duration;
  abs(): Duration;
  total(options: { readonly unit: string, ... }): number;
  toString(): string;
  toJSON(): string;
}

/** What a `Duration` can be written as at a call site. */
export type DurationLike = {
  readonly years?: number,
  readonly months?: number,
  readonly weeks?: number,
  readonly days?: number,
  readonly hours?: number,
  readonly minutes?: number,
  readonly seconds?: number,
  readonly milliseconds?: number,
};

/**
 * The `Intl.DateTimeFormat` options this module passes and forwards.
 *
 * Flow's vendored `intl.js` declares no `Intl$DateTimeFormatOptions` at all, so
 * the name an editor suggests does not resolve. Declared here as the keys that
 * are used, plus the ones a caller of `toLocaleString` reasonably passes on.
 *
 * Every property is `readonly`, which is not decoration. An optional property
 * that is writable is invariant, so an object built by spreading a caller's
 * options would be rejected for having fewer keys than the type names — and the
 * only way to satisfy that is to write all fourteen at every call site.
 */
export type DateTimeFormatOptions = {
  readonly timeZone?: string,
  readonly timeZoneName?: "short" | "long" | "shortOffset" | "longOffset",
  readonly calendar?: string,
  readonly dateStyle?: "full" | "long" | "medium" | "short",
  readonly timeStyle?: "full" | "long" | "medium" | "short",
  readonly weekday?: "narrow" | "short" | "long",
  readonly era?: "narrow" | "short" | "long",
  readonly year?: "numeric" | "2-digit",
  readonly month?: "numeric" | "2-digit" | "narrow" | "short" | "long",
  readonly day?: "numeric" | "2-digit",
  readonly hour?: "numeric" | "2-digit",
  readonly minute?: "numeric" | "2-digit",
  readonly second?: "numeric" | "2-digit",
  readonly hour12?: boolean,
  readonly hourCycle?: "h11" | "h12" | "h23" | "h24",
  ...
};

/** The part of `Intl` this module uses, declared because the libdef stops short. */
declare class DateTimeFormat {
  constructor(locales?: string | void, options?: DateTimeFormatOptions): void;
  format(value: number): string;
  formatToParts(value: number): Array<{ type: string, value: string, ... }>;
}

declare var Intl: {
  DateTimeFormat?: Class<DateTimeFormat>,
  ...
};

// ---------------------------------------------------------------------------
// Fixed-length units
// ---------------------------------------------------------------------------

const MILLIS_PER_SECOND = 1_000;
const MILLIS_PER_MINUTE = 60_000;
const MILLIS_PER_HOUR = 3_600_000;
const MILLIS_PER_DAY = 86_400_000;
const MILLIS_PER_WEEK = 604_800_000;

/**
 * Milliseconds in one `unit`, or null where the unit has no fixed length.
 *
 * A function rather than a lookup table, so that "month" is a `null` a caller
 * can be told about rather than an `undefined` that becomes `NaN` four lines
 * later. Both the singular and the plural spelling, because Temporal accepts
 * both and a caller who writes the wrong one has made no mistake.
 */
function millisIn(unit: string): number | null {
  switch (unit) {
    case "week":
    case "weeks":
      return MILLIS_PER_WEEK;
    case "day":
    case "days":
      return MILLIS_PER_DAY;
    case "hour":
    case "hours":
      return MILLIS_PER_HOUR;
    case "minute":
    case "minutes":
      return MILLIS_PER_MINUTE;
    case "second":
    case "seconds":
      return MILLIS_PER_SECOND;
    case "millisecond":
    case "milliseconds":
      return 1;
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// Zone arithmetic
// ---------------------------------------------------------------------------

/** The wall-clock fields a zone shows at one instant. */
type Parts = {
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number,
  second: number,
};

/**
 * One formatter per zone, kept.
 *
 * Constructing an `Intl.DateTimeFormat` is the expensive part of everything
 * below — it loads the zone's transition table — and a list rendering a hundred
 * timestamps would construct a hundred identical ones. Keyed by zone alone,
 * because every formatter built here is built with the same options.
 */
const formatters: Map<string, DateTimeFormat> = new Map();

/** `Date.UTC` without the two-digit-year rule, which is a silent 1900-year error. */
function utcOf(
  year: number,
  month: number,
  day: number,
  hour: number,
  minute: number,
  second: number,
  millisecond: number,
): number {
  const at = new Date(0);
  at.setUTCFullYear(year, month - 1, day);
  at.setUTCHours(hour, minute, second, millisecond);
  return at.getTime();
}

/** The wall-clock fields `timeZone` shows at `epochMilliseconds`. */
function partsIn(epochMilliseconds: number, timeZone: string): Parts {
  if (timeZone === "UTC") {
    // Answered without `Intl` on purpose. UTC is the zone a server renders in
    // and the one a test asks for, and a host with no ICU — a stripped
    // container, a worker built for size — should still be able to render a
    // date rather than failing on the one zone that needs no data at all.
    const at = new Date(epochMilliseconds);
    return {
      year: at.getUTCFullYear(),
      month: at.getUTCMonth() + 1,
      day: at.getUTCDate(),
      hour: at.getUTCHours(),
      minute: at.getUTCMinutes(),
      second: at.getUTCSeconds(),
    };
  }

  const Formatter = Intl.DateTimeFormat;
  if (Formatter == null) {
    throw new RangeError(
      `@uniflowed/core/temporal: this host has no Intl.DateTimeFormat, so it cannot ` +
        `resolve ${timeZone}. Only UTC is available here.`,
    );
  }
  let formatter = formatters.get(timeZone);
  if (formatter == null) {
    formatter = new Formatter("en-US", {
      timeZone,
      era: "short",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      // `h23` rather than `hour12: false`, which some engines still answer with
      // "24" for midnight — a value `Date.UTC` reads as the following day.
      hourCycle: "h23",
    });
    formatters.set(timeZone, formatter);
  }

  const found: { [string]: string } = {};
  for (const part of formatter.formatToParts(epochMilliseconds)) {
    found[part.type] = part.value;
  }
  const year = Number(found.year);
  return {
    // ISO 8601 has a year zero and the Gregorian calendar does not, so 1 BC is
    // ISO year 0 and 2 BC is ISO year -1. Without this a date before the common
    // era formats as its own mirror image.
    year: found.era === "B" || found.era === "BC" ? 1 - year : year,
    month: Number(found.month),
    day: Number(found.day),
    hour: Number(found.hour),
    minute: Number(found.minute),
    second: Number(found.second),
  };
}

/** How far `timeZone` is ahead of UTC at `epochMilliseconds`, in milliseconds. */
function offsetMillisIn(epochMilliseconds: number, timeZone: string): number {
  if (timeZone === "UTC") {
    return 0;
  }
  const parts = partsIn(epochMilliseconds, timeZone);
  const wall = utcOf(parts.year, parts.month, parts.day, parts.hour, parts.minute, parts.second, 0);
  // Compared against the instant floored to a second, because the formatter has
  // no sub-second field: a zone offset by half an hour would otherwise come out
  // with the instant's own milliseconds folded into it.
  return wall - Math.floor(epochMilliseconds / MILLIS_PER_SECOND) * MILLIS_PER_SECOND;
}

/** `value`, left-padded with zeros to `width`, sign dropped. */
function pad(value: number, width: number): string {
  return String(Math.abs(value)).padStart(width, "0");
}

/** `+09:00`, or `-03:30`, from an offset in milliseconds. */
function offsetText(offsetMillis: number): string {
  const sign = offsetMillis < 0 ? "-" : "+";
  const minutes = Math.abs(Math.trunc(offsetMillis / MILLIS_PER_MINUTE));
  return `${sign}${pad(Math.trunc(minutes / 60), 2)}:${pad(minutes % 60, 2)}`;
}

/** An ISO year, which is signed and six digits wide outside 0000-9999. */
function yearText(year: number): string {
  if (year >= 0 && year <= 9999) {
    return pad(year, 4);
  }
  return `${year < 0 ? "-" : "+"}${pad(year, 6)}`;
}

/** Days in `month` of `year`, Gregorian. */
function daysInMonth(year: number, month: number): number {
  const start = utcOf(year, month, 1, 0, 0, 0, 0);
  const next = utcOf(year, month + 1, 1, 0, 0, 0, 0);
  return Math.round((next - start) / MILLIS_PER_DAY);
}

// ---------------------------------------------------------------------------
// The five types
// ---------------------------------------------------------------------------

/** How a `Duration` is carried: every field, exactly as it was written. */
type DurationFields = {
  years: number,
  months: number,
  weeks: number,
  days: number,
  hours: number,
  minutes: number,
  seconds: number,
  milliseconds: number,
};

/**
 * A length of time, kept in the units it was written in.
 *
 * Unbalanced on purpose, which is what Temporal itself does: `PT90M` is ninety
 * minutes and stays ninety minutes, because a duration that silently became an
 * hour and a half would print as something the caller did not write.
 */
class LiteDuration {
  readonly years: number;
  readonly months: number;
  readonly weeks: number;
  readonly days: number;
  readonly hours: number;
  readonly minutes: number;
  readonly seconds: number;
  readonly milliseconds: number;
  /** Whether every field is zero. A field rather than a getter: it never changes. */
  readonly blank: boolean;

  constructor(fields: DurationFields) {
    this.years = fields.years;
    this.months = fields.months;
    this.weeks = fields.weeks;
    this.days = fields.days;
    this.hours = fields.hours;
    this.minutes = fields.minutes;
    this.seconds = fields.seconds;
    this.milliseconds = fields.milliseconds;
    this.blank =
      fields.years === 0 &&
      fields.months === 0 &&
      fields.weeks === 0 &&
      fields.days === 0 &&
      fields.hours === 0 &&
      fields.minutes === 0 &&
      fields.seconds === 0 &&
      fields.milliseconds === 0;
  }

  static from(value: Duration | DurationLike | string): LiteDuration {
    if (value instanceof LiteDuration) {
      return value;
    }
    if (typeof value === "string") {
      return parseDuration(value);
    }
    return new LiteDuration({
      years: value.years ?? 0,
      months: value.months ?? 0,
      weeks: value.weeks ?? 0,
      days: value.days ?? 0,
      hours: value.hours ?? 0,
      minutes: value.minutes ?? 0,
      seconds: value.seconds ?? 0,
      milliseconds: value.milliseconds ?? 0,
    });
  }

  /** Everything with a fixed length, in milliseconds. Weeks count; months do not. */
  exactMillis(): number {
    return (
      this.weeks * MILLIS_PER_WEEK +
      this.days * MILLIS_PER_DAY +
      this.hours * MILLIS_PER_HOUR +
      this.minutes * MILLIS_PER_MINUTE +
      this.seconds * MILLIS_PER_SECOND +
      this.milliseconds
    );
  }

  negated(): LiteDuration {
    return new LiteDuration({
      years: -this.years,
      months: -this.months,
      weeks: -this.weeks,
      days: -this.days,
      hours: -this.hours,
      minutes: -this.minutes,
      seconds: -this.seconds,
      milliseconds: -this.milliseconds,
    });
  }

  abs(): LiteDuration {
    return this.sign() < 0 ? this.negated() : this;
  }

  /** -1, 0 or 1, decided by the largest field that is not zero. */
  sign(): number {
    const fields = [
      this.years,
      this.months,
      this.weeks,
      this.days,
      this.hours,
      this.minutes,
      this.seconds,
      this.milliseconds,
    ];
    for (const value of fields) {
      if (value !== 0) {
        return value < 0 ? -1 : 1;
      }
    }
    return 0;
  }

  /**
   * The whole duration expressed in one unit.
   *
   * Years and months are refused rather than approximated, because their length
   * depends on which year and which month — Temporal refuses them here too,
   * asking for a `relativeTo` this implementation does not have. Approximating
   * would give "1 month" a value that is right for April and wrong for
   * February, silently, in a component nobody would think to check.
   */
  total(options: { readonly unit: string, ... }): number {
    if (this.years !== 0 || this.months !== 0) {
      throw new RangeError(
        "@uniflowed/core/temporal: a duration with years or months has no fixed length, " +
          "so it cannot be totalled without a reference date.",
      );
    }
    const scale = millisIn(options.unit);
    if (scale == null) {
      throw new RangeError(`@uniflowed/core/temporal: ${options.unit} is not a fixed-length unit`);
    }
    return this.exactMillis() / scale;
  }

  toString(): string {
    if (this.blank) {
      return "PT0S";
    }
    const negative = this.sign() < 0;
    const at = negative ? this.negated() : this;
    let out = "P";
    if (at.years !== 0) out += `${at.years}Y`;
    if (at.months !== 0) out += `${at.months}M`;
    if (at.weeks !== 0) out += `${at.weeks}W`;
    if (at.days !== 0) out += `${at.days}D`;
    const seconds = at.seconds + at.milliseconds / MILLIS_PER_SECOND;
    if (at.hours !== 0 || at.minutes !== 0 || seconds !== 0) {
      out += "T";
      if (at.hours !== 0) out += `${at.hours}H`;
      if (at.minutes !== 0) out += `${at.minutes}M`;
      if (seconds !== 0) out += `${seconds}S`;
    }
    return negative ? `-${out}` : out;
  }

  toJSON(): string {
    return this.toString();
  }
}

/** `P3DT4H`, and the negative and fractional-second forms of it. */
const DURATION_PATTERN =
  /^([+-])?P(?:(\d+)Y)?(?:(\d+)M)?(?:(\d+)W)?(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+(?:\.\d+)?)S)?)?$/;

function parseDuration(value: string): LiteDuration {
  const found = DURATION_PATTERN.exec(value);
  if (found == null || value === "P" || value === "-P") {
    throw new RangeError(`@uniflowed/core/temporal: ${value} is not an ISO 8601 duration`);
  }
  const seconds = Number(found[8] ?? 0);
  const whole = Math.trunc(seconds);
  const made = new LiteDuration({
    years: Number(found[2] ?? 0),
    months: Number(found[3] ?? 0),
    weeks: Number(found[4] ?? 0),
    days: Number(found[5] ?? 0),
    hours: Number(found[6] ?? 0),
    minutes: Number(found[7] ?? 0),
    seconds: whole,
    // Rounded rather than truncated: `PT0.3S` is 300ms, and the subtraction
    // that isolates the fraction lands on 299.99999999999994 before it is one.
    milliseconds: Math.round((seconds - whole) * MILLIS_PER_SECOND),
  });
  return found[1] === "-" ? made.negated() : made;
}

/** The fixed-length part of a duration, refusing the units that have no length. */
function exactMillisOf(duration: Duration | DurationLike | string): number {
  const made = LiteDuration.from(duration);
  if (made.years !== 0 || made.months !== 0) {
    throw new RangeError(
      "@uniflowed/core/temporal: years and months cannot be added to an instant, whose " +
        "arithmetic is exact. Convert to a ZonedDateTime first.",
    );
  }
  return made.exactMillis();
}

/** `2026-09-04T06:00:00Z`, and the offset and fractional forms of it. */
const INSTANT_PATTERN =
  /^([+-]\d{6}|\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?(Z|[+-]\d{2}:?\d{2})$/i;

/** A point in time. No zone, no calendar, nothing to disagree about. */
class LiteInstant {
  readonly epochMilliseconds: number;

  constructor(epochMilliseconds: number) {
    if (!Number.isFinite(epochMilliseconds)) {
      throw new RangeError("@uniflowed/core/temporal: an instant must be a finite ms count");
    }
    this.epochMilliseconds = Math.trunc(epochMilliseconds);
  }

  /**
   * Parse an instant, refusing what Temporal refuses.
   *
   * The pattern is checked before `Date.parse` rather than left to it, because
   * `Date.parse` accepts `"2026-09-04"` — a date with no zone, which it reads as
   * UTC midnight and a browser once read as local midnight. Being lenient here
   * where Temporal is strict would mean code that works on Lite throws on a host
   * that has the real thing, which is the one failure this module exists to
   * prevent.
   */
  static from(value: Instant | string): LiteInstant {
    if (typeof value !== "string") {
      return new LiteInstant(value.epochMilliseconds);
    }
    if (!INSTANT_PATTERN.test(value)) {
      throw new RangeError(
        `@uniflowed/core/temporal: ${value} is not an instant. An instant needs a date, a ` +
          `time and an offset, as in 2026-09-04T06:00:00Z.`,
      );
    }
    const at = Date.parse(value);
    if (Number.isNaN(at)) {
      throw new RangeError(`@uniflowed/core/temporal: ${value} is not an instant`);
    }
    return new LiteInstant(at);
  }

  static fromEpochMilliseconds(epochMilliseconds: number): LiteInstant {
    return new LiteInstant(epochMilliseconds);
  }

  static compare(a: Instant, b: Instant): number {
    if (a.epochMilliseconds === b.epochMilliseconds) return 0;
    return a.epochMilliseconds < b.epochMilliseconds ? -1 : 1;
  }

  toZonedDateTimeISO(timeZone: string): LiteZonedDateTime {
    return new LiteZonedDateTime(this.epochMilliseconds, timeZone);
  }

  add(duration: Duration | DurationLike | string): LiteInstant {
    return new LiteInstant(this.epochMilliseconds + exactMillisOf(duration));
  }

  subtract(duration: Duration | DurationLike | string): LiteInstant {
    return new LiteInstant(this.epochMilliseconds - exactMillisOf(duration));
  }

  /** How long since `other`; negative when `other` is later. */
  since(other: Instant): LiteDuration {
    return durationOfMillis(this.epochMilliseconds - other.epochMilliseconds);
  }

  /** How long until `other`; negative when `other` is earlier. */
  until(other: Instant): LiteDuration {
    return durationOfMillis(other.epochMilliseconds - this.epochMilliseconds);
  }

  equals(other: Instant): boolean {
    return this.epochMilliseconds === other.epochMilliseconds;
  }

  /**
   * `2026-09-04T06:00:00Z`, and `2026-09-04T06:00:00.250Z` when there is a
   * fraction.
   *
   * `Date.prototype.toISOString` would be one line and would always print
   * `.000`, which native Temporal does not — and a text difference between the
   * two implementations is the one bug this module cannot have, because it
   * would only appear on a browser that had shipped Temporal.
   */
  toString(): string {
    const iso = new Date(this.epochMilliseconds).toISOString();
    return iso.endsWith(".000Z") ? `${iso.slice(0, -5)}Z` : iso;
  }

  toJSON(): string {
    return this.toString();
  }

  /**
   * Refused, exactly as Temporal refuses it.
   *
   * Without this, `a < b` coerces both to a primitive and compares the *text*,
   * which is right for two instants in the same century and wrong the moment one
   * of them crosses a digit. `Instant.compare` is the operation that means what
   * the caller meant.
   */
  valueOf(): empty {
    throw new TypeError(
      "@uniflowed/core/temporal: instants cannot be compared with < or >; use Instant.compare",
    );
  }
}

/** `since`/`until`'s answer: seconds and milliseconds, which is Temporal's own shape. */
function durationOfMillis(millis: number): LiteDuration {
  const whole = Math.trunc(millis / MILLIS_PER_SECOND);
  return new LiteDuration({
    years: 0,
    months: 0,
    weeks: 0,
    days: 0,
    hours: 0,
    minutes: 0,
    seconds: whole,
    milliseconds: millis - whole * MILLIS_PER_SECOND,
  });
}

/** `2026-09-04T15:00:00+09:00[Asia/Tokyo]`. */
const ZONED_PATTERN = /^(.*)\[([^\]]+)\]$/;

/** A date and a time in a named zone. */
class LiteZonedDateTime {
  readonly epochMilliseconds: number;
  readonly timeZoneId: string;
  readonly year: number;
  readonly month: number;
  readonly day: number;
  readonly hour: number;
  readonly minute: number;
  readonly second: number;
  readonly millisecond: number;
  readonly dayOfWeek: number;
  readonly offset: string;

  /**
   * Every field is computed here rather than behind a getter.
   *
   * Each one costs an `Intl.DateTimeFormat` call, and a component reading four
   * of them would otherwise pay for four — for the same instant, in the same
   * zone, in the same render.
   */
  constructor(epochMilliseconds: number, timeZone: string) {
    const at = Math.trunc(epochMilliseconds);
    const parts = partsIn(at, timeZone);
    this.epochMilliseconds = at;
    this.timeZoneId = timeZone;
    this.year = parts.year;
    this.month = parts.month;
    this.day = parts.day;
    this.hour = parts.hour;
    this.minute = parts.minute;
    this.second = parts.second;
    // Adjusted so that an instant before the epoch has a millisecond field in
    // [0, 1000) rather than a negative one.
    this.millisecond = ((at % MILLIS_PER_SECOND) + MILLIS_PER_SECOND) % MILLIS_PER_SECOND;
    this.offset = offsetText(offsetMillisIn(at, timeZone));
    // 1 for Monday through 7 for Sunday, which is ISO 8601's numbering and not
    // `Date`'s. Taken from the zone's own calendar date rather than from the
    // instant, or a Tokyo morning is still the previous day.
    const weekday = new Date(utcOf(parts.year, parts.month, parts.day, 0, 0, 0, 0)).getUTCDay();
    this.dayOfWeek = weekday === 0 ? 7 : weekday;
  }

  static from(value: ZonedDateTime | string): LiteZonedDateTime {
    if (typeof value !== "string") {
      return new LiteZonedDateTime(value.epochMilliseconds, value.timeZoneId);
    }
    const found = ZONED_PATTERN.exec(value);
    if (found == null) {
      throw new RangeError(
        `@uniflowed/core/temporal: ${value} names no time zone. A ZonedDateTime is written ` +
          `2026-09-04T15:00:00+09:00[Asia/Tokyo].`,
      );
    }
    return new LiteZonedDateTime(LiteInstant.from(found[1]).epochMilliseconds, found[2]);
  }

  static compare(a: ZonedDateTime, b: ZonedDateTime): number {
    if (a.epochMilliseconds === b.epochMilliseconds) return 0;
    return a.epochMilliseconds < b.epochMilliseconds ? -1 : 1;
  }

  toInstant(): LiteInstant {
    return new LiteInstant(this.epochMilliseconds);
  }

  toPlainDate(): LitePlainDate {
    return new LitePlainDate(this.year, this.month, this.day);
  }

  toPlainTime(): LitePlainTime {
    return new LitePlainTime(this.hour, this.minute, this.second, this.millisecond);
  }

  equals(other: ZonedDateTime): boolean {
    return (
      this.epochMilliseconds === other.epochMilliseconds && this.timeZoneId === other.timeZoneId
    );
  }

  /** `2026-09-04T15:00:00+09:00[Asia/Tokyo]`, which is Temporal's own form. */
  toString(): string {
    const date = `${yearText(this.year)}-${pad(this.month, 2)}-${pad(this.day, 2)}`;
    const time = `${pad(this.hour, 2)}:${pad(this.minute, 2)}:${pad(this.second, 2)}`;
    const fraction = this.millisecond === 0 ? "" : `.${pad(this.millisecond, 3)}`;
    return `${date}T${time}${fraction}${this.offset}[${this.timeZoneId}]`;
  }

  toJSON(): string {
    return this.toString();
  }

  /**
   * The reader's own format for this instant, in this zone.
   *
   * The zone is supplied rather than left out, which is the whole difference
   * between this and `Date.prototype.toLocaleString`: a `ZonedDateTime` knows
   * which zone it is in, so a caller who omits `timeZone` gets that one instead
   * of whichever zone the machine happens to be set to.
   */
  toLocaleString(locales?: string, options?: DateTimeFormatOptions): string {
    const Formatter = Intl.DateTimeFormat;
    if (Formatter == null) {
      return this.toString();
    }
    const settings = { ...options, timeZone: options?.timeZone ?? this.timeZoneId };
    return new Formatter(locales, settings).format(this.epochMilliseconds);
  }
}

/** `2026-09-04`, and the six-digit-year form of it. */
const DATE_PATTERN = /^([+-]\d{6}|\d{4})-(\d{2})-(\d{2})/;

/** A calendar date. */
class LitePlainDate {
  readonly year: number;
  readonly month: number;
  readonly day: number;

  constructor(year: number, month: number, day: number) {
    this.year = year;
    this.month = month;
    this.day = day;
  }

  static from(
    value: PlainDate | string | { year: number, month: number, day: number, ... },
  ): LitePlainDate {
    if (typeof value === "string") {
      const found = DATE_PATTERN.exec(value);
      if (found == null) {
        throw new RangeError(`@uniflowed/core/temporal: ${value} is not a date`);
      }
      return new LitePlainDate(Number(found[1]), Number(found[2]), Number(found[3]));
    }
    return new LitePlainDate(value.year, value.month, value.day);
  }

  static compare(a: PlainDate, b: PlainDate): number {
    const left = a.toString();
    const right = b.toString();
    if (left === right) return 0;
    return left < right ? -1 : 1;
  }

  /**
   * `duration` later.
   *
   * Months are added before days, and the day is clamped to the length of the
   * month it lands in — so the 31st of January plus one month is the 28th of
   * February rather than the 3rd of March. That is Temporal's default
   * `constrain` overflow, and it is what a person means; rolling over is how
   * "monthly, on the 31st" quietly becomes March.
   */
  add(duration: Duration | DurationLike | string): LitePlainDate {
    return this.shifted(LiteDuration.from(duration));
  }

  subtract(duration: Duration | DurationLike | string): LitePlainDate {
    return this.shifted(LiteDuration.from(duration).negated());
  }

  /** The arithmetic both of the two above are, once the sign has been decided. */
  shifted(by: LiteDuration): LitePlainDate {
    const months = this.year * 12 + (this.month - 1) + by.years * 12 + by.months;
    const year = Math.floor(months / 12);
    // Subtracted rather than `%`, which keeps the sign of its left operand and
    // would put a date before the year zero in month -3.
    const month = months - year * 12 + 1;
    const day = Math.min(this.day, daysInMonth(year, month));
    const exact =
      utcOf(year, month, day, 0, 0, 0, 0) + by.weeks * MILLIS_PER_WEEK + by.days * MILLIS_PER_DAY;
    const landed = new Date(exact);
    return new LitePlainDate(
      landed.getUTCFullYear(),
      landed.getUTCMonth() + 1,
      landed.getUTCDate(),
    );
  }

  equals(other: PlainDate): boolean {
    return this.year === other.year && this.month === other.month && this.day === other.day;
  }

  toString(): string {
    return `${yearText(this.year)}-${pad(this.month, 2)}-${pad(this.day, 2)}`;
  }

  toJSON(): string {
    return this.toString();
  }
}

/** `15:00`, `15:00:30`, `15:00:30.250`. */
const TIME_PATTERN = /^(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,3})\d*)?)?/;

/** A wall-clock time. */
class LitePlainTime {
  readonly hour: number;
  readonly minute: number;
  readonly second: number;
  readonly millisecond: number;

  constructor(hour: number, minute: number, second: number = 0, millisecond: number = 0) {
    this.hour = hour;
    this.minute = minute;
    this.second = second;
    this.millisecond = millisecond;
  }

  static from(value: PlainTime | string): LitePlainTime {
    if (typeof value !== "string") {
      return new LitePlainTime(value.hour, value.minute, value.second, value.millisecond);
    }
    const found = TIME_PATTERN.exec(value);
    if (found == null) {
      throw new RangeError(`@uniflowed/core/temporal: ${value} is not a time`);
    }
    return new LitePlainTime(
      Number(found[1]),
      Number(found[2]),
      Number(found[3] ?? 0),
      Number((found[4] ?? "0").padEnd(3, "0")),
    );
  }

  equals(other: PlainTime): boolean {
    return (
      this.hour === other.hour &&
      this.minute === other.minute &&
      this.second === other.second &&
      this.millisecond === other.millisecond
    );
  }

  toString(): string {
    const base = `${pad(this.hour, 2)}:${pad(this.minute, 2)}:${pad(this.second, 2)}`;
    return this.millisecond === 0 ? base : `${base}.${pad(this.millisecond, 3)}`;
  }

  toJSON(): string {
    return this.toString();
  }
}

// ---------------------------------------------------------------------------
// What the module exports
// ---------------------------------------------------------------------------

/** The clock, and what is read from it. */
export type TemporalNow = {
  instant(): Instant,
  timeZoneId(): string,
  zonedDateTimeISO(timeZone?: string): ZonedDateTime,
  plainDateISO(timeZone?: string): PlainDate,
  plainTimeISO(timeZone?: string): PlainTime,
};

/** The five constructors, without the clock hung off them. */
export type TemporalTypes = {
  readonly Instant: {
    from(value: Instant | string): Instant,
    fromEpochMilliseconds(epochMilliseconds: number): Instant,
    compare(a: Instant, b: Instant): number,
    ...
  },
  readonly ZonedDateTime: {
    from(value: ZonedDateTime | string): ZonedDateTime,
    compare(a: ZonedDateTime, b: ZonedDateTime): number,
    ...
  },
  readonly PlainDate: {
    from(value: PlainDate | string | { year: number, month: number, day: number, ... }): PlainDate,
    compare(a: PlainDate, b: PlainDate): number,
    ...
  },
  readonly PlainTime: { from(value: PlainTime | string): PlainTime, ... },
  readonly Duration: { from(value: Duration | DurationLike | string): Duration, ... },
  ...
};

/** The surface uf promises on every host. Native Temporal is a superset of it. */
export type TemporalApi = { ...TemporalTypes, readonly Now: TemporalNow, ... };

/**
 * `globalThis`, as far as this module is concerned.
 *
 * Declared rather than cast, because a cast to `any` to read one optional
 * property would turn off the checker for the one line where the question is
 * whether the property is there. Flow's library definitions have no Temporal
 * yet, and this says exactly what is being assumed about the host and nothing
 * more.
 */
declare var globalThis: { Temporal?: TemporalApi, ... };

/**
 * The host's Temporal, when it has one.
 *
 * Read once: a host either has it or does not, and re-reading a global would be
 * a lookup on the hot path of every date on the page. Three constructors are
 * probed rather than the object itself, because a partial polyfill somebody
 * installed is worse than no polyfill at all — this module would defer to it and
 * then call something it does not have.
 */
const host: TemporalApi | null = (() => {
  const found = globalThis.Temporal;
  if (found == null) {
    return null;
  }
  const complete =
    typeof found.Instant?.fromEpochMilliseconds === "function" &&
    typeof found.ZonedDateTime?.from === "function" &&
    typeof found.Duration?.from === "function";
  return complete ? found : null;
})();

/** Whether the five types come from this module rather than from the host. */
export const isLite: boolean = host == null;

/**
 * The clock, and everything derived from it, read from `@uniflowed/core/clock`.
 *
 * Built against whichever implementation is in use — `types.Instant` is the
 * host's where there is one — so `Now.instant()` is a native `Temporal.Instant`
 * on a host that has Temporal, and interoperates with everything else on it.
 */
function nowFor(types: TemporalTypes): TemporalNow {
  const zoned = (timeZone?: string): ZonedDateTime => {
    const clock = currentClock();
    const at = types.Instant.fromEpochMilliseconds(clock.now());
    return at.toZonedDateTimeISO(timeZone ?? clock.timeZone());
  };
  return {
    instant: () => types.Instant.fromEpochMilliseconds(currentClock().now()),
    timeZoneId: () => currentClock().timeZone(),
    zonedDateTimeISO: zoned,
    plainDateISO: (timeZone?: string) => zoned(timeZone).toPlainDate(),
    plainTimeISO: (timeZone?: string) => zoned(timeZone).toPlainTime(),
  };
}

/**
 * The Lite constructors, written out one entry at a time.
 *
 * The classes themselves would have been shorter, and this is not a workaround
 * for a checker complaint: what is written here is the surface this module
 * promises, so a method added to `LiteInstant` for its own internal reasons
 * cannot silently widen what uf claims works on a host without Temporal. The
 * classes are exported below for anyone who wants the whole of one.
 *
 * The cost is that `x instanceof Temporal.Instant` is false on a Lite host and
 * true on a native one. Nothing in the documented subset needs it — Temporal
 * values are compared with `equals` and `compare`, which is why they exist —
 * and it is named here rather than left to be found.
 */
const lite: TemporalTypes = {
  Instant: {
    from: (value) => LiteInstant.from(value),
    fromEpochMilliseconds: (epochMilliseconds) =>
      LiteInstant.fromEpochMilliseconds(epochMilliseconds),
    compare: (a, b) => LiteInstant.compare(a, b),
  },
  ZonedDateTime: {
    from: (value) => LiteZonedDateTime.from(value),
    compare: (a, b) => LiteZonedDateTime.compare(a, b),
  },
  PlainDate: {
    from: (value) => LitePlainDate.from(value),
    compare: (a, b) => LitePlainDate.compare(a, b),
  },
  PlainTime: { from: (value) => LitePlainTime.from(value) },
  Duration: { from: (value) => LiteDuration.from(value) },
};

/**
 * `Temporal`, on every host.
 *
 * Spread from the host's where there is one, so that everything this module has
 * not documented — `PlainDateTime`, `round`, the calendar surface — stays
 * reachable on a host that has it rather than being hidden by uf. `Now` is
 * replaced in both cases, for the reason at the top of this file.
 */
export const Temporal: TemporalApi = (() => {
  const types: TemporalTypes = host ?? lite;
  return { ...types, Now: nowFor(types) };
})();

export { LiteDuration, LiteInstant, LitePlainDate, LitePlainTime, LiteZonedDateTime };
