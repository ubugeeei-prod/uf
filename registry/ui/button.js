// @flow
//
// Button: a `<button>` in four tones and four sizes, styled with uf's tokens.
//
// `uf ui add button` wrote this file into the project, and from then on it is
// the project's: change the tones, the sizes, the markup. `uf ui diff button`
// shows how it differs from the registry in the uf you are running, and the
// comment `uf ui add` put on its last line records which uf this copy began as.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// The platform's `<button>` already has the role, the keyboard and the focus,
// so a headless package has nothing to add to it, and `@uniflowed/ui` declines
// a Button for exactly that reason. What an application wants beside it is one
// button its pages agree on. That is a decision about this application, so it
// belongs in the application's repository, which is where this file is.
//
// Everything a caller passes reaches the element: `form`, `formAction`,
// `name`, `value`, a `ref`, every `aria-*` and every handler. A wrapper that
// forwarded a chosen few would lose the rest quietly, which is the objection
// `@uniflowed/ui`'s header makes to wrapping a `<button>` at all.
//
// # What to keep true when you change it
//
// * **`type` is `"button"` unless you say otherwise.** A `<button>` inside a
//   form submits it by default, and a Cancel that posts the form is the classic
//   bug. A submit button says `type="submit"`.
// * **The focus ring is drawn for the keyboard.** `:focus-visible`, two pixels
//   of `ufTokens.focus` two pixels outside the edge, which is the 3:1 a focus
//   indicator needs against `surface` and `canvas` in both shipped themes. A
//   button without it is one a keyboard user cannot find.
// * **Every size is a target a finger can hit.** The smallest is 32px tall,
//   over the 24px WCAG 2.5.8 asks for, and `icon` is 36px square.
// * **An icon button needs a name.** `size="icon"` holds no text, so pass
//   `aria-label`, and give the icon `aria-hidden`.
// * **Disabled has two spellings, and both look disabled.** `disabled` takes the
//   button out of the tab order. `aria-disabled="true"` keeps it focusable, so
//   the reason it is unavailable can still be reached and read; ignoring the
//   click is then yours to do.
// * **Colour comes from tokens, in pairs that are measured.** Every text colour
//   here sits on the background `@uniflowed/stylex`'s own button puts it on,
//   and `crates/uf_stylex/src/tests/preset.rs` holds those pairs to 4.5:1 in
//   the light default and the dark theme. `ufAutoTheme`, `ufDarkTheme` and a
//   theme of your own restyle every tone without an edit here; a literal colour
//   is where that stops being true.
// * **A press is felt.** The button gives to 97% of its size while it is held
//   and comes back on release, and a keyboard focus ring draws outward from
//   its edge; both in `durationFast`. Under `prefers-reduced-motion: reduce`
//   neither moves, and only the colours still change.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/** How loud a button is, and therefore what it is for. */
export type ButtonTone = "primary" | "neutral" | "ghost" | "danger";

/** How big it is. `icon` is square, for a button whose only content is an icon. */
export type ButtonSize = "sm" | "md" | "lg" | "icon";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * element. `key` is named `empty` because React takes it off before a component
 * is called, which is what lets the rest be spread onto a `<button>`.
 */
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
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    whiteSpace: "nowrap",
    textDecoration: "none",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "transparent",
    borderRadius: ufTokens.radiusMd,
    cursor: {
      default: "pointer",
      ":disabled": "not-allowed",
      ":is([aria-disabled=true])": "not-allowed",
    },
    opacity: {
      default: 1,
      ":disabled": 0.55,
      ":is([aria-disabled=true])": 0.55,
    },
    // The ring is drawn for a keyboard focus only, which is what
    // `:focus-visible` is for: a click should not light the control up.
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
    // Press: the button gives a little under the pointer, to 97% of its size
    // in `durationFast`, and comes back on release without passing 1. A
    // disabled button does not move, and under reduced motion none does: only
    // the colours still change.
    transform: {
      default: "none",
      ":active": "scale(0.97)",
      ":disabled": "none",
      ":is([aria-disabled=true])": "none",
      "@media (prefers-reduced-motion: reduce)": "none",
    },
    // The focus ring draws itself outward from the edge (`outline-width`
    // 0 → 2px) rather than blinking on, in `durationFast`: it is where a
    // keyboard reader's eye is, so it should arrive, not flash.
    transitionProperty: {
      default: "background-color, border-color, color, outline-width, transform",
      "@media (prefers-reduced-motion: reduce)": "background-color, border-color, color",
    },
    transitionDuration: ufTokens.durationFast,
    transitionTimingFunction: ufTokens.easing,
  },
  // Each tone restates its resting colour for both disabled states, because
  // `:hover` would otherwise still repaint a button that does nothing.
  primary: {
    backgroundColor: {
      default: ufTokens.accent,
      ":hover": ufTokens.accentHover,
      ":disabled": ufTokens.accent,
      ":is([aria-disabled=true])": ufTokens.accent,
    },
    color: ufTokens.accentInk,
  },
  neutral: {
    backgroundColor: {
      default: ufTokens.surface,
      ":hover": ufTokens.surfaceHover,
      ":disabled": ufTokens.surface,
      ":is([aria-disabled=true])": ufTokens.surface,
    },
    borderColor: ufTokens.border,
    color: ufTokens.ink,
  },
  ghost: {
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.surfaceHover,
      ":disabled": "transparent",
      ":is([aria-disabled=true])": "transparent",
    },
    color: ufTokens.ink,
  },
  danger: {
    backgroundColor: {
      default: ufTokens.danger,
      ":hover": ufTokens.dangerHover,
      ":disabled": ufTokens.danger,
      ":is([aria-disabled=true])": ufTokens.danger,
    },
    color: ufTokens.dangerInk,
  },
  sm: {
    minHeight: "32px",
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space3,
    fontSize: ufTokens.textSm,
  },
  md: {
    minHeight: "36px",
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space4,
    fontSize: ufTokens.textSm,
  },
  lg: {
    minHeight: "44px",
    paddingBlock: ufTokens.space3,
    paddingInline: ufTokens.space6,
    fontSize: ufTokens.textMd,
  },
  icon: {
    width: "36px",
    height: "36px",
    padding: 0,
    fontSize: ufTokens.textSm,
  },
});

/**
 * A button.
 *
 *     <Button tone="primary" type="submit">Save</Button>
 *     <Button aria-label="Close" size="icon"><CloseIcon aria-hidden /></Button>
 *
 * `xstyle` takes a `stylex.create` namespace and wins property by property,
 * which is how to change one declaration here from the call site. `className`
 * adds a class of your own beside these.
 */
export component Button(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  type?: "button" | "submit" | "reset" = "button",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const styled = props(
    styles.base,
    match (tone) {
      "primary" => styles.primary,
      "neutral" => styles.neutral,
      "ghost" => styles.ghost,
      "danger" => styles.danger,
    },
    match (size) {
      "sm" => styles.sm,
      "md" => styles.md,
      "lg" => styles.lg,
      "icon" => styles.icon,
    },
    xstyle,
  );
  return (
    <button {...rest} className={classNames(styled.className, className)} type={type}>
      {children}
    </button>
  );
}

/**
 * The classes this file chose, then the caller's.
 *
 * Appended rather than merged: a class from a stylesheet cannot be ordered
 * against StyleX's property by property, so `xstyle` is the way to replace a
 * declaration here and `className` is the way to add one.
 */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
