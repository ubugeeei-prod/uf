// @flow
//
// A month of dates, as one stop in the page's tab order.
//
// This is the largest component in the catalogue and the one where a small
// lookalike is most tempting, because a month of buttons in a grid *looks*
// finished from a screenshot. What makes a calendar usable is entirely in the
// keyboard and the announcements:
//
//   * `ArrowRight` / `ArrowLeft` move by a day, mirrored in a right-to-left
//     page; `ArrowDown` / `ArrowUp` move by a week.
//   * `Home` / `End` go to the ends of the *week*, not of the month.
//   * `PageUp` / `PageDown` change the month, `Shift` with either changes the
//     year.
//   * Running off the end of the month shows the next one and lands on its first
//     day — the grid is re-rendered under the reader's focus, and focus has to
//     arrive on a cell that did not exist when the key was pressed.
//   * An unavailable day is `aria-disabled` and still reachable, which is the
//     deliberate opposite of what a menu item does. `internal/date-grid.js` has
//     the argument.
//   * The month is announced when it changes, in a live region that was already
//     in the document — because a sighted reader sees the caption change and a
//     screen reader is told nothing at all otherwise.
//
// # One focused date, and everything else derived from it
//
// The component holds exactly one piece of grid state: which date has the tab
// stop. The month on display is that date's month, and there is deliberately no
// separate `month` prop to control. `role="grid"` requires that exactly one cell
// is `tabindex="0"` — a set with two tab stops takes two `Tab` presses to leave,
// and a set with none is unreachable — and two independent values are two ways
// to break that invariant. `Calendar.Previous` moves the focused date rather
// than a month cursor, and `onMonthChange` reports the result for a caller
// loading availability a month at a time.
//
// # Today comes from the clock seam, and is read once
//
// `Temporal.Now.plainDateISO()` inside a render is the bug `@uniflowed/core`'s
// temporal module exists to prevent: the server's date and the browser's are
// different, and React compares the markup. So it is read once, in a state
// initialiser, and it reads `@uniflowed/core/clock` rather than the machine —
// which is the seam a server installs per request and a test replaces outright.
// An application that server-renders a calendar and does neither can hydrate
// into a different month; the way out is that seam, or the `today` prop, and
// both are cheaper than a component that re-reads a clock it cannot trust.
//
// # Range selection is not here
//
// One date. A range is two values with both ends announced, `aria-selected` on
// everything between them, and a second set of keyboard rules for extending;
// none of that is written, and a `value` that quietly accepted a pair would be
// the lookalike this package refuses. It is tracked rather than implied.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";
import type { DateTimeFormatOptions, PlainDate } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";

import { useControlled } from "./internal/controlled-state.js";
import type { Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  forwarded,
  withoutComposed,
} from "./internal/merge-props.js";
import { directionOf } from "./internal/roving-focus.js";
import {
  firstDayOfWeekFor,
  moveDate,
  movementForDateKey,
  weekdaysFrom,
  weeksOf,
} from "./internal/date-grid.js";

/**
 * A date, however the caller had one to hand.
 *
 * A string is accepted because `value="2026-10-14"` is what a form field, a URL
 * parameter and a JSON payload all carry, and making every caller construct a
 * `PlainDate` to pass one in would be ceremony. It is ISO 8601 — the format
 * `Temporal.PlainDate.from` parses — and not a locale format: parsing
 * `12/03/26` is a locale question that belongs to `@uniflowed/temporal` rather
 * than to a UI package, and `date-picker.js` says what that means for a field a
 * reader types into.
 */
export type DateValue = PlainDate | string;

/**
 * How the caption and the column headers are worded.
 *
 * Annotated rather than inferred: an object literal of strings is a
 * `{month: string, ...}`, and `Intl` accepts four spellings of `month` and not
 * every string, so the annotation is what makes a typo here a checker error
 * instead of a caption that silently prints nothing.
 */
const CAPTION_FORMAT: DateTimeFormatOptions = { month: "long", year: "numeric" };
const COLUMN_FORMAT: DateTimeFormatOptions = { weekday: "long" };
const COLUMN_ABBREVIATION: DateTimeFormatOptions = { weekday: "short" };

