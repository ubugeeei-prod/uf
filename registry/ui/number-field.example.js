// @flow
import * as React from "@uniflowed/react";
import * as NumberField from "./number-field.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <NumberField.Root defaultValue={2} min={0} max={5}>
      <NumberField.Decrement />
      <NumberField.Input aria-label="Quantity" />
      <NumberField.Increment />
    </NumberField.Root>
  );
}
