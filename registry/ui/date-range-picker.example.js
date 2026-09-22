// @flow
import * as React from "@uniflowed/react";
import {
  DateRangePicker,
  DateRangePickerStartField,
  DateRangePickerEndField,
  DateRangePickerTrigger,
  DateRangePickerCalendar,
} from "./date-range-picker.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <DateRangePicker defaultValue={{ start: "2026-09-14", end: "2026-09-16" }}>
      <DateRangePickerStartField aria-label="Start date" />
      <DateRangePickerEndField aria-label="End date" />
      <DateRangePickerTrigger>Choose range</DateRangePickerTrigger>
      <DateRangePickerCalendar aria-label="Date range" />
    </DateRangePicker>
  );
}
