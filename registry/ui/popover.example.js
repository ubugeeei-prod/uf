// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Button } from "./button.js";
import { Popover, PopoverContent, PopoverTrigger } from "./popover.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space3,
  },
  note: {
    margin: 0,
    color: ufTokens.muted,
  },
  field: {
    display: "grid",
    gap: ufTokens.space1,
    fontWeight: ufTokens.weightMedium,
  },
  input: {
    boxSizing: "border-box",
    width: "100%",
    minHeight: "36px",
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.sunken,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
  },
});

/** A share link, beside the button that asked for it. */
export component Example() {
  return (
    <Popover>
      <PopoverTrigger>Share</PopoverTrigger>
      <PopoverContent>
        <div {...props(styles.stack)}>
          <p {...props(styles.note)}>Anyone with the link can read this page.</p>
          <div {...props(styles.field)}>
            <label htmlFor="share-link">Link</label>
            <input
              {...props(styles.input)}
              id="share-link"
              name="link"
              readOnly
              type="text"
              value="https://example.com/pages/1024"
            />
          </div>
          <Button size="sm" tone="primary">
            Copy link
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
