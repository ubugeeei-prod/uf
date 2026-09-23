"use client";
// @flow
//
// Menu: a list of actions and options that opens from a button and is moved
// through from the keyboard.
//
// `uf ui add menu` wrote this file into the project, and it is the project's
// from then on. `uf ui diff menu` shows how it has moved away from the registry
// in the uf you are running.
//
// # What this file owns, and what it does not
//
// The panel, its rows, the check beside a checked row and the dot beside a
// chosen one, shortcut text, the labels and lines that group rows, and the
// arrow on a row that opens a submenu. `context-menu.js` and `menubar.js` dress
// their menus with these same parts. `@uniflowed/ui`'s `Menu` owns the pattern:
// `role="menu"` named by its trigger, focus on the rows and between them from
// the keyboard, submenus, `aria-checked` on checkbox and radio rows, and
// `aria-expanded` on a row whose submenu is open. The focused row is drawn from
// `:focus-visible` and `:hover`, an open submenu's row from `aria-expanded`, a
// disabled row from `aria-disabled`, and a mark from `aria-checked`.
//
// # What to keep true when you change it
//
// * **Draw the focused row.** Focus sits on the rows, and the highlight is the
//   sign of it: `accent` on `accentSoft`, a pair
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes. The
//   transparent outline is for forced colours, where backgrounds are dropped.
// * **Draw marks from what is announced.** The check and the dot follow
//   `aria-checked`, so what a sighted reader sees is what a screen reader
//   hears.
// * **Keep shortcuts out of the name.** `MenuShortcut` is `aria-hidden`; tell
//   assistive technology the keys with `aria-keyshortcuts` on the row.
// * **Name a menu nothing names.** A context menu's panel needs an
//   `aria-label`.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { Align, LogicalSide, MenuSelect } from "@uniflowed/ui";
import * as Primitive from "@uniflowed/ui";

import type { ButtonSize, ButtonTone } from "./button.js";
import { Button } from "./button.js";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

/** A caller's own element in place of the one a part renders. */
type RenderProp = (props: Rest) => React.Node;

const styles = stylex.create({
  content: {
    zIndex: 50,
    boxSizing: "border-box",
    minWidth: "12rem",
    maxWidth: "calc(100vw - 16px)",
    maxHeight: "var(--uf-anchor-available-height, none)",
    overflowY: "auto",
    margin: 0,
    padding: ufTokens.space1,
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingTight,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  item: {
    // Read by the check or the dot inside, which cannot see this row's state.
    "--uf-menu-mark": { default: "0", ":is([aria-checked=true])": "1" },
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space2,
    boxSizing: "border-box",
    width: "100%",
    minHeight: "32px",
    margin: 0,
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingTight,
    textAlign: "start",
    userSelect: "none",
    cursor: { default: "pointer", ":is([aria-disabled=true])": "not-allowed" },
    color: {
      default: ufTokens.ink,
      ":hover": ufTokens.accent,
      ":focus-visible": ufTokens.accent,
      ":is([aria-expanded=true])": ufTokens.accent,
      ":is([aria-disabled=true])": ufTokens.muted,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.accentSoft,
      ":focus-visible": ufTokens.accentSoft,
      ":is([aria-expanded=true])": ufTokens.accentSoft,
      ":is([aria-disabled=true])": "transparent",
    },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: "transparent",
    outlineOffset: "-2px",
  },
  mark: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    flexShrink: 0,
    width: "16px",
    height: "16px",
    opacity: "var(--uf-menu-mark, 0)",
  },
  shortcut: {
    marginInlineStart: "auto",
    paddingInlineStart: ufTokens.space4,
    fontSize: ufTokens.textXs,
    letterSpacing: "0.05em",
  },
  chevron: {
    flexShrink: 0,
    marginInlineStart: "auto",
  },
  group: {
    display: "grid",
  },
  label: {
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    fontSize: ufTokens.textXs,
    fontWeight: ufTokens.weightMedium,
    color: ufTokens.muted,
  },
  separator: {
    height: "1px",
    marginBlock: ufTokens.space1,
    backgroundColor: ufTokens.border,
  },
});

/** The menu, open or closed. Uncontrolled unless `open` is given. */
export component Menu(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <Primitive.Menu.Root defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
      {children}
    </Primitive.Menu.Root>
  );
}

/**
 * The button that opens the menu, names it, and gets focus back. It is a
 * `Button` unless `render` says otherwise.
 */
export component MenuTrigger(
  children: React.Node,
  tone?: ButtonTone = "neutral",
  size?: ButtonSize = "md",
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Menu.Trigger
      {...forwarded(rest)}
      render={
        render ??
        ((trigger) => (
          <Button
            {...forwarded(trigger)}
            className={className}
            size={size}
            tone={tone}
            xstyle={xstyle}
          />
        ))
      }
    >
      {children}
    </Primitive.Menu.Trigger>
  );
}

/**
 * The panel of rows: under its trigger unless it does not fit, and to the
 * inline end of the row that opens it inside a `MenuSub`.
 */
