// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Button } from "./button.js";
import { Spinner } from "./spinner.js";

const styles = stylex.create({
  row: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: ufTokens.space4,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
});

/**
 * Three sizes, one of them named for what it is loading, and one inside a
 * button whose words already say it, where the spinner is silent.
 */
export component Example() {
  return (
    <div {...props(styles.row)}>
      <Spinner size="sm" />
      <Spinner label="Loading invoices" />
      <Spinner size="lg" />
      <Button aria-disabled="true" tone="primary">
        <Spinner label={null} size="sm" tone="inherit" />
        Saving…
      </Button>
    </div>
  );
}
