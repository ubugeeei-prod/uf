// @flow
//
// Checkbox: one answer in a form, checked, unchecked or mixed, drawn as a box
// beside its label.
//
// `uf ui add checkbox` wrote this file into the project, and it is the
// project's from then on. `uf ui diff checkbox` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The box, the tick, the dash and the label beside them, all inside the one
// button the part renders, so the whole row is the target and its words are
// the name. `@uniflowed/ui`'s `Checkbox` owns `role="checkbox"`, an `aria-checked`
// of `true`, `false` or `mixed`, `Space` toggling it, and `Enter` submitting
// the form it is in rather than toggling. The box follows `aria-checked`
// through `:is([aria-checked=true])` and `:is([aria-checked=mixed])`.
//
// # What to keep true when you change it
//
// * **Mixed is its own drawing, and its owner's to clear.** A "select all"
//   with some rows selected shows a dash, not a tick and not an empty box.
//   `indeterminate` is a prop rather than state: a press reports `true`
//   through `onCheckedChange`, and the rows it summarises decide what it shows
//   next.
// * **Its words are its name.** Put the label inside `Checkbox`; a checkbox
//   with no words needs `aria-label`.
// * **The box is an edge a reader can see.** Unchecked, a `muted` border on
//   `surface`; checked, an `accentInk` mark on `accent`. Both are pairs
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.
// * **A setting that applies at once is a `switch`,** which a reader is told
//   is on or off rather than checked.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Checkbox as CheckboxPart } from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    // Read by the box and its marks, which cannot see the button's state.
    "--uf-checkbox-fill": {
      default: ufTokens.surface,
      ":is([aria-checked=true])": ufTokens.accent,
      ":is([aria-checked=mixed])": ufTokens.accent,
    },
    "--uf-checkbox-edge": {
      default: ufTokens.muted,
      ":is([aria-checked=true])": ufTokens.accent,
      ":is([aria-checked=mixed])": ufTokens.accent,
    },
    "--uf-checkbox-tick": { default: "0", ":is([aria-checked=true])": "1" },
    "--uf-checkbox-dash": { default: "0", ":is([aria-checked=mixed])": "1" },
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space2,
    minHeight: "32px",
    margin: 0,
    padding: 0,
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    backgroundColor: "transparent",
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingTight,
    textAlign: "start",
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    opacity: { default: 1, ":disabled": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  box: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    boxSizing: "border-box",
    width: ufTokens.sizeControl,
    height: ufTokens.sizeControl,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--uf-checkbox-edge)",
    borderRadius: ufTokens.radiusSm,
    backgroundColor: "var(--uf-checkbox-fill)",
    color: ufTokens.accentInk,
  },
  tick: {
    position: "absolute",
    opacity: "var(--uf-checkbox-tick)",
  },
  dash: {
    position: "absolute",
    opacity: "var(--uf-checkbox-dash)",
  },
});

/** A checkbox and its label. Uncontrolled from `defaultChecked` unless `checked` is given. */
export component Checkbox(
  children?: React.Node,
  checked?: boolean,
  defaultChecked?: boolean = false,
  indeterminate?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <CheckboxPart
      {...forwarded(rest)}
      checked={checked}
      className={classNames(props(styles.root, xstyle).className, className)}
      defaultChecked={defaultChecked}
      disabled={disabled}
      indeterminate={indeterminate}
      onCheckedChange={onCheckedChange}
    >
      <span {...props(styles.box)} aria-hidden="true">
        <svg
          {...props(styles.tick)}
          fill="none"
          focusable="false"
          height="12"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="3"
          viewBox="0 0 24 24"
          width="12"
        >
          <path d="M20 6 9 17l-5-5" />
        </svg>
        <svg
          {...props(styles.dash)}
          fill="none"
          focusable="false"
          height="12"
          stroke="currentColor"
          strokeLinecap="round"
          strokeWidth="3"
          viewBox="0 0 24 24"
          width="12"
        >
          <path d="M5 12h14" />
        </svg>
      </span>
      {children}
    </CheckboxPart>
  );
}

/**
 * A caller's props on their way into a part rather than onto an element. Flow
 * checks that spread against the part's own `...rest`, whose `key` is `empty`
 * where this file's indexer says `mixed`; `@uniflowed/ui` papers over the same
 * hole the same way, and nothing checked is lost, because the elements its
 * parts render have `any`-typed props in uf's library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
