"use client";
// @flow
//
// Select: `@uniflowed/ui`'s select-only combobox, drawn as a field and a list.
//
// `uf ui add select` wrote this file into the project, and it is the project's
// from then on. `uf ui diff select` shows how it has moved away from the
// registry in the uf you are running.
//
// # Use a native `<select>` when one will do
//
// `@uniflowed/ui/select` says it first and means it: the platform's control is
// announced correctly by software nobody here has tested against, is a wheel
// on a phone, autofills and validates. This component is for the list a native
// select cannot draw — an option with an icon, a second line or a check.
//
// # What this file owns, and what it does not
//
// The look: the trigger drawn as a field with a chevron, the list as a panel
// under it, the highlight on the option the keyboard is on, the check beside
// the chosen one. The behaviour is imported: focus that never leaves the
// trigger, `aria-activedescendant` moving over the options, typeahead, `Home`
// and `End`, an `Escape` that changes nothing, a `Tab` that takes the option
// under the cursor, the hidden input a form submits, and the list kept against
// its trigger as the page scrolls.
//
// Both states the list draws are attributes the part writes. The option under
// the cursor says `data-active="true"`, and no pseudo-class can find it,
// because focus stays on the trigger — so the highlight is the only focus
// indicator a keyboard user has inside the list, and it is drawn from that
// attribute. The chosen option says `aria-selected="true"`, and shows its check
// through a custom property it sets for the icon inside it.
//
// # What to keep true when you change it
//
// * **Label it.** `SelectLabel` names the trigger through `aria-labelledby`,
//   and a select with no label is a combobox with no name. A design with no
//   visible label passes `aria-label` to `SelectTrigger`.
// * **Draw the cursor.** However you restyle an option, `data-active` needs a
//   difference a sighted keyboard user can see. Here the background and the
//   text both change, `accent` on `accentSoft`, a pair
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both shipped
//   themes.
// * **The chosen option is marked twice.** A check and a heavier weight, so the
//   choice is not carried by colour alone.
// * **The list is as wide as its trigger and no taller than its room.** The
//   part writes `--uf-anchor-trigger-width` and `--uf-anchor-available-height`
//   on the list, and those are what size it.
// * **The icons are decoration.** The chevron and the check are `aria-hidden`:
//   the trigger's text and `aria-selected` already say what they draw.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { Align, LogicalSide } from "@uniflowed/ui/select";
import * as Primitive from "@uniflowed/ui/select";

/**
 * Every prop a caller passes that this file does not name, on its way to the
 * part underneath. See `forwarded` for the one thing Flow cannot say about it.
 */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "grid",
    gap: ufTokens.space2,
    justifyItems: "start",
    fontFamily: ufTokens.fontSans,
    color: ufTokens.ink,
  },
  label: {
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
  },
  trigger: {
    // Read by the chevron inside, which cannot see this element's state.
    "--uf-select-chevron-turn": { default: "0deg", ":is([aria-expanded=true])": "180deg" },
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: ufTokens.space2,
    boxSizing: "border-box",
    minWidth: "12rem",
    minHeight: "36px",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingTight,
    textAlign: "start",
    color: ufTokens.ink,
    backgroundColor: {
      default: ufTokens.surface,
      ":hover": ufTokens.surfaceHover,
      ":disabled": ufTokens.surface,
    },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-expanded=true])": ufTokens.accent },
    borderRadius: ufTokens.radiusSm,
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    opacity: { default: 1, ":disabled": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
  },
  chevron: {
    flexShrink: 0,
    color: ufTokens.muted,
    transform: "rotate(var(--uf-select-chevron-turn))",
    transitionProperty: "transform",
    transitionDuration: {
      default: ufTokens.durationFast,
      "@media (prefers-reduced-motion: reduce)": "0s",
    },
    transitionTimingFunction: ufTokens.easing,
  },
  list: {
    zIndex: 50,
    boxSizing: "border-box",
    minWidth: "var(--uf-anchor-trigger-width)",
    maxHeight: "min(20rem, var(--uf-anchor-available-height, 20rem))",
    overflowY: "auto",
    margin: 0,
    padding: ufTokens.space1,
    backgroundColor: ufTokens.surface,
    color: ufTokens.ink,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    boxShadow: ufTokens.shadowPanel,
  },
  option: {
    // Read by the check inside, which cannot see this element's state.
    "--uf-select-check": { default: "0", ":is([aria-selected=true])": "1" },
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: ufTokens.space3,
    minHeight: "32px",
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    borderRadius: ufTokens.radiusSm,
    lineHeight: ufTokens.leadingTight,
    cursor: "pointer",
    userSelect: "none",
    color: { default: ufTokens.ink, ":is([data-active=true])": ufTokens.accent },
    backgroundColor: { default: "transparent", ":is([data-active=true])": ufTokens.accentSoft },
    fontWeight: {
      default: ufTokens.weightRegular,
      ":is([aria-selected=true])": ufTokens.weightMedium,
    },
  },
  // Merged after `option`, so it replaces the highlight a disabled option
  // would otherwise still be drawn with.
  optionDisabled: {
    color: ufTokens.muted,
    backgroundColor: "transparent",
    cursor: "not-allowed",
  },
  optionText: {
    minWidth: 0,
  },
  check: {
    flexShrink: 0,
    color: ufTokens.accent,
    opacity: "var(--uf-select-check)",
  },
  group: {
    display: "grid",
    paddingBlock: ufTokens.space1,
  },
  groupLabel: {
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

/**
 * The select. Uncontrolled from `defaultValue` unless `value` is given, and
 * submitted with a form under `name`.
 *
 *     <Select name="country">
 *       <SelectLabel>Country</SelectLabel>
 *       <SelectTrigger>
 *         <SelectValue placeholder="Choose a country" />
 *       </SelectTrigger>
 *       <SelectList>
 *         <SelectOption value="fr">France</SelectOption>
 *         <SelectOption value="jp">Japan</SelectOption>
 *       </SelectList>
 *     </Select>
 */
export component Select(
  children: React.Node,
  value?: string | null,
  defaultValue?: string | null = null,
  onValueChange?: (value: string | null) => void,
  open?: boolean,
  defaultOpen?: boolean = false,
  onOpenChange?: (open: boolean) => void,
  name?: string,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SelectRoot
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
      defaultOpen={defaultOpen}
      defaultValue={defaultValue}
      disabled={disabled}
      name={name}
      onOpenChange={onOpenChange}
      onValueChange={onValueChange}
      open={open}
      value={value}
    >
      {children}
    </Primitive.SelectRoot>
  );
}

