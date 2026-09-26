// @flow
//
// Label: a styled `<label>`, which names the control its `htmlFor` points at.
//
// `uf ui add label` wrote this file into the project, and it is the project's
// from then on. `uf ui diff label` shows how it has moved away from the
// registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// The platform already does everything a label needs: `<label htmlFor>` gives
// the control its accessible name and makes a click on the label focus or
// toggle the control. `@uniflowed/ui` declines a Label for that reason, and
// `field.js`'s `FieldLabel` is the one to use inside a field row, where the id
// is wired for you. This file is the label for a control on its own, such as
// an `Input`, a `Checkbox` or a `Switch`, drawn the same way `FieldLabel` is.
//
// # What to keep true when you change it
//
// * **It stays a `<label>`.** A `<span>` that looks the same names nothing,
//   and clicking it does nothing. `htmlFor` must match the control's `id`, or
//   the control has no name.
// * **Required is said in words.** An asterisk alone is read as "star" or not
//   at all. Put `required` on the control, which is announced, and write
//   "(required)" in the label when the page does not say it elsewhere.
// * **Text stays on a measured pair.** `ink` on `surface` and `canvas`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both shipped
//   themes.

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
  label: {
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space2,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    color: ufTokens.ink,
    cursor: "default",
  },
});

/**
 * A label.
 *
 *     <Label htmlFor="email">Email</Label>
 *     <Input id="email" type="email" />
 */
export component Label(
  children: React.Node,
  htmlFor?: string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <label
      {...rest}
      className={classNames(props(styles.label, xstyle).className, className)}
      htmlFor={htmlFor}
    >
      {children}
    </label>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
