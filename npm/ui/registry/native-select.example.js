// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Label } from "./label.js";
import { NativeSelect } from "./native-select.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space2,
    maxWidth: "20rem",
  },
});

/** A labelled select with option groups, an invalid one, and a disabled one. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Label htmlFor="native-select-example-region">Region</Label>
      <NativeSelect defaultValue="eu-west" id="native-select-example-region" name="region">
        <optgroup label="Americas">
          <option value="us-east">US East</option>
          <option value="us-west">US West</option>
        </optgroup>
        <optgroup label="Europe">
          <option value="eu-west">EU West</option>
          <option disabled value="eu-north">
            EU North (full)
          </option>
        </optgroup>
      </NativeSelect>
      <Label htmlFor="native-select-example-plan">Plan</Label>
      <NativeSelect aria-invalid="true" defaultValue="" id="native-select-example-plan" required>
        <option value="">Choose a plan</option>
        <option value="hobby">Hobby</option>
        <option value="team">Team</option>
      </NativeSelect>
      <Label htmlFor="native-select-example-currency">Currency</Label>
      <NativeSelect defaultValue="usd" disabled id="native-select-example-currency">
        <option value="usd">US dollar</option>
      </NativeSelect>
    </div>
  );
}
