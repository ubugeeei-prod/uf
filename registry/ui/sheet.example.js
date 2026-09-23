// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Sheet from "./sheet.js";

const styles = stylex.create({
  option: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space2,
    minHeight: "32px",
  },
  box: {
    width: "16px",
    height: "16px",
    margin: 0,
    accentColor: ufTokens.accent,
  },
});

/** A panel of filters, against the right edge. */
export component Example() {
  return (
    <Sheet.Root side="right">
      <Sheet.Trigger>Filters</Sheet.Trigger>
      <Sheet.Content>
        <Sheet.Header>
          <Sheet.Title>Filters</Sheet.Title>
          <Sheet.Description>Narrow the list to what you are looking for.</Sheet.Description>
        </Sheet.Header>
        <div {...props(styles.option)}>
          <input {...props(styles.box)} id="filter-open" name="open" type="checkbox" />
          <label htmlFor="filter-open">Only open issues</label>
        </div>
        <div {...props(styles.option)}>
          <input {...props(styles.box)} id="filter-mine" name="mine" type="checkbox" />
          <label htmlFor="filter-mine">Only mine</label>
        </div>
        <Sheet.Footer>
          <Sheet.Close tone="primary">Show results</Sheet.Close>
        </Sheet.Footer>
      </Sheet.Content>
    </Sheet.Root>
  );
}
