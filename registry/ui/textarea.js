// @flow
//
// Textarea: a styled multi-line `<textarea>`, for a control that has no field
// row around it.
//
// `uf ui add textarea` wrote this file into the project, and it is the
// project's from then on. `uf ui diff textarea` shows how it has moved away
// from the registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// For the same reasons as `input.js`. A `<textarea>` already is the component,
// so `@uniflowed/ui` declines to wrap it, and `field.js`'s `FieldTextarea` is
// the one to use inside a labelled row. This file draws the text area on its
// own, the same way `FieldTextarea` draws it.
//
// # What to keep true when you change it
//
// * **Every text area has a name a reader hears.** Use a `<label htmlFor>` or
//   an `aria-label`. A placeholder is not a name.
// * **It grows the way a reader expects.** The browser resizes it vertically
//   only, so a reader can make room for a long answer without breaking the
//   page's width. `rows` sets where it starts.
// * **Invalid is an attribute, and more than a colour.** The border follows
//   `aria-invalid="true"`, and the words saying what is wrong sit near it.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface` and
//   `sunken`, which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in
//   both shipped themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * element. `key` is `empty` because React takes it off before a component is
 * called.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  control: {
    boxSizing: "border-box",
    width: "100%",
    minHeight: "5rem",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    color: { default: ufTokens.ink, ":disabled": ufTokens.muted },
    backgroundColor: { default: ufTokens.surface, ":disabled": ufTokens.sunken },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-invalid=true])": ufTokens.danger },
    borderRadius: ufTokens.radiusMd,
    cursor: { default: "text", ":disabled": "not-allowed" },
    resize: "vertical",
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
    "::placeholder": {
      color: ufTokens.muted,
      opacity: 1,
    },
  },
});

/**
 * A text area.
 *
 *     <Label htmlFor="note">Note</Label>
 *     <Textarea id="note" name="note" rows={4} />
 */
export component Textarea(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <textarea
      {...rest}
      className={classNames(props(styles.control, xstyle).className, className)}
    />
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
