// @flow
//
// ButtonGroup: related buttons joined into one bar, named as a group.
//
// `uf ui add button-group` wrote this file into the project, and it is the
// project's from then on. `uf ui diff button-group` shows how it has moved
// away from the registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// A button group is `role="group"` around some buttons, with their inner
// corners squared. The role is the one attribute in it, and every button keeps
// the keys the platform gives it, so `@uniflowed/ui` declines a `ButtonGroup`
// (`crates/uf_lib/src/ui.rs` records it). What an application wants is the
// joined look, the same everywhere, and that is written here.
//
// # What to keep true when you change it
//
// * **The group has a name.** `role="group"` with no `aria-label` or
//   `aria-labelledby` is announced as "group" and nothing else. `label` is
//   required for that reason, and lands as `aria-label`.
// * **Every button is its own tab stop.** `Tab` walks through them in order,
//   the way it walks through any buttons. A bar where the arrow keys move
//   between the buttons and `Tab` leaves it in one step is a toolbar, which is
//   a keyboard contract rather than a look; do not add arrow keys here without
//   the rest of it.
// * **A choice of one is not a button group.** Buttons where exactly one is on
//   ("Day", "Week", "Month") are a `ToggleGroup`, whose `type="single"` is the
//   radio pattern and announces which one is chosen.
// * **An icon-only item needs a name.** Pass `aria-label`, and give the icon
//   `aria-hidden`.
// * **`type` is `"button"` unless you say otherwise,** as in `button.js`: an
//   item inside a form does not submit it by accident.
// * **The focus ring is drawn over the neighbours.** A focused item is raised
//   above the ones beside it, so its ring is not cut off by their borders.
// * **Text stays on measured pairs.** `ink` on `surface` and `surfaceHover`,
//   which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both shipped
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
    display: "inline-flex",
    alignItems: "stretch",
    isolation: "isolate",
  },
  item: {
    position: "relative",
    zIndex: { default: 0, ":hover": 1, ":focus-visible": 2 },
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: ufTokens.space2,
    boxSizing: "border-box",
    minHeight: "36px",
    margin: 0,
    // Each item after the first overlaps the one before by its border, so
    // two neighbours draw one line between them rather than two.
    marginInlineStart: { default: "-1px", ":first-child": 0 },
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space4,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    whiteSpace: "nowrap",
    color: ufTokens.ink,
    backgroundColor: {
      default: ufTokens.surface,
      ":hover": ufTokens.surfaceHover,
      ":disabled": ufTokens.surface,
    },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    // Square inside, rounded at the two ends of the bar.
    borderRadius: 0,
    borderStartStartRadius: { default: 0, ":first-child": ufTokens.radiusMd },
    borderEndStartRadius: { default: 0, ":first-child": ufTokens.radiusMd },
    borderStartEndRadius: { default: 0, ":last-child": ufTokens.radiusMd },
    borderEndEndRadius: { default: 0, ":last-child": ufTokens.radiusMd },
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    opacity: { default: 1, ":disabled": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
    // Only colour changes, so it is the same under reduced motion.
    transitionProperty: "background-color",
    transitionDuration: ufTokens.durationFast,
    transitionTimingFunction: ufTokens.easing,
  },
});

/** The bar, named as a group by `label`. */
component ButtonGroupRoot(
  children: React.Node,
  label: string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div
      {...rest}
      aria-label={label}
      className={classNames(props(styles.root, xstyle).className, className)}
      role="group"
    >
      {children}
    </div>
  );
}

/** One button in the bar. */
component ButtonGroupItem(
  children: React.Node,
  type?: "button" | "submit" | "reset" = "button",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <button
      {...rest}
      className={classNames(props(styles.item, xstyle).className, className)}
      type={type}
    >
      {children}
    </button>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}

/**
 * The parts, under the names `import * as ButtonGroup from "./button-group.js"`
 * gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes
 * `<ButtonGroup.Root>` and `<ButtonGroup.Item>`. Each is declared under its
 * full name, so React DevTools and an error say `ButtonGroupItem` rather than
 * `Item`.
 */
export { ButtonGroupRoot as Root, ButtonGroupItem as Item };
