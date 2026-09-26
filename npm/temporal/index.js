// @flow
//
// `@uniflowed/temporal`: the name an application imports Temporal by.
//
// The implementation is `@uniflowed/core/temporal` and this file is the front
// door, which is not indirection: `uf_lib`'s registry promises the specifier
// `@uniflowed/temporal`, and where the code lives is a packaging question that
// an application should not have to know the answer to. It lives in
// `@uniflowed/core` because `@uniflowed/hooks` reads the same clock and is on
// npm, and a published package may not depend on one that is not.
//
// # What still needs the native runtime
//
// One thing, and it is not an oversight. Temporal's calendar surface — a
// Japanese era, a Hebrew month, an Islamic date — is the CLDR calendar tables,
// and those are megabytes of data that a JavaScript implementation would have
// to carry into every bundle to answer a question most applications never ask.
// uf's binary already has them.
//
// So `withCalendar` raises `NativeRuntimeRequiredError`, naming itself, which
// is what every uf declaration does for a binding that needs the binary. A
// caller who never mentions a calendar never reaches it: everything in
// `Temporal` below is ISO 8601, which is what a timestamp on a page is.
//
// That is also why this package is on the release bootstrap's pending list
// rather than private. The useful Temporal surface is implemented and should
// have its own installable name; the one non-ISO calendar seam remains an
// explicit native-runtime boundary instead of pretending to work.

import { nativeRuntimeRequired } from "@uniflowed/core/native";
import { Temporal as implementation, isLite } from "@uniflowed/core/temporal";

const MODULE = "@uniflowed/temporal";

export type {
  DateTimeFormatOptions,
  Duration,
  DurationLike,
  Instant,
  PlainDate,
  PlainTime,
  TemporalApi,
  TemporalNow,
  ZonedDateTime,
} from "@uniflowed/core/temporal";

/**
 * Whether the five types are uf's own rather than the host's.
 *
 * Worth exporting because it is the honest answer to "will this behave the same
 * everywhere": on a host with native Temporal an application gets the whole
 * standard, and on one without it gets the subset `@uniflowed/core/temporal`
 * documents. A page that needs to know can ask; most do not.
 */
export { isLite };

/** `Temporal`, with the clock uf injects. See `@uniflowed/core/temporal`. */
export const Temporal: typeof implementation = implementation;

/**
 * Re-express a date in a non-ISO calendar.
 *
 * The one binding here that the native runtime owns. It takes a calendar
 * identifier — `japanese`, `hebrew`, `islamic-umalqura` — and there is no
 * honest way to answer it from JavaScript without shipping the tables that
 * define those calendars.
 *
 * `toLocaleString` is not this, and is the thing most callers actually want: a
 * `ZonedDateTime` formatted with `{ calendar: "japanese" }` goes through `Intl`,
 * which has the tables already, and prints a Japanese era without any of this.
 * What is missing is *arithmetic* in another calendar — the next month in the
 * Hebrew year — and that is what waits for the binary.
 */
export function withCalendar<T>(value: T, calendar: string): T {
  return nativeRuntimeRequired(MODULE, "withCalendar");
}
