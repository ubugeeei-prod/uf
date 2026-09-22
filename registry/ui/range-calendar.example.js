// @flow
import * as React from "@uniflowed/react";
import { RangeCalendar } from "./range-calendar.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <RangeCalendar
      defaultValue={{ start: "2026-09-14", end: "2026-09-16" }}
      locale="en-US"
      today="2026-09-14"
    />
  );
}
