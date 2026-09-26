// @flow
//
// NativeSelect: the platform's `<select>`, drawn to match the registry's
// inputs, with a chevron in place of the one the browser draws.
//
// `uf ui add native-select` wrote this file into the project, and it is the
// project's from then on. `uf ui diff native-select` shows how it has moved
// away from the registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// A `<select>` already is the component: every screen reader announces it, a
// phone opens its own picker for it, and a form submits, validates and resets
// it without a line of script. `@uniflowed/ui` declines to wrap it for that
// reason (`crates/uf_lib/src/ui.rs` records it), and its `Select` is the other
// answer, for options that need more than text. What is left is the look,
// which this file owns: the field the other inputs are drawn as, and a chevron.
//
// # Use this before `select.js`
//
// Reach for `select.js` only when an option needs an icon, a second line or a
// check the platform cannot draw. Everything else is better native.
//
// # What to keep true when you change it
//
// * **It stays a real `<select>`.** `name`, `value`, `defaultValue`,
//   `onChange`, `required`, `form`, `autoComplete`, a `ref` and every `aria-*`
//   reach the element, and the options are the caller's own `<option>` and
//   `<optgroup>` elements. A wrapper that rebuilt the list from data would lose
//   `<optgroup>` and disabled options without saying so.
// * **Every select has a name a reader hears.** Use a `<label htmlFor>` (see
//   `label.js`) or an `aria-label`. The first option is not a name.
// * **The chevron is decoration.** It is `aria-hidden`, and it lets a click
//   through to the `<select>` under it, so pressing on it opens the list.
//   `multiple` draws no chevron, because a list box shows its rows and has
//   nothing to open.
// * **Invalid is an attribute, and more than a colour.** The border turns
//   `danger` under `aria-invalid="true"`, as `input.js` does. Say what is wrong
//   in words, near the select.
// * **The focus ring is drawn for the keyboard.** `:focus-visible` shows two
//   pixels of `ufTokens.focus`.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface` and
//   `sunken`, which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in
//   both shipped themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * `<select>`. `key` is `empty` because React takes it off before a component
 * is called.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  // The select and the chevron share one grid cell, so the chevron sits over
  // the select's end without being positioned against anything.
  wrapper: {
    display: "inline-grid",
    alignItems: "center",
    width: "100%",
    minWidth: "12rem",
  },
  control: {
    gridArea: "1 / 1",
    boxSizing: "border-box",
    width: "100%",
    minHeight: "36px",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInlineStart: ufTokens.space3,
    paddingInlineEnd: ufTokens.space8,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingTight,
    color: { default: ufTokens.ink, ":disabled": ufTokens.muted },
    backgroundColor: { default: ufTokens.surface, ":disabled": ufTokens.sunken },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-invalid=true])": ufTokens.danger },
    borderRadius: ufTokens.radiusMd,
    // The browser's own arrow goes, and the chevron below replaces it.
    appearance: "none",
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
  },
  // A list box shows its rows, so it needs no room for a chevron.
  multiple: {
    paddingInlineEnd: ufTokens.space3,
  },
  chevron: {
    gridArea: "1 / 1",
    justifySelf: "end",
    marginInlineEnd: ufTokens.space3,
    color: ufTokens.muted,
    pointerEvents: "none",
  },
});

/**
 * A native select.
 *
 *     <Label htmlFor="region">Region</Label>
 *     <NativeSelect id="region" name="region" defaultValue="eu">
 *       <option value="us">United States</option>
 *       <option value="eu">Europe</option>
 *     </NativeSelect>
 *
 * `xstyle` and `className` go on the `<select>`: `xstyle` takes a
 * `stylex.create` namespace and wins property by property, and `className`
 * adds a class of your own beside these.
 */
export component NativeSelect(
  children: React.Node,
  multiple?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <span {...props(styles.wrapper)}>
      <select
        {...rest}
        className={classNames(
          props(styles.control, multiple && styles.multiple, xstyle).className,
          className,
        )}
        multiple={multiple}
      >
        {children}
      </select>
      {multiple ? null : <ChevronIcon />}
    </span>
  );
}

/** The chevron, where the browser's own arrow was. */
component ChevronIcon() {
  return (
    <svg
      {...props(styles.chevron)}
      aria-hidden="true"
      fill="none"
      focusable="false"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
