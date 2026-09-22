// @flow
import * as React from "@uniflowed/react";
import { DateField } from "./date-field.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return <DateField aria-label="Start date" defaultValue="2026-09-14" />;
}
