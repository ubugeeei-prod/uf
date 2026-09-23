// @flow
import * as React from "@uniflowed/react";
import * as ColorPicker from "./color-picker.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <ColorPicker.Root defaultValue="#336699">
      <ColorPicker.Input aria-label="Choose color" />
      <ColorPicker.Field aria-label="Hex color" />
      <ColorPicker.Channel channel="red" aria-label="Red" />
      <ColorPicker.Swatch />
    </ColorPicker.Root>
  );
}