type CalendarState = {|
  readonly base: string,
  /** The month and year over the grid, and the sentence the live region says. */
  readonly caption: string,
  readonly focused: PlainDate,
  /**
   * The cell holding the tab stop, written after every render of the grid.
   *
   * For an overlay around the calendar to send focus there when it opens:
   * `Popover.Body` and `Dialog.Body` both take an `initialFocus` ref, and a
   * reader who opened a date picker is looking for the date rather than for the
   * button that steps back a month. It is filled from an effect in
   * `Calendar.Month`, which runs before any effect of a component *around* the
   * calendar - React runs a child's effects first, and that ordering is what
   * makes the ref already correct when the overlay reads it.
   */
  readonly focusedDayRef: { current: HTMLElement | null },
  readonly isDisabled: (date: PlainDate) => boolean,
  readonly locale: string | void,
  readonly moveFocus: (date: PlainDate, viaKeyboard: boolean) => void,
  /**
   * The cell the keyboard should be on after the next render, as an ISO date.
   *
   * A ref rather than state for the reason `Menu.Body`'s `pendingFocus` is one:
   * it is an instruction for the next commit and not a value anything renders.
   * It is what makes `ArrowRight` off the end of October land on the 1st of
   * November — a cell that is rendered for the first time by the same update
   * that asked for it, so nothing could have focused it when the key arrived.
   */
  readonly pendingFocus: { current: string | null },
  readonly select: (date: PlainDate) => void,
  readonly selected: PlainDate | null,
  readonly today: PlainDate,
  readonly weekStartsOn: number,
|};

const CalendarContext: React.Context<CalendarState | null> = createContext(null);

