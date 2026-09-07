// @flow
//
// A field somebody types a date into, and a calendar for the times they would
// rather point at one.
//
// # Why the field comes first
//
// A date picker whose only input is the grid is slower for everybody who
// already knows the date — nine keystrokes to arrow to a day they could have
// typed in six — and it is unusable for anybody who cannot operate a grid at
// all. So the text field is the control, the calendar is the second way in, and
// the composition is written down here rather than left to each application to
// assemble differently.
//
// # It is a composition, and the parts are the ones that already exist
//
// `Popover` for the overlay and its dismissal, `Calendar` for the grid. Nothing
// about anchoring, outside presses, `Escape` or focus restoration is
// reimplemented here, which is the point: ubugeeei-prod/uf#256's complaint was
// five components each carrying a corner of the same behaviour, and a sixth
// carrying its own corner would be the same mistake with a different name.
//
// What this module does own is the three joins between them:
//
//   * the field's text and the chosen date, which are not the same value and
//     must not be kept in step by an effect that fights the reader's typing;
//   * `Escape` and a chosen date both returning focus to the *field* rather than
//     to the button, because the field is the primary control;
//   * the calendar opening with focus on a date rather than on the button that
//     steps back a month, which is `Popover.Body`'s `initialFocus` and
//     `Calendar.Root`'s `focusedDayRef` meeting.
//
// # Parsing is `@uniflowed/temporal`'s, not this module's
//
// `format` and `parse` default to ISO 8601, because that is the format
// `Temporal.PlainDate` reads and writes and the only one this package can claim
// to handle. `12/03/26` is the third of December or the twelfth of March
// depending on where the reader is, and answering that needs the locale's date
// patterns — CLDR data, which is what `@uniflowed/temporal`'s calendar surface
// waits on the native runtime for. A UI package that shipped its own guess at it
// would be a wrong date in production rather than a missing feature, so the two
// props are the seam: an application with a locale format passes both, and gets
// its own round trip rather than this module's approximation of one.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useMemo, useRef, useState } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";
import type { PlainDate } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";

import type { DateValue } from "./calendar.js";
import { CalendarRoot } from "./calendar.js";
import { useControlled } from "./internal/controlled-state.js";
import type { Align, LogicalSide } from "./internal/anchor.js";
import type { Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  forwarded,
  withoutComposed,
} from "./internal/merge-props.js";
import { PopoverBody, PopoverRoot, PopoverTrigger } from "./popover.js";

type DatePickerState = {|
  /** The text in the field, which is the draft while one is being typed. */
  readonly text: string,
  readonly setDraft: (text: string | null) => void,
  /** Whether the last thing typed could not be read as a date. */
  readonly invalid: boolean,
  readonly commit: (text: string) => void,
  readonly choose: (date: PlainDate) => void,
  readonly value: PlainDate | null,
  readonly fieldRef: { current: HTMLElement | null },
  readonly focusField: () => void,
|};

const DatePickerContext: React.Context<DatePickerState | null> = createContext(null);

