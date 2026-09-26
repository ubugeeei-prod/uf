// @flow
//
// InputGroup: an input with text, icons or buttons attached to its sides,
// drawn as one field.
//
// `uf ui add input-group` wrote this file into the project, and it is the
// project's from then on. `uf ui diff input-group` shows how it has moved away
// from the registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// The input is an `<input>`, and the addons are text and buttons the platform
// already makes accessible. The one thing that looks like behaviour, the
// focus ring drawn round the whole group while the input has focus, is
// `:focus-within` in CSS. So `@uniflowed/ui` declines an `InputGroup`
// (`crates/uf_lib/src/ui.rs` records it), and this file is the look: a border
// round the input and its addons, the ring on the group, and the addons in the
// quieter colour.
//
// # What to keep true when you change it
//
// * **An addon is not the input's name.** "USD" beside a field is read as
//   nothing when the field is reached, because a screen reader announces the
//   input's label, not the text beside it. The label says "Amount in USD", with
//   a `<label htmlFor>` (see `label.js`) or an `aria-label` on `InputGroup.Input`.
// * **Decoration is hidden, words are not.** An icon that repeats the label
//   (a magnifier beside "Search") goes in an addon with `aria-hidden`. Text a
//   reader needs ("https://") stays readable; `aria-describedby` on the input,
//   pointing at the addon's `id`, is how it is heard with the input.
// * **A button in an addon is a real button with a name.** A clear or copy
//   button is `button.js` at `size="icon"` with an `aria-label`, and it is its
//   own tab stop after the input.
// * **The ring is the group's.** The input draws no outline of its own, and
//   the group draws two pixels of `ufTokens.focus` while anything inside it has
//   focus. Removing one without the other leaves a keyboard user with no ring.
// * **Invalid is an attribute, and more than a colour.** The group's border
//   turns `danger` when the input has `aria-invalid="true"`. Say what is wrong
//   in words, near the field.
// * **Everything reaches the input.** `type`, `name`, `value`, `onChange`,
//   `autoComplete`, `inputMode`, a `ref` and every `aria-*` on
//   `InputGroup.Input` land on the `<input>`.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface`, which
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
  root: {
    display: "flex",
    alignItems: "center",
    boxSizing: "border-box",
    width: "100%",
    minHeight: "36px",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    color: ufTokens.ink,
    backgroundColor: {
      default: ufTokens.surface,
      ":has(input:disabled)": ufTokens.sunken,
    },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: {
      default: ufTokens.border,
      ":has([aria-invalid=true])": ufTokens.danger,
    },
    borderRadius: ufTokens.radiusMd,
    outlineWidth: { default: "0", ":focus-within": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
  },
  input: {
    flexGrow: 1,
    flexShrink: 1,
    minWidth: 0,
    boxSizing: "border-box",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: "inherit",
    fontSize: "inherit",
    lineHeight: "inherit",
    color: { default: ufTokens.ink, ":disabled": ufTokens.muted },
    backgroundColor: "transparent",
    borderWidth: 0,
    borderRadius: 0,
    // The group draws the ring; see the header.
    outlineStyle: "none",
    cursor: { default: "text", ":disabled": "not-allowed" },
    "::placeholder": {
      color: ufTokens.muted,
      opacity: 1,
    },
  },
  addon: {
    display: "inline-flex",
    alignItems: "center",
    flexShrink: 0,
    gap: ufTokens.space1,
    paddingInline: ufTokens.space3,
    color: ufTokens.muted,
    whiteSpace: "nowrap",
  },
});

/** The frame round the input and its addons. */
component InputGroupRoot(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.root, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** The input itself. Its label is its name; see the header. */
component InputGroupInput(
  type?: string = "text",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <input
      {...rest}
      className={classNames(props(styles.input, xstyle).className, className)}
      type={type}
    />
  );
}

/** Text, an icon or a button beside the input, before or after it in reading order. */
component InputGroupAddon(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <span {...rest} className={classNames(props(styles.addon, xstyle).className, className)}>
      {children}
    </span>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}

/**
 * The parts, under the names `import * as InputGroup from "./input-group.js"`
 * gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes
 * `<InputGroup.Root>` and `<InputGroup.Input>`. Each is declared under its
 * full name, so React DevTools and an error say `InputGroupInput` rather than
 * `Input`.
 */
export { InputGroupRoot as Root, InputGroupInput as Input, InputGroupAddon as Addon };
