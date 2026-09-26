// @flow
"use client";
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import type { PlainDate } from "@uniflowed/core/temporal";
import { useControlled } from "./internal/controlled-state.js";
import type { Rest } from "./internal/merge-props.js";
import type { DateRange } from "./internal/date-range.js";
import { validateRange, unavailableInRange } from "./internal/date-range.js";
import { Root as CalendarRoot, Month as CalendarMonth } from "./calendar.js";
import { visuallyHiddenStyle } from "./internal/visually-hidden-style.js";
export type { DateRange } from "./internal/date-range.js";

component RangeCalendarRoot(
  children?: React.Node = <CalendarMonth />,
  value?: DateRange | null,
  defaultValue?: DateRange | null = null,
  onValueChange?: (value: DateRange | null) => void,
  minValue?: string,
  maxValue?: string,
  isDateDisabled?: (date: PlainDate) => boolean,
  defaultFocused?: string,
  today?: string,
  locale?: string,
  focusedDayRef?: { current: HTMLElement | null },
  ...rest: Rest
) {
  const [range, setRange] = useControlled(value, defaultValue, onValueChange);
  const [anchor, setAnchor] = useState<string | null>(null);
  const [announcement, announce] = useState("");
  validateRange(range);
  const disabled = (date: PlainDate) =>
    (minValue != null && date.toString() < minValue) ||
    (maxValue != null && date.toString() > maxValue) ||
    isDateDisabled?.(date) === true;
  const choose = (date: PlainDate) => {
    const iso = date.toString();
    if (disabled(date)) return;
    if (anchor == null) {
      setAnchor(iso);
      announce(`Start ${iso}. Choose an end date.`);
      return;
    }
    const start = anchor < iso ? anchor : iso,
      end = anchor < iso ? iso : anchor;
    if (unavailableInRange(start, end, isDateDisabled)) {
      announce("The range contains an unavailable date");
      return;
    }
    setRange({ start, end });
    setAnchor(null);
    announce(`Selected ${start} to ${end}`);
  };
  const selected = (date: PlainDate) => {
    const iso = date.toString();
    return anchor != null
      ? iso === anchor
      : range != null && iso >= range.start && iso <= range.end;
  };
  return (
    <div {...rest}>
      <CalendarRoot
        value={anchor ?? range?.start ?? null}
        defaultFocused={defaultFocused}
        today={today}
        locale={locale}
        focusedDayRef={focusedDayRef}
        isDateDisabled={disabled}
        isDateSelected={selected}
        onValueChange={choose}
      >
        {children}
      </CalendarRoot>
      <span role="status" aria-live="polite" style={visuallyHiddenStyle}>
        {announcement}
      </span>
    </div>
  );
}

/**
 * The parts, under the names the `RangeCalendar` namespace gives them.
 *
 * `index.js` re-exports this module whole — `export * as RangeCalendar from "./range-calendar.js"` —
 * so a caller writes `<RangeCalendar.Root>`, and the namespace is the prefix. Each
 * part is still *declared* as `RangeCalendarRoot`, so React DevTools, a component
 * stack and an error name the part a reader can find rather than one of forty
 * `Root`s.
 */
export { RangeCalendarRoot as Root };

/**
 * The parts `RangeCalendar` shares with `Calendar`, re-exported so the `RangeCalendar`
 * namespace is complete on its own: `<RangeCalendar.Month>` is `Calendar`'s part,
 * not a copy of it.
 */
export { Month, Day, Previous, Next } from "./calendar.js";
