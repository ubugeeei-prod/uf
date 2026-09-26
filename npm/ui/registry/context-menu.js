"use client";
// @flow
//
// Context menu: the menu a right-click or a long press opens over an area of
// the page, where the reader is.
//
// `uf ui add context-menu` wrote this file into the project, and it is the
// project's from then on. `uf ui diff context-menu` shows how it has moved
// away from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// Only a focus ring for the area: what the area looks like is the page's. The
// panel and its rows are `menu.js`'s parts under context-menu names, so a
// context menu and a menu look alike and change together.
// `@uniflowed/ui`'s `ContextMenu` owns opening at the pointer, a long press on a
// touch screen, opening from the keyboard on a focused area, and the menu
// pattern after that.
//
// # What to keep true when you change it
//
// * **Name the panel.** No trigger names it, so `ContextMenu.Content` needs an
//   `aria-label`.
// * **Keep the area focusable.** Its menu opens from the keyboard only when the
//   area can take focus.
// * **Offer the actions elsewhere too.** A context menu is a shortcut; whatever
//   is in it should also be reachable some other way.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { ContextMenu } from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  area: {
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
});

/** The context menu, open or closed. Uncontrolled unless `open` is given. */
component ContextMenuRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <ContextMenu.Root defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
      {children}
    </ContextMenu.Root>
  );
}

/** The area whose menu this is. */
component ContextMenuTrigger(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <ContextMenu.Trigger
      {...forwarded(rest)}
      className={classNames(props(styles.area, xstyle).className, className)}
    >
      {children}
    </ContextMenu.Trigger>
  );
}

/**
 * The menu parts, which are `menu.js`'s own: a context menu is a menu opened another
 * way, so `<ContextMenu.Item>` is the styled `Menu.Item` and a change to it there
 * reaches both.
 */
export {
  CheckboxItem,
  Content,
  Group,
  Item,
  Label,
  RadioGroup,
  RadioItem,
  Separator,
  Shortcut,
  Sub,
  SubTrigger,
} from "./menu.js";

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

/**
 * The parts, under the names `import * as ContextMenu from "./context-menu.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<ContextMenu.Root>`
 * and `<ContextMenu.Trigger>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `ContextMenuRoot` rather than `Root`.
 */
export { ContextMenuRoot as Root, ContextMenuTrigger as Trigger };
