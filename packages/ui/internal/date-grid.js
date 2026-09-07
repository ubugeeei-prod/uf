// @flow
//
// The month a calendar shows, and the keyboard that walks it.
//
// # Why this is not `roving-focus.js`
//
// A date grid is a roving tab stop — one stop in the page's tab order, arrow
// keys inside it — so most of `internal/roving-focus.js` applies and the
// calendar uses it: `directionOf` for a right-to-left week, `isEnabled` nowhere,
// and the same rule about reading the document rather than a registry. What does
// not fit is the part that does the moving, and it does not fit for four
// reasons rather than one:
//
//   * **The movement is arithmetic on a date, not an index in a NodeList.**
//     `moveTo` walks the items it was handed. `ArrowDown` in a calendar is "a
//     week later", and a week later is often a cell that is not in the grid at
//     all yet — so the answer cannot be found among the elements, and a
//     `movementFor` grown to two axes would still return "next" for a key whose
//     real meaning is `+7 days`.
//   * **Running off the end changes what is rendered.** `ArrowRight` on the 31st
//     shows the next month *and* leaves focus on the 1st, which is a cell that
//     did not exist when the key was pressed. That is a `pendingFocus` problem,
//     the same shape `Menu.Body` solves for a menu opening onto its last item,
//     and it is why the movement is computed as a value the component can act on
//     over two renders rather than as a `.focus()` inside a helper.
//   * **An unavailable date stays focusable.** `moveTo` skips anything
//     `isEnabled` rejects, which is right for a menu item and wrong here: a
//     reader arrowing through October has to be able to pass over the days that
//     cannot be booked, each `aria-disabled="true"` and each still reachable. A
//     grid that skipped them would present a month with holes in it and no way
//     to find out what is in the holes.
//   * **There is no wrap, and no ends.** A list has a first and a last item.
//     A calendar has neither: every direction leads to another month.
//
// So the two primitives stay apart, and the boundary is that one owns *focus
// among elements that exist* and this one owns *which date the keyboard means*.
// Everything below is a pure function over `PlainDate` values: no element, no
// React, no document. That is deliberate for the same reason
// `internal/anchor.js` keeps `placeOverlay` pure — the month-boundary cases are
// the ones worth testing exhaustively, and testing them through a rendered grid
// would test the renderer instead.
//
// # Dates come from Temporal, and from `@uniflowed/core/temporal` in particular
//
// Not `Date`. A package that ships a temporal library and then computes a month
// length with `new Date(y, m + 1, 0)` is the opposite of what "build uf with uf"
// asks for, and `Date`'s month-is-zero-based, mutates-in-place, local-timezone
// arithmetic is where calendar bugs come from in the first place.
//
// The specifier is `@uniflowed/core/temporal` rather than `@uniflowed/temporal`,
// which is the same object — `packages/temporal/index.js` is a re-export and
// says so. It has to be that one: `@uniflowed/ui` is published to npm,
// `@uniflowed/temporal` is not yet (its calendar surface waits on the native
// runtime), and `tools/ci/publishable.sh` refuses a published package that
// depends on an unpublished one because `npm install` would answer `ETARGET`.
// When the name is published this import can move, and nothing else changes.

import type { PlainDate } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";

import type { Direction } from "./roving-focus.js";

/**
 * ISO 8601's first day of the week, which is Monday.
 *
 * The fallback when nothing better is known, and deliberately not Sunday: ISO is
 * the standard the rest of this package's date handling follows, and a default
 * that matched one large locale would be a guess dressed as a convention.
 */
export const ISO_WEEK_START: number = 1;

/** Days in a week, which is the width of every grid here. */
export const DAYS_IN_WEEK: number = 7;

/**
 * What a key asks the focused date to become.
 *
 * A value rather than a new date, because the component that acts on it has to
 * do two things with the answer — move the focus and possibly re-render a
 * different month — and because "PageDown means a month" is the part worth
 * asserting on its own.
 */
