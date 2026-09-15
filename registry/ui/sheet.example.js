// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from "./sheet.js";

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
    <Sheet side="right">
      <SheetTrigger>Filters</SheetTrigger>
      <SheetContent>
        <SheetHeader>
          <SheetTitle>Filters</SheetTitle>
          <SheetDescription>Narrow the list to what you are looking for.</SheetDescription>
        </SheetHeader>
        <div {...props(styles.option)}>
          <input {...props(styles.box)} id="filter-open" name="open" type="checkbox" />
          <label htmlFor="filter-open">Only open issues</label>
        </div>
        <div {...props(styles.option)}>
          <input {...props(styles.box)} id="filter-mine" name="mine" type="checkbox" />
          <label htmlFor="filter-mine">Only mine</label>
        </div>
        <SheetFooter>
          <SheetClose tone="primary">Show results</SheetClose>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}
