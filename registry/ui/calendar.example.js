// @flow
import {
  Calendar,
  CalendarHeader,
  CalendarMonth,
  CalendarNext,
  CalendarPrevious,
} from "./calendar.js";

/** Whether a date falls on a Saturday or a Sunday. */
function weekend(date: { readonly dayOfWeek: number, ... }): boolean {
  return date.dayOfWeek > 5;
}

/** September 2026, the 14th chosen and today, with weekends out of reach. */
export component Example() {
  return (
    <Calendar defaultValue="2026-09-14" isDateDisabled={weekend} locale="en-US" today="2026-09-14">
      <CalendarHeader>
        <CalendarPrevious />
        <CalendarNext />
      </CalendarHeader>
      <CalendarMonth />
    </Calendar>
  );
}