hook useCalendar(part: string): CalendarState {
  const state = useContext(CalendarContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Calendar.Root`);
  }
  return state;
}

/** A `DateValue` as a `PlainDate`, whichever it arrived as. */
function toDate(value: DateValue): PlainDate {
  return Temporal.PlainDate.from(value);
}

/**
 * The calendar's state, its live region, and nothing else it renders.
 *
 * A `<div>` around whatever the caller laid out, plus one `role="status"` that
 * is in the document from the first render. That timing is the whole point of
 * it: a live region added to the page in the same commit as the text it holds
 * is usually not announced, because the technology watching it had nothing to
 * watch until it was too late. `Combobox.Status` documents the same constraint,
 * and here there is no part for the region because a caller has no reason to
 * place it — so `Calendar.Root` renders it and keeps it empty until the month
 * actually changes.
 */
export component CalendarRoot(
  children: React.Node,
  defaultFocused?: DateValue,
  defaultValue?: DateValue | null = null,
  /** Filled with the cell that holds the tab stop; see `CalendarState`. */
  focusedDayRef?: { current: HTMLElement | null },
  /** Whether a date may be chosen. A rejected date stays reachable; see the header. */
  isDateDisabled?: (date: PlainDate) => boolean,
  locale?: string,
  onMonthChange?: (firstOfMonth: PlainDate) => mixed,
  onValueChange?: (value: PlainDate) => mixed,
  today?: DateValue,
  value?: DateValue | null,
  /** ISO day numbers: 1 is Monday, 7 is Sunday. Defaults to the locale's own. */
  weekStartsOn?: number,
  ...rest: Rest
) {
  const base = useId();
  const pendingFocus = useRef<string | null>(null);
  // One is always allocated, because a hook may not be called conditionally;
  // the caller's is used when there is one.
  const ownDayRef = useRef<HTMLElement | null>(null);
  const dayRef = focusedDayRef ?? ownDayRef;

  // Read once. The header says why this is not the `Now`-in-a-render bug, and
  // why an application that server-renders a calendar wants the clock seam.
  const [clockToday] = useState<PlainDate>(() => Temporal.Now.plainDateISO());
  const currentDate = useMemo(
    () => (today === undefined ? clockToday : toDate(today)),
    [clockToday, today],
  );

  const controlled = useMemo(
    () => (value === undefined ? undefined : value === null ? null : toDate(value)),
    [value],
  );
  const initialValue = useMemo(
    () => (defaultValue == null ? null : toDate(defaultValue)),
    [defaultValue],
  );
  // Narrowed on the way out rather than in the prop's type: the component never
  // clears a selection, so a caller's handler should not have to accept a `null`
  // it can never be given.
  const report = useStableCallback((next: PlainDate | null) => {
    if (next != null) {
      onValueChange?.(next);
    }
  });
  const [selected, setSelected] = useControlled<PlainDate | null>(controlled, initialValue, report);

  const [focused, setFocused] = useState<PlainDate>(() => {
    const start = defaultFocused ?? value ?? defaultValue;
    return start == null ? currentDate : toDate(start);
  });

  const weekStart = useMemo(
    () => (weekStartsOn == null ? firstDayOfWeekFor(locale) : weekStartsOn),
    [locale, weekStartsOn],
  );

  const isDisabled = useStableCallback((date: PlainDate) => isDateDisabled?.(date) === true);

  const moveFocus = useStableCallback((date: PlainDate, viaKeyboard: boolean) => {
    if (viaKeyboard) {
      // Only for the keyboard. A press already put focus on the cell it landed
      // on, and asking for it again would fight a caller who moved it.
      pendingFocus.current = date.toString();
    }
    // Compared rather than assigned, because `Calendar.Day` reports the focus
    // that this very call produced: a key moves the tab stop, the effect focuses
    // the new cell, and the cell says so. Without the comparison that is one
    // extra render of the whole month per arrow key, for a value that did not
    // change.
    setFocused((current) => (current.equals(date) ? current : date));
  });

  const select = useStableCallback((date: PlainDate) => {
    if (isDisabled(date)) {
      return;
    }
    setSelected(date);
    setFocused((current) => (current.equals(date) ? current : date));
  });

  const caption = useMemo(() => focused.toLocaleString(locale, CAPTION_FORMAT), [focused, locale]);

  const [announcement, setAnnouncement] = useState("");
  const shown = useRef(`${focused.year}-${focused.month}`);
  const monthChanged = useStableCallback((first: PlainDate) => {
    onMonthChange?.(first);
  });

  useEffect(() => {
    const key = `${focused.year}-${focused.month}`;
    if (shown.current === key) {
      return;
    }
    shown.current = key;
    // The caption, deliberately word for word: a reader who hears something
    // other than what is written above the grid has to work out that the two
    // are the same thing.
    setAnnouncement(caption);
    monthChanged(Temporal.PlainDate.from({ day: 1, month: focused.month, year: focused.year }));
  }, [caption, focused, monthChanged]);

  const state = useMemo(
    () => ({
      base,
      caption,
      focused,
      focusedDayRef: dayRef,
      isDisabled,
      locale,
      moveFocus,
      pendingFocus,
      select,
      selected,
      today: currentDate,
      weekStartsOn: weekStart,
    }),
    [
      base,
      caption,
      currentDate,
      dayRef,
      focused,
      isDisabled,
      locale,
      moveFocus,
      select,
      selected,
      weekStart,
    ],
  );

  return (
    <CalendarContext.Provider value={state}>
      <div {...rest}>
        {children}
        <div aria-atomic="true" aria-live="polite" id={`${base}-status`} role="status">
          {announcement}
        </div>
      </div>
    </CalendarContext.Provider>
  );
}

/**
 * The grid: a caption, seven named columns, and the month's days.
 *
 * `role="grid"` rather than a plain table, because the cells are the thing a
 * reader operates rather than data they read past. The keys are handled here
 * rather than on each day, for the reason `Menu.Body` handles its own: every one
 * of them is a question about the *set* — "a week later", "the end of this
 * week" — and a day knows nothing about the days around it.
 *
 * `children` is a function rather than a list of parts, and that is what a month
 * needs: the caller is given each date and returns the cell for it, so a day
 * with a dot under it for an appointment is a `Calendar.Day` with a child, not a
 * fork of this component. Omitted, every day renders its own number.
 */
export component CalendarMonth(children?: (date: PlainDate) => renders CalendarDay, ...rest: Rest) {
  const calendar = useCalendar("Calendar.Month");
  const gridRef = useRef<HTMLElement | null>(null);
  const { focused, moveFocus, pendingFocus, weekStartsOn } = calendar;

  const weeks = useMemo(
    () => weeksOf(focused.year, focused.month, weekStartsOn),
    [focused.month, focused.year, weekStartsOn],
  );
  const columns = useMemo(() => weekdaysFrom(weekStartsOn), [weekStartsOn]);

  // No dependency list, and guarded by what it reads rather than by one: the
  // cell it looks for may have been produced by a *caller's* render, which is a
  // change no dependency list of this component's own values can describe. The
  // work is one `querySelector` over a grid that is at most forty-two cells.
  useEffect(() => {
    const grid = gridRef.current;
    if (grid == null) {
      return;
    }
    const at = focused.toString();
    const cell: HTMLElement | null = (grid.querySelector(`[data-date="${at}"]`): $FlowFixMe);
    calendar.focusedDayRef.current = cell;
    // Only when a key asked for it, and only once the render it asked for has
    // happened: `ArrowRight` off the end of October sets this to the 1st of
    // November, and the cell exists for the first time in the commit this
    // effect belongs to.
    if (pendingFocus.current !== at) {
      return;
    }
    pendingFocus.current = null;
    cell?.focus();
  });

  const passed = withoutComposed(rest, ["onKeyDown", "ref"]);

  return (
    <table
      {...passed}
      // Named by its own caption, and said twice on purpose. A `<caption>` is
      // the table's name in HTML and needs no attribute — but `role="grid"`
      // overrides the element's own role, and how a caption survives that is
      // exactly the sort of thing that differs between screen readers. The
      // attribute points at the caption that is there either way, so the two
      // answers cannot disagree.
      aria-labelledby={`${calendar.base}-caption`}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        const grid: $FlowFixMe = event.currentTarget;
        const movement = movementForDateKey(event, directionOf(grid));
        if (movement == null) {
          return;
        }
        // Before moving, or the arrow scrolls the page under the cell that has
        // just taken focus and the reader ends up looking somewhere else.
        event.preventDefault();
        moveFocus(moveDate(focused, movement, weekStartsOn), true);
      })}
      ref={composeRefs(rest.ref, (element) => {
        gridRef.current = element;
      })}
      role="grid"
    >
      <caption id={`${calendar.base}-caption`}>{calendar.caption}</caption>
      <thead>
        <tr>
          {columns.map((day) => (
            <th
              // The full day name is the accessible name and the short one is
              // what is drawn, because a column of `Wednesday` is not a calendar
              // and a screen reader saying "We" is not a day. `aria-hidden` on
              // the visible text is what keeps the two from being announced one
              // after the other.
              aria-label={day.toLocaleString(calendar.locale, COLUMN_FORMAT)}
              key={day.dayOfWeek}
              scope="col"
            >
              <span aria-hidden="true">
                {day.toLocaleString(calendar.locale, COLUMN_ABBREVIATION)}
              </span>
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {weeks.map((week) => (
          <tr key={week.find((day) => day != null)?.toString() ?? ""}>
            {week.map((day, column) =>
              day == null ? (
                // A blank rather than the neighbouring month's day; see
                // `internal/date-grid.js`. No `gridcell` role, so it is a cell a
                // reader is told is empty rather than a date they cannot reach.
                <td key={`blank-${column}`} />
              ) : (
                <React.Fragment key={day.toString()}>
                  {children == null ? <CalendarDay date={day} /> : children(day)}
                </React.Fragment>
              ),
            )}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/**
 * One day.
 *
 * The cell itself is the focus stop rather than a button inside it. A grid's
 * cells are what `role="grid"` says the arrow keys move between, and a button
 * inside each one would put a second focusable element in every cell — which
 * either doubles the tab stops or leaves the cell announced as empty.
 *
 * It carries no `aria-label`. The name of a `gridcell` is its contents, and a
 * screen reader in a grid announces the column header and the caption around it
 * — so "14" is heard as "Wednesday 14, October 2026" without this component
 * inventing a second wording that a translation table would then have to own. A
 * caller who wants one passes it; it is not overridden here.
 */
export component CalendarDay(date: PlainDate, children?: React.Node, ...rest: Rest) {
  const calendar = useCalendar("Calendar.Day");
  const disabled = calendar.isDisabled(date);
  const chosen = calendar.selected != null && calendar.selected.equals(date);
  const passed = withoutComposed(rest, ["onClick", "onFocus", "onKeyDown"]);

  return (
    <td
      {...passed}
      aria-current={calendar.today.equals(date) ? "date" : undefined}
      // `aria-disabled`, never the `disabled` a menu item would refuse with:
      // this one stays in the accessibility tree and stays reachable, so a
      // reader can find out that the 3rd is unavailable instead of finding a
      // hole in the month where the 3rd should be.
      aria-disabled={disabled ? "true" : undefined}
      // Only on the chosen day. Every other cell saying `aria-selected="false"`
      // makes a screen reader announce "not selected" on all thirty-one.
      aria-selected={chosen ? "true" : undefined}
      data-date={date.toString()}
      onClick={composeHandlers(rest.onClick, () => calendar.select(date))}
      // The tab stop follows real focus, which is the half of a roving set that
      // is easy to leave out and impossible to see. A press on an *unavailable*
      // day is the case: the browser focuses the cell, this component refuses
      // the selection, and without this the tab stop would still be on the day
      // the reader left — so the next arrow key would move from a date they are
      // no longer looking at. `Menu.Item` keeps the same invariant the same way.
      onFocus={composeHandlers(rest.onFocus, () => calendar.moveFocus(date, false))}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (event.key !== "Enter" && event.key !== " ") {
          return;
        }
        // A cell is not a button, so the two keys that activate one have to be
        // handled here. `Space` in particular: unclaimed, it scrolls the page.
        event.preventDefault();
        calendar.select(date);
      })}
      role="gridcell"
      // The roving tab stop: exactly one cell in the grid, always the focused
      // date, which is why the displayed month is derived from it.
      tabIndex={date.equals(calendar.focused) ? 0 : -1}
    >
      {children ?? date.day}
    </td>
  );
}

/**
 * The button that shows the month before this one.
 *
 * `forwarded` rather than a bare spread, for the reason
 * `internal/merge-props.js` gives at length: a part is spreadable onto a `<div>`
 * and not onto a sibling part, because `Rest` names `key` out of its indexer and
 * the receiving component's own indexer answers `mixed` for it.
 */
export component CalendarPrevious(children: React.Node, ...rest: Rest) {
  return (
    <MonthStep {...forwarded(rest)} by={-1}>
      {children}
    </MonthStep>
  );
}

/** The button that shows the month after this one. */
export component CalendarNext(children: React.Node, ...rest: Rest) {
  return (
    <MonthStep {...forwarded(rest)} by={1}>
      {children}
    </MonthStep>
  );
}

/**
 * The two month buttons, which differ by a sign.
 *
 * Focus stays on the button rather than following the month into the grid, which
 * is what lets a reader step through several months in a row. The focused date
 * moves with the month — `PlainDate.add` clamps it, so the 31st of March
 * stepping back is the 28th or 29th of February rather than the 2nd or 3rd of
 * March — because the grid must always contain exactly one tab stop.
 */
component MonthStep(children: React.Node, by: number, ...rest: Rest) {
  const calendar = useCalendar(by < 0 ? "Calendar.Previous" : "Calendar.Next");
  const passed = withoutComposed(rest, ["onClick"]);

  return (
    <button
      {...passed}
      onClick={composeHandlers(rest.onClick, () => {
        calendar.moveFocus(calendar.focused.add({ months: by }), false);
      })}
      type="button"
    >
      {children}
    </button>
  );
}
