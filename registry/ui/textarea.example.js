// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Label } from "./label.js";
import { Textarea } from "./textarea.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space2,
    maxWidth: "28rem",
  },
});

/** A labelled text area that starts four rows tall. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Label htmlFor="textarea-example-note">Release note</Label>
      <Textarea
        id="textarea-example-note"
        name="note"
        placeholder="What changed, for someone who was not there"
        rows={4}
      />
    </div>
  );
}
