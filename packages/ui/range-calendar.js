// @flow
"use client";
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import type { PlainDate } from "@uniflowed/core/temporal";
import { useControlled } from "./internal/controlled-state.js";
import type { Rest } from "./internal/merge-props.js";
import type { DateRange } from "./internal/date-range.js";
import { validateRange, unavailableInRange } from "./internal/date-range.js";
import { CalendarRoot, CalendarMonth } from "./calendar.js";
import { visuallyHiddenStyle } from "./internal/visually-hidden-style.js";
import { useLocale } from "./i18n-provider.js";
import { formatIsoDate, useMessages } from "./internal/messages.js";
export type { DateRange } from "./internal/date-range.js";

export component RangeCalendarRoot(
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
  const inherited = useLocale();
  const speaking = locale ?? inherited.locale;
  const messages = useMessages(speaking);
  const say = (iso: string) => formatIsoDate(iso, speaking);
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
      announce(messages.rangeStarted(say(iso)));
      return;
    }
    const start = anchor < iso ? anchor : iso,
      end = anchor < iso ? iso : anchor;
    if (unavailableInRange(start, end, isDateDisabled)) {
      announce(messages.rangeUnavailable);
      return;
    }
    setRange({ start, end });
    setAnchor(null);
    announce(messages.rangeSelected(say(start), say(end)));
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
