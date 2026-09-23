"use client";
// @flow
//
// Menubar: a row of menus, as along the top of a desktop application, moved
// along with the arrow keys.
//
// `uf ui add menubar` wrote this file into the project, and it is the
// project's from then on. `uf ui diff menubar` shows how it has moved away from
// the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The bar and its triggers. The panels and their rows are `menu.js`'s parts
// under menubar names, so a menubar's menus and a menu look alike and change
// together. `@uniflowed/ui`'s `Menubar` owns the pattern: `role="menubar"`, one tab
// stop for the whole bar with the arrow keys moving between triggers, each
// trigger opening and naming its menu, and moving on to the next menu while
// one is open. A trigger is drawn highlighted while `aria-expanded` says its
// menu is open.
//
// # What to keep true when you change it
//
// * **Name the bar.** An `aria-label` on `Menubar` says what the menus are for
//   when a page has more than one bar.
// * **Keep the ring.** Triggers take focus one at a time from the keyboard, and
//   the outline is how a sighted keyboard user follows it.
// * **Text stays on measured pairs.** `ink` on `surface`, and `accent` on
//   `accentSoft` while highlighted, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

import {
  MenuCheckboxItem,
  MenuContent,
  MenuGroup,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
  MenuShortcut,
  MenuSub,
  MenuSubTrigger,
} from "./menu.js";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  bar: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space1,
    boxSizing: "border-box",
    padding: ufTokens.space1,
    backgroundColor: ufTokens.surface,
    fontFamily: ufTokens.fontSans,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
  trigger: {
    display: "inline-flex",
    alignItems: "center",
    minHeight: "32px",
    margin: 0,
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space3,
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    cursor: "pointer",
    userSelect: "none",
    color: {
      default: ufTokens.ink,
      ":hover": ufTokens.accent,
      ":focus-visible": ufTokens.accent,
      ":is([aria-expanded=true])": ufTokens.accent,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.accentSoft,
      ":focus-visible": ufTokens.accentSoft,
      ":is([aria-expanded=true])": ufTokens.accentSoft,
    },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
  },
});

/** The bar. Every child is a `MenubarMenu`. */
export component Menubar(
  children: renders* MenubarMenu,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Menubar.Root
      {...forwarded(rest)}
      className={classNames(props(styles.bar, xstyle).className, className)}
    >
      {children}
    </Primitive.Menubar.Root>
  );
}

/** One menu on the bar: a `MenubarTrigger` and the `MenubarContent` it opens. */
export component MenubarMenu(children: React.Node, value: string) renders Primitive.Menubar.Menu {
  return <Primitive.Menubar.Menu value={value}>{children}</Primitive.Menubar.Menu>;
}

/** The button on the bar that opens its menu and names it. */
export component MenubarTrigger(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Menubar.Trigger
      {...forwarded(rest)}
      className={classNames(props(styles.trigger, xstyle).className, className)}
    >
      {children}
    </Primitive.Menubar.Trigger>
  );
}

export {
  MenuCheckboxItem as MenubarCheckboxItem,
  MenuContent as MenubarContent,
  MenuGroup as MenubarGroup,
  MenuItem as MenubarItem,
  MenuLabel as MenubarLabel,
  MenuRadioGroup as MenubarRadioGroup,
  MenuRadioItem as MenubarRadioItem,
  MenuSeparator as MenubarSeparator,
  MenuShortcut as MenubarShortcut,
  MenuSub as MenubarSub,
  MenuSubTrigger as MenubarSubTrigger,
};

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
