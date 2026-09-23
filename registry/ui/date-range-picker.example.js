// @flow
import * as React from "@uniflowed/react";
import * as DateRangePicker from "./date-range-picker.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <DateRangePicker.Root defaultValue={{ start: "2026-09-14", end: "2026-09-16" }}>
      <DateRangePicker.StartField aria-label="Start date" />
      <DateRangePicker.EndField aria-label="End date" />
      <DateRangePicker.Trigger>Choose range</DateRangePicker.Trigger>
      <DateRangePicker.Calendar aria-label="Date range" />
    </DateRangePicker.Root>
  );
}