/** The field's name, which is also the trigger's accessible name. */
export component SelectLabel(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SelectLabel
      {...forwarded(rest)}
      className={classNames(props(styles.label, xstyle).className, className)}
    >
      {children}
    </Primitive.SelectLabel>
  );
}

/** The button that opens the list, drawn as a field with a chevron. */
export component SelectTrigger(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SelectTrigger
      {...forwarded(rest)}
      className={classNames(props(styles.trigger, xstyle).className, className)}
    >
      {children}
      <ChevronIcon />
    </Primitive.SelectTrigger>
  );
}

/** What the trigger says: the chosen option, or the placeholder. */
export component SelectValue(
  placeholder?: React.Node,
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SelectValue
      {...forwarded(rest)}
      className={classNames(props(xstyle).className, className)}
      placeholder={placeholder}
    >
      {children}
    </Primitive.SelectValue>
  );
}

/** The list of options, under the trigger and as wide as it. */
export component SelectList(
  children: renders* (SelectOption | SelectGroup | SelectSeparator),
  align?: Align = "start",
  alignOffset?: number = 0,
  avoidCollisions?: boolean = true,
  collisionPadding?: number = 8,
  side?: LogicalSide = "bottom",
  sideOffset?: number = 4,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.SelectList
      {...forwarded(rest)}
      align={align}
      alignOffset={alignOffset}
      avoidCollisions={avoidCollisions}
      className={classNames(props(styles.list, xstyle).className, className)}
      collisionPadding={collisionPadding}
      side={side}
      sideOffset={sideOffset}
    >
      {children}
    </Primitive.SelectList>
  );
}

/** One option, with the check that shows when it is the chosen one. */
export component SelectOption(
  value: string,
  children: React.Node,
  label?: string,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.SelectOption {
  const styled = props(styles.option, disabled && styles.optionDisabled, xstyle);
  return (
    <Primitive.SelectOption
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      disabled={disabled}
      label={label}
      value={value}
    >
      <span {...props(styles.optionText)}>{children}</span>
      <CheckIcon />
    </Primitive.SelectOption>
  );
}

/** A named group of options. */
export component SelectGroup(
  children: renders* (SelectOption | SelectGroupLabel),
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.SelectGroup {
  return (
    <Primitive.SelectGroup
      {...forwarded(rest)}
      className={classNames(props(styles.group, xstyle).className, className)}
    >
      {children}
    </Primitive.SelectGroup>
  );
}

/** The heading that names a group, which the arrow keys pass over. */
export component SelectGroupLabel(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.SelectGroupLabel {
  return (
    <Primitive.SelectGroupLabel
      {...forwarded(rest)}
      className={classNames(props(styles.groupLabel, xstyle).className, className)}
    >
      {children}
    </Primitive.SelectGroupLabel>
  );
}

/** A rule between groups: decoration, and out of the accessibility tree. */
export component SelectSeparator(
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.SelectSeparator {
  return (
    <Primitive.SelectSeparator
      {...forwarded(rest)}
      className={classNames(props(styles.separator, xstyle).className, className)}
    />
  );
}

/** The chevron, which turns while the list is open. */
component ChevronIcon() {
  return (
    <svg
      {...props(styles.chevron)}
      aria-hidden="true"
      fill="none"
      focusable="false"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

/** The check beside the chosen option. */
component CheckIcon() {
  return (
    <svg
      {...props(styles.check)}
      aria-hidden="true"
      fill="none"
      focusable="false"
      height="16"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
      width="16"
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}

/**
 * A caller's props on their way into a part, rather than onto an element.
 *
 * Flow checks a spread into a component against that component's own
 * `...rest`, `key` included, and the indexer in `Rest` answers `mixed` where
 * the part says `empty`. `@uniflowed/ui` meets the same hole between its own
 * parts, and papers over it the same way in one named place,
 * `internal/merge-props.js`'s `forwarded`: nothing that was ever checked is
 * lost, because every element those parts render has `any`-typed props in uf's
 * library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. See `button.js`. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