export component MenuContent(
  children: renders* (
    | MenuItem
    | MenuCheckboxItem
    | MenuRadioGroup
    | MenuSeparator
    | MenuGroup
    | MenuSub
  ),
  align?: Align = "start",
  side?: LogicalSide,
  sideOffset?: number = 4,
  collisionPadding?: number = 8,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Menu.Body
      {...forwarded(rest)}
      align={align}
      className={classNames(props(styles.content, xstyle).className, className)}
      collisionPadding={collisionPadding}
      side={side}
      sideOffset={sideOffset}
    >
      {children}
    </Primitive.Menu.Body>
  );
}

/** A row that does something, and closes the menu unless `closeOnSelect` is false. */
export component MenuItem(
  children: React.Node,
  disabled?: boolean = false,
  closeOnSelect?: boolean = true,
  onSelect?: (event: MenuSelect) => mixed,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Menu.Item {
  return (
    <Primitive.Menu.Item
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
      closeOnSelect={closeOnSelect}
      disabled={disabled}
      onSelect={onSelect}
    >
      {children}
    </Primitive.Menu.Item>
  );
}

/** A row that turns something on or off, with a check while it is on. */
export component MenuCheckboxItem(
  children: React.Node,
  checked?: boolean,
  defaultChecked?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  closeOnSelect?: boolean = false,
  onSelect?: (event: MenuSelect) => mixed,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Menu.CheckboxItem {
  return (
    <Primitive.Menu.CheckboxItem
      {...forwarded(rest)}
      checked={checked}
      className={classNames(props(styles.item, xstyle).className, className)}
      closeOnSelect={closeOnSelect}
      defaultChecked={defaultChecked}
      disabled={disabled}
      onCheckedChange={onCheckedChange}
      onSelect={onSelect}
    >
      <span {...props(styles.mark)}>
        <svg
          aria-hidden="true"
          fill="none"
          focusable="false"
          height="14"
          stroke="currentColor"
          strokeLinecap="round"
          strokeLinejoin="round"
          strokeWidth="2.5"
          viewBox="0 0 24 24"
          width="14"
        >
          <path d="M20 6 9 17l-5-5" />
        </svg>
      </span>
      {children}
    </Primitive.Menu.CheckboxItem>
  );
}

/** Rows of which one is chosen. A `MenuLabel` inside names the group. */
export component MenuRadioGroup(
  children: renders* (MenuRadioItem | MenuLabel | MenuSeparator),
  value?: string | null,
  defaultValue?: string | null = null,
  onValueChange?: (value: string) => void,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Menu.RadioGroup {
  return (
    <Primitive.Menu.RadioGroup
      {...forwarded(rest)}
      className={classNames(props(styles.group, xstyle).className, className)}
      defaultValue={defaultValue}
      onValueChange={onValueChange}
      value={value}
    >
      {children}
    </Primitive.Menu.RadioGroup>
  );
}

/** One choice in a `MenuRadioGroup`, with a dot while it is the chosen one. */
export component MenuRadioItem(
  children: React.Node,
  value: string,
  disabled?: boolean = false,
  closeOnSelect?: boolean = false,
  onSelect?: (event: MenuSelect) => mixed,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Menu.RadioItem {
  return (
    <Primitive.Menu.RadioItem
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
      closeOnSelect={closeOnSelect}
      disabled={disabled}
      onSelect={onSelect}
      value={value}
    >
      <span {...props(styles.mark)}>
        <svg aria-hidden="true" focusable="false" height="8" viewBox="0 0 8 8" width="8">
          <circle cx="4" cy="4" fill="currentColor" r="4" />
        </svg>
      </span>
      {children}
    </Primitive.Menu.RadioItem>
  );
}

/** Rows kept together. A `MenuLabel` inside names the group. */
export component MenuGroup(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Menu.Group {
  return (
    <Primitive.Menu.Group
      {...forwarded(rest)}
      className={classNames(props(styles.group, xstyle).className, className)}
    >
      {children}
    </Primitive.Menu.Group>
  );
}

/** The name of the group it is in, which the keyboard passes over. */
export component MenuLabel(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Menu.Label {
  return (
    <Primitive.Menu.Label
      {...forwarded(rest)}
      className={classNames(props(styles.label, xstyle).className, className)}
    >
      {children}
    </Primitive.Menu.Label>
  );
}

/** A line between groups of rows. */
export component MenuSeparator(
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Menu.Separator {
  return (
    <Primitive.Menu.Separator
      {...forwarded(rest)}
      className={classNames(props(styles.separator, xstyle).className, className)}
    />
  );
}

/** The keys that do the same as a row, at its end and out of its name. */
export component MenuShortcut(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <span
      {...rest}
      aria-hidden="true"
      className={classNames(props(styles.shortcut, xstyle).className, className)}
    >
      {children}
    </span>
  );
}

/** A submenu: a `MenuSubTrigger` and the `MenuContent` it opens. */
export component MenuSub(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) renders Primitive.Menu.Sub {
  return (
    <Primitive.Menu.Sub defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open}>
      {children}
    </Primitive.Menu.Sub>
  );
}

/** The row that opens a submenu, with an arrow toward it. */
export component MenuSubTrigger(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Menu.SubTrigger
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
    >
      {children}
      <svg
        {...props(styles.chevron)}
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="14"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="14"
      >
        <path d="m9 18 6-6-6-6" />
      </svg>
    </Primitive.Menu.SubTrigger>
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
