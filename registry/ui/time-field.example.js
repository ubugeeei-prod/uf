// @flow
import * as React from "@uniflowed/react";
import { TimeField } from "./time-field.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return <TimeField aria-label="Meeting time" defaultValue="14:30" hourCycle="h23" />;
}
