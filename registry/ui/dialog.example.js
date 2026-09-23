// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Dialog from "./dialog.js";

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
    <Dialog.Root>
      <Dialog.Trigger>Rename project</Dialog.Trigger>
      <Dialog.Content>
        <Dialog.Header>
          <Dialog.Title>Rename project</Dialog.Title>
          <Dialog.Description>The new name is what everyone on the team sees.</Dialog.Description>
        </Dialog.Header>
        <label {...props(styles.field)}>
          Name
          <input {...props(styles.input)} defaultValue="Atlas" name="name" type="text" />
        </label>
        <Dialog.Footer>
          <Dialog.Close>Cancel</Dialog.Close>
          <Dialog.Close tone="primary">Save</Dialog.Close>
        </Dialog.Footer>
      </Dialog.Content>
    </Dialog.Root>
  );
}
