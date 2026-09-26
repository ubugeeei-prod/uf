// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as InputGroup from "./input-group.js";
import { Label } from "./label.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space2,
    maxWidth: "20rem",
  },
});

/**
 * A search box with a decorative icon, a website whose prefix is read with
 * it, and an amount whose currency is in its label.
 */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Label htmlFor="input-group-example-search">Search</Label>
      <InputGroup.Root>
        <InputGroup.Addon aria-hidden="true">
          <svg
            fill="none"
            focusable="false"
            height="16"
            stroke="currentColor"
            strokeLinecap="round"
            strokeWidth="2"
            viewBox="0 0 24 24"
            width="16"
          >
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-3.5-3.5" />
          </svg>
        </InputGroup.Addon>
        <InputGroup.Input id="input-group-example-search" name="q" type="search" />
      </InputGroup.Root>
      <Label htmlFor="input-group-example-site">Website</Label>
      <InputGroup.Root>
        <InputGroup.Addon id="input-group-example-site-prefix">https://</InputGroup.Addon>
        <InputGroup.Input
          aria-describedby="input-group-example-site-prefix"
          id="input-group-example-site"
          name="site"
        />
      </InputGroup.Root>
      <Label htmlFor="input-group-example-amount">Amount in USD</Label>
      <InputGroup.Root>
        <InputGroup.Addon aria-hidden="true">$</InputGroup.Addon>
        <InputGroup.Input
          aria-invalid="true"
          defaultValue="-5"
          id="input-group-example-amount"
          inputMode="decimal"
          name="amount"
        />
        <InputGroup.Addon aria-hidden="true">USD</InputGroup.Addon>
      </InputGroup.Root>
    </div>
  );
}
