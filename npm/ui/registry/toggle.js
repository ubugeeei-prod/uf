// @flow
//
// Toggle: a button that stays pressed until it is pressed again, for a mode
// such as bold or a filter that is on.
//
// `uf ui add toggle` wrote this file into the project, and it is the project's
// from then on. `uf ui diff toggle` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The look: two tones and three sizes, and a pressed state drawn from
// `aria-pressed` through `:is([aria-pressed=true])`. `@uniflowed/ui`'s `Toggle`
// owns `aria-pressed` itself and the press that flips it.
//
// # What to keep true when you change it
//
// * **Its name does not change with its state.** "Bold", pressed or not; a
//   name that says "Turn bold off" contradicts the pressed state a reader is
//   already told.
// * **An icon toggle needs `aria-label`,** and the icon `aria-hidden`.
// * **Pressed is more than a colour.** Pressed takes a background as well as the
//   `accent` text, so the state is not carried by the hue alone.
// * **The colours are measured pairs:** `ink` on `surfaceHover`, and `accent`
//   on `accentSoft` when pressed, both held to 4.5:1 by
//   `crates/uf_stylex/src/tests/preset.rs` in both themes.
// * **A group of toggles is a `toggle-group`,** which gives the row one tab
//   stop and arrow keys.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Toggle as TogglePart } from "@uniflowed/ui";

/** Whether a toggle has an edge while it is not pressed. */
export type ToggleTone = "ghost" | "outline";

/** How big it is. `icon` is square, for a toggle whose only content is an icon. */
export type ToggleSize = "sm" | "md" | "icon";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  base: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: ufTokens.space2,
    flexShrink: 0,
    boxSizing: "border-box",
    margin: 0,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    borderWidth: "1px",
    borderStyle: "solid",
    borderRadius: ufTokens.radiusMd,
    color: { default: ufTokens.ink, ":is([aria-pressed=true])": ufTokens.accent },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.surfaceHover,
      ":disabled": "transparent",
      ":is([aria-pressed=true])": ufTokens.accentSoft,
    },
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    opacity: { default: 1, ":disabled": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
    // The focus ring draws itself outward from the edge (`outline-width`
    // 0 → 2px) rather than blinking on, in `durationFast`: it is where a
    // keyboard reader's eye is, so it should arrive, not flash.
    transitionProperty: {
      default: "background-color, color, outline-width",
      "@media (prefers-reduced-motion: reduce)": "background-color, color",
    },
    transitionDuration: ufTokens.durationFast,
    transitionTimingFunction: ufTokens.easing,
  },
  ghost: {
    borderColor: "transparent",
  },
  outline: {
    borderColor: ufTokens.border,
  },
  sm: {
    minHeight: "32px",
    paddingInline: ufTokens.space2,
  },
  md: {
    minHeight: "36px",
    paddingInline: ufTokens.space3,
  },
  icon: {
    width: "36px",
    height: "36px",
    padding: 0,
  },
});

/** A toggle. Uncontrolled from `defaultPressed` unless `pressed` is given. */
export component Toggle(
  children?: React.Node,
  pressed?: boolean,
  defaultPressed?: boolean = false,
  onPressedChange?: (pressed: boolean) => void,
  disabled?: boolean = false,
  tone?: ToggleTone = "ghost",
  size?: ToggleSize = "md",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = props(
    styles.base,
    match (tone) {
      "ghost" => styles.ghost,
      "outline" => styles.outline,
    },
    match (size) {
      "sm" => styles.sm,
      "md" => styles.md,
      "icon" => styles.icon,
    },
    xstyle,
  );
  return (
    <TogglePart
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      defaultPressed={defaultPressed}
      disabled={disabled}
      onPressedChange={onPressedChange}
      pressed={pressed}
    >
      {children}
    </TogglePart>
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
