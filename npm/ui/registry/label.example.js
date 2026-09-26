// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Input } from "./input.js";
import { Label } from "./label.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space2,
    maxWidth: "20rem",
  },
});

/** A label naming the input its `htmlFor` points at, and saying it is required. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Label htmlFor="label-example-email">Email (required)</Label>
      <Input autoComplete="email" id="label-example-email" required type="email" />
    </div>
  );
}
