// @flow
//
// Input: a styled single-line `<input>`, for a search box or any control that
// has no field row around it.
//
// `uf ui add input` wrote this file into the project, and it is the project's
// from then on. `uf ui diff input` shows how it has moved away from the
// registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// An `<input>` already is the component. It has a role, a value, a keyboard
// and a form, and `@uniflowed/ui` declines to wrap it for that reason. When
// the input is part of a labelled row with help text and an error, use
// `field.js`, whose `Field` wires the ids and `aria-describedby`. This file is
// for the input on its own, drawn the same way `FieldInput` draws it, so the
// two match on one page.
//
// # What to keep true when you change it
//
// * **Every input has a name a reader hears.** Use a `<label htmlFor>` (see
//   `label.js`) or an `aria-label`. A placeholder is not a name: it disappears
//   once a reader types, and it is too faint to be text anyone has to rely on.
// * **Invalid is an attribute, and more than a colour.** The border turns
//   `danger` under `aria-invalid="true"`, so the look cannot drift from what is
//   announced. Say what is wrong in words, near the input.
// * **The focus ring is drawn for the keyboard.** `:focus-visible` shows two
//   pixels of `ufTokens.focus`.
// * **Nothing is forwarded selectively.** `type`, `name`, `value`,
//   `onChange`, `autoComplete`, `inputMode`, a `ref` and every `aria-*` reach
//   the element. A wrapper that passed through only some of them would drop the
//   rest without saying so.
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
    minHeight: "36px",
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
    borderRadius: ufTokens.radiusSm,
    cursor: { default: "text", ":disabled": "not-allowed" },
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
 * An input.
 *
 *     <Label htmlFor="q">Search</Label>
 *     <Input id="q" type="search" name="q" />
 *
 * `xstyle` takes a `stylex.create` namespace and wins property by property;
 * `className` adds a class of your own beside these.
 */
export component Input(
  type?: string = "text",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <input
      {...rest}
      className={classNames(props(styles.control, xstyle).className, className)}
      type={type}
    />
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
