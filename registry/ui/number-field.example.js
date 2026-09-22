// @flow
import * as React from "@uniflowed/react";
import {
  NumberField,
  NumberFieldInput,
  NumberFieldIncrement,
  NumberFieldDecrement,
} from "./number-field.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <NumberField defaultValue={2} min={0} max={5}>
      <NumberFieldDecrement />
      <NumberFieldInput aria-label="Quantity" />
      <NumberFieldIncrement />
    </NumberField>
  );
}
