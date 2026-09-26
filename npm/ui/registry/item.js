// @flow
//
// Item: one row of media, a title, a description and actions, alone or in a
// list of them.
//
// `uf ui add item` wrote this file into the project, and it is the project's
// from then on. `uf ui diff item` shows how it has moved away from the
// registry in the uf you are running.
//
// # Why a component here, when `@uniflowed/ui` has none
//
// Every part of an item is an element with a class, and when the rows are a
// list the list is `<ul>` and `<li>`. Nothing in it is a role, a state or a
// key, so `@uniflowed/ui` declines an `Item` (`crates/uf_lib/src/ui.rs` records
// it). What an application wants is one row its settings pages, member lists
// and notification panels agree on, and that is written here.
//
// # What to keep true when you change it
//
// * **Rows that belong together are a list.** `Item.Group` is a `<ul>` and
//   `<Item.Root as="li">` its rows, so a reader hears "list, 3 items" before
//   the first one. Name the group with `aria-label` when the page has more
//   than one.
// * **A row the arrow keys move through is not an item.** Rows a reader
//   selects, or moves between with the arrow keys, are a `ListBox`, a
//   `GridList` or a `Menu`, each of which has the keyboard contract that makes
//   it one. An item's controls are ordinary tab stops.
// * **An item that goes somewhere has one link, on its title.** Wrapping the
//   whole row in `<a>` makes a screen reader read every word in it as the
//   link's name, and it cannot hold the buttons in `Item.Actions`. Put the link
//   in `Item.Title`.
// * **The media is decoration unless you say otherwise.** An avatar beside the
//   person's name takes `alt=""`, and an icon takes `aria-hidden`, so the name
//   is not read twice. `Item.Media` does not hide its content itself, because
//   an image that carries meaning belongs there too.
// * **Actions come last in the DOM,** after what they act on, so a keyboard
//   reaches them after reading it. An icon-only action needs an `aria-label`.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both shipped
//   themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

/** Whether a row draws its own border, or sits plain in a list that does. */
export type ItemVariant = "outline" | "plain";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * element. `key` is `empty` because React takes it off before a component is
 * called.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  group: {
    display: "grid",
    gap: ufTokens.space2,
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  item: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space4,
    boxSizing: "border-box",
    paddingBlock: ufTokens.space3,
    paddingInline: ufTokens.space4,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    color: ufTokens.ink,
    borderWidth: "1px",
    borderStyle: "solid",
    borderRadius: ufTokens.radiusMd,
  },
  outline: {
    backgroundColor: ufTokens.surface,
    borderColor: ufTokens.border,
  },
  plain: {
    backgroundColor: "transparent",
    borderColor: "transparent",
  },
  media: {
    display: "inline-flex",
    flexShrink: 0,
    alignItems: "center",
    justifyContent: "center",
    color: ufTokens.muted,
  },
  content: {
    display: "grid",
    flexGrow: 1,
    gap: "2px",
    minWidth: 0,
  },
  title: {
    margin: 0,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
  },
  description: {
    margin: 0,
    color: ufTokens.muted,
  },
  actions: {
    display: "flex",
    flexShrink: 0,
    alignItems: "center",
    gap: ufTokens.space2,
  },
});

/** A list of items, announced as one. */
component ItemGroup(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <ul {...rest} className={classNames(props(styles.group, xstyle).className, className)}>
      {children}
    </ul>
  );
}

/** One row: a `<div>`, or with `as="li"` a row of an `Item.Group`. */
component ItemRoot(
  children: React.Node,
  as?: "div" | "li" = "div",
  variant?: ItemVariant = "outline",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const shared = {
    ...rest,
    className: classNames(
      props(
        styles.item,
        match (variant) {
          "outline" => styles.outline,
          "plain" => styles.plain,
        },
        xstyle,
      ).className,
      className,
    ),
  };
  return as === "li" ? <li {...shared}>{children}</li> : <div {...shared}>{children}</div>;
}

/** An icon, an avatar or a thumbnail at the start of the row. */
component ItemMedia(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.media, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** The title and the description, which take the room the row has. */
component ItemContent(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.content, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** What the row is. Not a heading: a row of a list is not a section. */
component ItemTitle(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <p {...rest} className={classNames(props(styles.title, xstyle).className, className)}>
      {children}
    </p>
  );
}

/** A line under the title, in the quieter colour. */
component ItemDescription(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <p {...rest} className={classNames(props(styles.description, xstyle).className, className)}>
      {children}
    </p>
  );
}

/** The buttons or links at the end of the row, after what they act on. */
component ItemActions(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...rest} className={classNames(props(styles.actions, xstyle).className, className)}>
      {children}
    </div>
  );
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}

/**
 * The parts, under the names `import * as Item from "./item.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Item.Root>`
 * and `<Item.Title>`. Each is declared under its full name, so React DevTools
 * and an error say `ItemRoot` rather than `Root`.
 */
export {
  ItemGroup as Group,
  ItemRoot as Root,
  ItemMedia as Media,
  ItemContent as Content,
  ItemTitle as Title,
  ItemDescription as Description,
  ItemActions as Actions,
};
