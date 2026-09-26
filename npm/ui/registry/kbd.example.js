// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Kbd } from "./kbd.js";

const styles = stylex.create({
  text: {
    margin: 0,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    color: ufTokens.ink,
  },
});

/** A single key and a combination, in the sentence that needs them. */
export component Example() {
  return (
    <p {...props(styles.text)}>
      Press <Kbd keys={["Ctrl", "K"]} /> to search, and <Kbd>Esc</Kbd> to close it.
    </p>
  );
}