hook useDatePicker(part: string): DatePickerState {
  const state = useContext(DatePickerContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a DatePicker.Root`);
  }
  return state;
}

/** ISO 8601, which is what `PlainDate.toString` produces and `from` parses. */
function isoFormat(date: PlainDate): string {
  return date.toString();
}

/**
 * ISO 8601 or nothing.
 *
 * `PlainDate.from` throws a `RangeError` on anything it cannot read, and a
 * reader half way through typing a date is in that state on almost every
 * keystroke — so the failure is a value here rather than an exception, and the
 * field decides what to do about it.
 */
function isoParse(text: string): PlainDate | null {
  const trimmed = text.trim();
  if (trimmed === "") {
    return null;
  }
  try {
    return Temporal.PlainDate.from(trimmed);
  } catch {
    return null;
  }
}

/**
 * The value, the popover around the calendar, and the joins between them.
 *
 * Renders no element of its own: the field, the button and the calendar are
 * siblings in whatever layout the caller wrote, and a wrapper would put a
 * `<div>` between them that they then have to style around.
 */
export component DatePickerRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  defaultValue?: DateValue | null = null,
  /** How a chosen date is written into the field. ISO 8601 unless told otherwise. */
  format?: (date: PlainDate) => string = isoFormat,
  isDateDisabled?: (date: PlainDate) => boolean,
  locale?: string,
  onOpenChange?: (open: boolean) => void,
  onValueChange?: (value: PlainDate | null) => mixed,
  open?: boolean,
  /** How typed text becomes a date, or null when it is not one yet. */
  parse?: (text: string) => PlainDate | null = isoParse,
  today?: DateValue,
  value?: DateValue | null,
  weekStartsOn?: number,
) {
  const fieldRef = useRef<HTMLElement | null>(null);
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);

  const controlled = useMemo(
    () =>
      value === undefined ? undefined : value === null ? null : Temporal.PlainDate.from(value),
    [value],
  );
  const initial = useMemo(
    () => (defaultValue == null ? null : Temporal.PlainDate.from(defaultValue)),
    [defaultValue],
  );
  const report = useStableCallback((next: PlainDate | null) => {
    onValueChange?.(next);
  });
  const [chosen, setChosen] = useControlled<PlainDate | null>(controlled, initial, report);

  // The field's text is the *draft* while there is one, and the formatted value
  // otherwise. Two pieces of state kept in step by an effect is the arrangement
  // this avoids: an effect that copies the value into the field overwrites what
  // the reader is halfway through typing, and one that does not runs stale the
  // moment the caller sets a value from outside.
  const [draft, setDraft] = useState<string | null>(null);
  const [invalid, setInvalid] = useState(false);
  const text = draft ?? (chosen == null ? "" : format(chosen));

  const focusField = useStableCallback(() => {
    fieldRef.current?.focus?.();
  });

  const commit = useStableCallback((typed: string) => {
    if (typed.trim() === "") {
      setChosen(null);
      setDraft(null);
      setInvalid(false);
      return;
    }
    const parsed = parse(typed);
    if (parsed == null) {
      // The text stays. Clearing it would throw away what the reader typed and
      // leave them nothing to correct.
      setInvalid(true);
      return;
    }
    setChosen(parsed);
    setDraft(null);
    setInvalid(false);
  });

  const choose = useStableCallback((date: PlainDate) => {
    setChosen(date);
    setDraft(null);
    setInvalid(false);
    // Before the popover closes, and that order is load-bearing: `Popover.Body`
    // restores focus to its trigger only when focus would otherwise be lost, so
    // moving it to the field first is what makes the field - and not the button
    // - where the reader ends up.
    focusField();
    setOpen(false);
  });

  const dateSettings = useMemo(
    () => ({ isDateDisabled, locale, today, weekStartsOn }),
    [isDateDisabled, locale, today, weekStartsOn],
  );

  const state = useMemo(
    () => ({
      choose,
      commit,
      fieldRef,
      focusField,
      invalid,
      setDraft,
      text,
      value: chosen,
    }),
    [choose, chosen, commit, focusField, invalid, text],
  );

  return (
    <DatePickerContext.Provider value={state}>
      <CalendarSettings.Provider value={dateSettings}>
        <PopoverRoot onOpenChange={setOpen} open={isOpen}>
          {children}
        </PopoverRoot>
      </CalendarSettings.Provider>
    </DatePickerContext.Provider>
  );
}

/** What `DatePicker.Root` was told about dates, for the calendar it renders. */
type CalendarSettingsValue = {|
  readonly isDateDisabled: ((date: PlainDate) => boolean) | void,
  readonly locale: string | void,
  readonly today: DateValue | void,
  readonly weekStartsOn: number | void,
|};

const CalendarSettings: React.Context<CalendarSettingsValue> = createContext({
  isDateDisabled: undefined,
  locale: undefined,
  today: undefined,
  weekStartsOn: undefined,
});

/**
 * The text field, which is the control.
 *
 * An ordinary `<input type="text">` rather than `type="date"`: the native one is
 * a different widget with its own popup, its own format and no way to be told
 * which dates are unavailable, and wrapping it would leave two calendars in one
 * control. It carries no `role`, no `aria-haspopup` and no `aria-expanded` — it
 * does not open the popover, the button beside it does, and telling a reader the
 * field expands something would be a promise the field does not keep.
 */
export component DatePickerInput(...rest: Rest) {
  const picker = useDatePicker("DatePicker.Input");
  const passed = withoutComposed(rest, ["onBlur", "onChange", "onKeyDown", "ref"]);

  return (
    <input
      {...passed}
      aria-invalid={picker.invalid ? "true" : undefined}
      onBlur={composeHandlers(rest.onBlur, (event) => {
        picker.commit((event.currentTarget: $FlowFixMe).value);
      })}
      onChange={composeHandlers(rest.onChange, (event) => {
        picker.setDraft((event.currentTarget: $FlowFixMe).value);
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (event.key !== "Enter") {
          return;
        }
        // Claimed, so a date picker inside a form is not a control where
        // pressing Enter to confirm what you typed submits the page instead.
        event.preventDefault();
        picker.commit((event.currentTarget: $FlowFixMe).value);
      })}
      ref={composeRefs(rest.ref, (element) => {
        picker.fieldRef.current = element;
      })}
      type="text"
      value={picker.text}
    />
  );
}

/** The button that opens the calendar. */
export component DatePickerTrigger(children: React.Node, ...rest: Rest) {
  // `forwarded`, because this part renders another part rather than an
  // intrinsic; `internal/merge-props.js` says what that costs and why.
  return <PopoverTrigger {...forwarded(rest)}>{children}</PopoverTrigger>;
}

/**
 * The calendar, in the popover, wired to the field.
 *
 * `children` is the calendar's own layout — the month buttons and
 * `Calendar.Month` — because where those sit is a design decision and there is
 * no arrangement of them this module could impose that would suit every one.
 */
export component DatePickerCalendar(
  children: React.Node,
  align?: Align = "start",
  side?: LogicalSide = "bottom",
  sideOffset?: number = 0,
  ...rest: Rest
) {
  const picker = useDatePicker("DatePicker.Calendar");
  const settings = useContext(CalendarSettings);
  const dayRef = useRef<HTMLElement | null>(null);
  const passed = withoutComposed(rest, ["onKeyDown"]);

  return (
    <PopoverBody
      {...forwarded(passed)}
      align={align}
      // The date, not the first button in the popover. The APG's date picker
      // dialog puts focus on the grid for the same reason.
      initialFocus={dayRef}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (event.key !== "Escape") {
          return;
        }
        // Not prevented and not stopped: `Popover.Body`'s own handler is what
        // closes it, and this only decides where focus lands afterwards. Moving
        // it to the field first is what makes the popover's restore stand down;
        // `choose` says why that is the order.
        picker.focusField();
      })}
      side={side}
      sideOffset={sideOffset}
    >
      <CalendarRoot
        defaultFocused={picker.value ?? undefined}
        focusedDayRef={dayRef}
        isDateDisabled={settings.isDateDisabled}
        locale={settings.locale}
        onValueChange={picker.choose}
        today={settings.today}
        value={picker.value}
        weekStartsOn={settings.weekStartsOn}
      >
        {children}
      </CalendarRoot>
    </PopoverBody>
  );
}
