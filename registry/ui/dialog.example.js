// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "./dialog.js";

const styles = stylex.create({
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
    backgroundColor: ufTokens.surface,
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

/** A rename: a title, a description, one field, and the two ways out. */
export component Example() {
  return (
    <Dialog>
      <DialogTrigger>Rename project</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Rename project</DialogTitle>
          <DialogDescription>The new name is what everyone on the team sees.</DialogDescription>
        </DialogHeader>
        <label {...props(styles.field)}>
          Name
          <input {...props(styles.input)} defaultValue="Atlas" name="name" type="text" />
        </label>
        <DialogFooter>
          <DialogClose>Cancel</DialogClose>
          <DialogClose tone="primary">Save</DialogClose>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