export type DateMovement =
  | {| readonly kind: "days", readonly by: number |}
  | {| readonly kind: "months", readonly by: number |}
  | {| readonly kind: "years", readonly by: number |}
  | {| readonly kind: "week-edge", readonly to: "start" | "end" |};

/** The part of a key event a grid reads. */
export type DateKeyPress = {
  readonly key: string,
  readonly shiftKey?: boolean,
  ...
};

/**
 * The movement a key asks for, or null when the key is not the grid's.
 *
 * The unhandled keys matter as much as the handled ones, for the reason
 * `movementFor` gives: `Tab` belongs to the page and `Escape` belongs to
 * whatever the calendar is inside, and a grid that swallowed either would be a
 * place a reader could not leave.
 *
 * `direction` mirrors the horizontal pair and nothing else. `ArrowDown` is a
 * week later in an Arabic calendar exactly as in an English one — a
 * right-to-left page still runs top to bottom — and `Home` and `End` name the
 * first and last day of the week in *reading* order, which is what
 * `weekEdge` walks.
 *
 * `Shift` turns the two page keys into years, which is the one keyboard
 * convention here that a reader cannot discover by trying: it is in the
 * WAI-ARIA date-picker pattern, every native date field has it, and a year is
 * otherwise twelve `PageDown` presses.
 */
export function movementForDateKey(event: DateKeyPress, direction: Direction): DateMovement | null {
  const forward = direction === "rtl" ? -1 : 1;
  const pages = event.shiftKey === true ? 12 : 1;
  return match (event.key) {
    "ArrowRight" => { kind: "days", by: forward },
    "ArrowLeft" => { kind: "days", by: -forward },
    "ArrowDown" => { kind: "days", by: DAYS_IN_WEEK },
    "ArrowUp" => { kind: "days", by: -DAYS_IN_WEEK },
    "Home" => { kind: "week-edge", to: "start" },
    "End" => { kind: "week-edge", to: "end" },
    // A year is twelve months rather than `{ years: 1 }`, so that the day is
    // clamped once by the same rule the month keys use: the 29th of February
    // plus a year is the 28th, and adding a year to a month-clamped date and
    // adding twelve months have to agree or `Shift+PageDown` twice would not
    // equal `PageDown` twenty-four times.
    "PageUp" => { kind: "months", by: -pages },
    "PageDown" => { kind: "months", by: pages },
    _ => null,
  };
}

/**
 * How far into its week `date` sits, counting from `weekStartsOn`.
 *
 * Zero for the first column, six for the last, in whichever order the week is
 * laid out. Both `weekEdge` and the grid's leading blanks are this number.
 */
export function columnOf(date: PlainDate, weekStartsOn: number): number {
  return (date.dayOfWeek - weekStartsOn + DAYS_IN_WEEK) % DAYS_IN_WEEK;
}

/** The first or the last day of `date`'s week, as this locale lays a week out. */
export function weekEdge(date: PlainDate, weekStartsOn: number, to: "start" | "end"): PlainDate {
  const into = columnOf(date, weekStartsOn);
  return to === "start"
    ? date.subtract({ days: into })
    : date.add({ days: DAYS_IN_WEEK - 1 - into });
}

/**
 * The date `movement` reaches from `from`.
 *
 * Nothing here refuses: a movement onto an unavailable date, into a month with
 * nothing selectable in it, or past whatever range the caller allows still
 * returns that date. Refusing is the component's decision and it makes a
 * different one — the module header says why a reader has to be able to walk
 * over an unavailable day rather than around it.
 *
 * The month and year movements clamp the day, because `PlainDate.add` does: the
 * 31st of January plus a month is the 28th of February rather than the 3rd of
 * March, which is Temporal's `constrain` overflow and what a person means by
 * "next month".
 */
