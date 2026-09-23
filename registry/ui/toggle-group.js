"use client";
// @flow
//
// Toggle group: a row of toggles that behaves as one control, pressing any
// number of them or choosing one.
//
// `uf ui add toggle-group` wrote this file into the project, and it is the
// project's from then on. `uf ui diff toggle-group` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The look: segments joined into one bordered row or column, with the chosen
// ones filled. `@uniflowed/ui`'s `ToggleGroup` owns the two behaviours `type`
// selects between. `multiple` is a `group` of `aria-pressed` buttons; `single`
// is a radio group drawn as segments, with `aria-checked` and the arrow keys
// that check as they move. Both have one tab stop for the whole row. A segment
// is drawn chosen from either attribute, through `:is([aria-pressed=true])`
// and `:is([aria-checked=true])`, so both modes look the same.
//
// # What to keep true when you change it
//
// * **Name the group.** `aria-label` on `ToggleGroup` says what the segments
//   choose between.
// * **A group holds its own items and nothing else.** `ToggleGroup` takes
//   `renders* ToggleGroupItem`, and each item renders `@uniflowed/ui`'s, so a
//   stray button in the row is a Flow error.
// * **An icon segment needs `aria-label`,** and the icon `aria-hidden`.
// * **Chosen is more than a colour:** a filled background as well as `accent`
//   text, on `accentSoft`, a pair `crates/uf_stylex/src/tests/preset.rs` holds
//   to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import { createContext, useContext } from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { ToggleGroupType } from "@uniflowed/ui";
import * as Primitive from "@uniflowed/ui";

/** Which way the segments run. */
export type ToggleGroupOrientation = "horizontal" | "vertical";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

/** The orientation the caller gave `ToggleGroup`, for the items' dividers. */
const OrientationContext: React.Context<ToggleGroupOrientation> = createContext("horizontal");

const styles = stylex.create({
  root: {
    display: "inline-flex",
    flexDirection: { default: "row", ":is([aria-orientation=vertical])": "column" },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    backgroundColor: ufTokens.surface,
    padding: 0,
  },
  item: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: ufTokens.space2,
    boxSizing: "border-box",
    minWidth: "36px",
    minHeight: "36px",
    margin: 0,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    borderWidth: 0,
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: 0,
    color: {
      default: ufTokens.ink,
      ":is([aria-pressed=true])": ufTokens.accent,
      ":is([aria-checked=true])": ufTokens.accent,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.surfaceHover,
      ":is([aria-pressed=true])": ufTokens.accentSoft,
      ":is([aria-checked=true])": ufTokens.accentSoft,
    },
    cursor: {
      default: "pointer",
      ":disabled": "not-allowed",
      ":is([aria-disabled=true])": "not-allowed",
    },
    opacity: { default: 1, ":disabled": 0.55, ":is([aria-disabled=true])": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
  // A divider before every segment but the first, along the row's axis.
  horizontal: {
    borderInlineStartWidth: { default: "1px", ":first-child": 0 },
  },
  vertical: {
    borderTopWidth: { default: "1px", ":first-child": 0 },
  },
});

/**
 * The row. `type="multiple"` presses any number of items and `type="single"`
 * chooses one; `value` is a list either way.
 */
export component ToggleGroup(
  children: renders* ToggleGroupItem,
  type?: ToggleGroupType = "multiple",
  defaultValue?: $ReadOnlyArray<string>,
  value?: $ReadOnlyArray<string>,
  onValueChange?: (value: $ReadOnlyArray<string>) => void,
  orientation?: ToggleGroupOrientation = "horizontal",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <OrientationContext.Provider value={orientation}>
      <Primitive.ToggleGroup.Root
        {...forwarded(rest)}
        className={classNames(props(styles.root, xstyle).className, className)}
        defaultValue={defaultValue}
        onValueChange={onValueChange}
        orientation={orientation}
        type={type}
        value={value}
      >
        {children}
      </Primitive.ToggleGroup.Root>
    </OrientationContext.Provider>
  );
}

/** One segment. `disabled` keeps it in the row, announced as unavailable. */
export component ToggleGroupItem(
  value: string,
  children?: React.Node,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.ToggleGroup.Item {
  const orientation = useContext(OrientationContext);
  const styled = props(
    styles.item,
    orientation === "vertical" ? styles.vertical : styles.horizontal,
    xstyle,
  );
  return (
    <Primitive.ToggleGroup.Item
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      disabled={disabled}
      value={value}
    >
      {children}
    </Primitive.ToggleGroup.Item>
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
