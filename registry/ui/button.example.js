// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Button } from "./button.js";

const styles = stylex.create({
  row: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: ufTokens.space2,
  },
});

/** Every tone, the sizes, both spellings of disabled, and an icon button. */
export component Example() {
  return (
    <div {...props(styles.row)}>
      <Button tone="primary">Save</Button>
      <Button>Cancel</Button>
      <Button tone="ghost">Skip</Button>
      <Button tone="danger">Delete</Button>
      <Button size="sm">Small</Button>
      <Button size="lg">Large</Button>
      <Button disabled tone="primary">
        Saving…
      </Button>
      <Button aria-disabled="true">Not yet</Button>
      <Button aria-label="Add a row" size="icon">
        <svg
          aria-hidden="true"
          fill="none"
          focusable="false"
          height="16"
          stroke="currentColor"
          strokeLinecap="round"
          strokeWidth="2"
          viewBox="0 0 24 24"
          width="16"
        >
          <path d="M12 5v14M5 12h14" />
        </svg>
      </Button>
    </div>
  );
}