export function moveDate(from: PlainDate, movement: DateMovement, weekStartsOn: number): PlainDate {
  return match (movement) {
    {kind: "days", by: const by} => from.add({ days: by }),
    {kind: "months", by: const by} => from.add({ months: by }),
    {kind: "years", by: const by} => from.add({ years: by }),
    {kind: "week-edge", to: const to} => weekEdge(from, weekStartsOn, to),
  };
}

/**
 * The rows of one month, seven cells wide, with `null` where no day falls.
 *
 * The blanks are blanks rather than the neighbouring months' days, and that is
 * the decision the rest of the keyboard behaviour rests on. A grid that showed
 * the 1st of November inside October's would have two cells that mean the same
 * date whenever a reader moved between them, and the WAI-ARIA pattern's
 * promise — that `ArrowRight` off the end of the month *changes the month* — has
 * nowhere to happen. Every blank is a `<td>` with no `gridcell` role, so the
 * rows stay rectangular for a screen reader counting columns.
 */
export function weeksOf(
  year: number,
  month: number,
  weekStartsOn: number,
): $ReadOnlyArray<$ReadOnlyArray<PlainDate | null>> {
  const first = Temporal.PlainDate.from({ day: 1, month, year });
  const blanks = columnOf(first, weekStartsOn);
  const days = first.daysInMonth;
  const rows = Math.ceil((blanks + days) / DAYS_IN_WEEK);

  const weeks = [];
  for (let row = 0; row < rows; row += 1) {
    const week = [];
    for (let column = 0; column < DAYS_IN_WEEK; column += 1) {
      const day = row * DAYS_IN_WEEK + column - blanks + 1;
      week.push(day >= 1 && day <= days ? first.add({ days: day - 1 }) : null);
    }
    weeks.push(week);
  }
  return weeks;
}

/**
 * Seven dates, one per column, for naming the columns.
 *
 * Dates rather than strings, because the names are the locale's and
 * `PlainDate.toLocaleString` is where a locale's day names live. Any week would
 * do; this one starts on a Monday so that `weekStartsOn` indexes it directly.
 */
export function weekdaysFrom(weekStartsOn: number): $ReadOnlyArray<PlainDate> {
  const monday = Temporal.PlainDate.from("2024-01-01");
  const days = [];
  for (let column = 0; column < DAYS_IN_WEEK; column += 1) {
    days.push(monday.add({ days: weekStartsOn - ISO_WEEK_START + column }));
  }
  return days;
}

/** Whether two dates are in the same month of the same year. */
export function sameMonth(a: PlainDate, b: PlainDate): boolean {
  return a.year === b.year && a.month === b.month;
}

/**
 * The day `locale` starts its weeks on, in ISO numbering.
 *
 * `Intl.Locale.prototype.getWeekInfo` is the answer the platform has, and its
 * `firstDay` is already ISO-numbered, so no translation is needed. Everything
 * around the call is defence rather than logic: Flow's vendored `intl.js`
 * declares no `Locale` at all, the method is newer than the class on some hosts,
 * and a locale tag that is merely *malformed* throws where an unknown one does
 * not. Each of those ends at ISO Monday, which is a defensible week rather than
 * a broken one.
 *
 * The cast is the narrowest available: one property read off `Intl`, for a class
 * the checker has never heard of. Widening it to the return value would hide
 * whether `firstDay` was a number at all, which is why that is asked separately.
 */
export function firstDayOfWeekFor(locale: string | void): number {
  const factory = (Intl as $FlowFixMe).Locale;
  if (typeof factory !== "function") {
    return ISO_WEEK_START;
  }
  try {
    const info = new factory(locale ?? "und").getWeekInfo?.();
    const first = info?.firstDay;
    return typeof first === "number" && first >= 1 && first <= DAYS_IN_WEEK
      ? first
      : ISO_WEEK_START;
  } catch {
    return ISO_WEEK_START;
  }
}
