// @flow
import * as React from "@uniflowed/react";
import {
  ColorPicker,
  ColorPickerInput,
  ColorPickerField,
  ColorPickerChannel,
  ColorPickerSwatch,
} from "./color-picker.js";

/** A named, keyboard-operable example using the default presentation. */
export component Example() {
  return (
    <ColorPicker defaultValue="#336699">
      <ColorPickerInput aria-label="Choose color" />
      <ColorPickerField aria-label="Hex color" />
      <ColorPickerChannel channel="red" aria-label="Red" />
      <ColorPickerSwatch />
    </ColorPicker>
  );
}
