// @flow
"use client";
import * as React from "@uniflowed/react";
import { createContext, useContext, useRef } from "@uniflowed/react";
import type { PlainDate } from "@uniflowed/core/temporal";
import { useControlled } from "./internal/controlled-state.js";
import type { Rest } from "./internal/merge-props.js";
import { forwarded } from "./internal/merge-props.js";
import type { DateRange } from "./internal/date-range.js";
import { validateRange, unavailableInRange } from "./internal/date-range.js";
import { CalendarMonth } from "./calendar.js";
import { RangeCalendarRoot } from "./range-calendar.js";
import { DateField } from "./date-field.js";
import { PopoverRoot, PopoverBody, PopoverTrigger } from "./popover.js";

type Picker = {
  range: DateRange | null,
  set: (range: DateRange | null) => void,
  close: () => void,
  minValue: string | void,
  maxValue: string | void,
  isDateDisabled: ((date: PlainDate) => boolean) | void,
};
const PickerContext: React.Context<Picker | null> = createContext(null);
hook usePicker(): Picker {
  const picker = useContext(PickerContext);
  if (picker == null) throw new Error("DateRangePicker parts must be inside DateRangePicker.Root");
  return picker;
}
export component DateRangePickerRoot(
  children: React.Node,
  value?: DateRange | null,
  defaultValue?: DateRange | null = null,
  onValueChange?: (range: DateRange | null) => void,
  open?: boolean,
  defaultOpen?: boolean = false,
  onOpenChange?: (open: boolean) => void,
  minValue?: string,
  maxValue?: string,
  isDateDisabled?: (date: PlainDate) => boolean,
) {
  const [range, setRange] = useControlled(value, defaultValue, onValueChange);
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  validateRange(range);
  const state = {
    range,
    set: setRange,
    close: () => setOpen(false),
    minValue,
    maxValue,
    isDateDisabled,
  };
  return (
    <PickerContext.Provider value={state}>
      <PopoverRoot open={isOpen} onOpenChange={setOpen}>
        {children}
      </PopoverRoot>
    </PickerContext.Provider>
  );
}
export component DateRangePickerStartField(...rest: Rest) {
  const picker = usePicker();
  return (
    <DateField
      {...forwarded(rest)}
      value={picker.range?.start ?? null}
      minValue={picker.minValue}
      maxValue={picker.range?.end ?? picker.maxValue}
      isDateUnavailable={(start) =>
        unavailableInRange(start, picker.range?.end ?? start, picker.isDateDisabled)
      }
      onValueChange={(start) => {
        if (start == null) picker.set(null);
        else picker.set({ start, end: picker.range?.end ?? start });
      }}
    />
  );
}
export component DateRangePickerEndField(...rest: Rest) {
  const picker = usePicker();
  return (
    <DateField
      {...forwarded(rest)}
      value={picker.range?.end ?? null}
      minValue={picker.range?.start ?? picker.minValue}
      maxValue={picker.maxValue}
      isDateUnavailable={(end) =>
        unavailableInRange(picker.range?.start ?? end, end, picker.isDateDisabled)
      }
      onValueChange={(end) => {
        if (end == null) picker.set(null);
        else picker.set({ start: picker.range?.start ?? end, end });
      }}
    />
  );
}
export component DateRangePickerTrigger(children: React.Node, ...rest: Rest) {
  return <PopoverTrigger {...forwarded(rest)}>{children}</PopoverTrigger>;
}
export component DateRangePickerCalendar(children?: React.Node = <CalendarMonth />, ...rest: Rest) {
  const picker = usePicker();
  const dayRef = useRef<HTMLElement | null>(null);
  return (
    <PopoverBody {...forwarded(rest)} initialFocus={dayRef}>
      <RangeCalendarRoot
        value={picker.range}
        minValue={picker.minValue}
        maxValue={picker.maxValue}
        isDateDisabled={picker.isDateDisabled}
        focusedDayRef={dayRef}
        onValueChange={(range) => {
          picker.set(range);
          picker.close();
        }}
      >
        {children}
      </RangeCalendarRoot>
    </PopoverBody>
  );
}
