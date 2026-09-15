"use client";
// @flow
//
// Combobox: a text field with a list of suggestions, navigated without leaving
// the field, and filtered by the page that owns the options.
//
// `uf ui add combobox` wrote this file into the project, and it is the
// project's from then on. `uf ui diff combobox` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The field, the list as a panel under it, the highlight on the option the
// keyboard is on and the check beside the chosen one. The pattern is imported
// from `@uniflowed/ui`'s `Combobox`: focus that stays in the field while
// `aria-activedescendant` moves over the options, `Enter` taking the
// highlighted option and leaving the form alone when there is none, `Escape`
// closing and then clearing, and a status that counts the results for a reader
// who cannot see the list. The highlight is drawn from `data-active`, and the
// check from `aria-selected`.
//
// # What to keep true when you change it
//
// * **The page filters.** The component shows the options it is given; which
//   ones match the text is the page's decision, because a list can be remote,
//   fuzzy or grouped.
// * **Keep `ComboboxStatus` in.** It is visually hidden, and it is how a reader
//   hears "3 results available".
// * **Draw the cursor.** However an option is restyled, `data-active` needs a
//   difference a sighted keyboard user can see, since focus never leaves the
//   field: `accent` on `accentSoft` here, a pair
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.
// * **Label it.** `ComboboxLabel` names the field; a placeholder is not a
//   label.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { Align, LogicalSide } from "@uniflowed/ui";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
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
  input: {
    boxSizing: "border-box",
    width: "16rem",
    maxWidth: "100%",
    minHeight: "36px",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([aria-expanded=true])": ufTokens.accent },
    borderRadius: ufTokens.radiusSm,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
    "::placeholder": {
      color: ufTokens.muted,
      opacity: 1,
    },
  },
  list: {
    zIndex: 50,
    boxSizing: "border-box",
    minWidth: "var(--uf-anchor-trigger-width)",
    maxHeight: "min(20rem, var(--uf-anchor-available-height, 20rem))",
    overflowY: "auto",
    margin: 0,
    padding: ufTokens.space1,
    listStyle: "none",
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
    "--uf-combobox-check": { default: "0", ":is([aria-selected=true])": "1" },
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
  optionDisabled: {
    color: ufTokens.muted,
    backgroundColor: "transparent",
    cursor: "not-allowed",
  },
  check: {
    flexShrink: 0,
    color: ufTokens.accent,
    opacity: "var(--uf-combobox-check)",
  },
  group: {
    display: "grid",
    margin: 0,
    padding: 0,
    paddingBlock: ufTokens.space1,
    listStyle: "none",
  },
  groupLabel: {
    paddingBlock: ufTokens.space1,
    paddingInline: ufTokens.space2,
    fontSize: ufTokens.textXs,
    fontWeight: ufTokens.weightMedium,
    color: ufTokens.muted,
  },
  empty: {
    margin: 0,
    paddingBlock: ufTokens.space2,
    fontSize: ufTokens.textSm,
    color: ufTokens.muted,
  },
  // Out of sight and still read: the rule every screen-reader-only class uses.
  hidden: {
    position: "absolute",
    width: "1px",
    height: "1px",
    margin: "-1px",
    padding: 0,
    overflow: "hidden",
    clip: "rect(0, 0, 0, 0)",
    whiteSpace: "nowrap",
    borderWidth: 0,
  },
});

/**
 * The combobox. `inputValue` and `onInputValueChange` are what the page filters
 * its options by; `value` is the option taken.
 */
export component Combobox(
  children: React.Node,
  value?: string | null,
  defaultValue?: string | null = null,
  onValueChange?: (value: string | null) => void,
  inputValue?: string,
  defaultInputValue?: string = "",
  onInputValueChange?: (text: string) => void,
  open?: boolean,
  defaultOpen?: boolean = false,
  onOpenChange?: (open: boolean) => void,
  name?: string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.ComboboxRoot
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
      defaultInputValue={defaultInputValue}
      defaultOpen={defaultOpen}
      defaultValue={defaultValue}
      inputValue={inputValue}
      name={name}
      onInputValueChange={onInputValueChange}
      onOpenChange={onOpenChange}
      onValueChange={onValueChange}
      open={open}
      value={value}
    >
      {children}
    </Primitive.ComboboxRoot>
  );
}

/** The field's name. */
export component ComboboxLabel(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.ComboboxLabel
      {...forwarded(rest)}
      className={classNames(props(styles.label, xstyle).className, className)}
    >
      {children}
    </Primitive.ComboboxLabel>
  );
}

/** The text field. */
export component ComboboxInput(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.ComboboxInput
      {...forwarded(rest)}
      className={classNames(props(styles.input, xstyle).className, className)}
    />
  );
}

/** The list of options, under the field and at least as wide as it. */
export component ComboboxList(
  children: renders* (ComboboxOption | ComboboxGroup),
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
    <Primitive.ComboboxList
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
    </Primitive.ComboboxList>
  );
}

/** One option, with the check that shows when it is the chosen one. */
export component ComboboxOption(
  value: string,
  children: React.Node,
  label?: string,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.ComboboxOption {
  const styled = props(styles.option, disabled && styles.optionDisabled, xstyle);
  return (
    <Primitive.ComboboxOption
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      disabled={disabled}
      label={label}
      value={value}
    >
      <span>{children}</span>
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
    </Primitive.ComboboxOption>
  );
}

/** A named group of options. */
export component ComboboxGroup(
  children: renders* (ComboboxOption | ComboboxGroupLabel),
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.ComboboxGroup {
  return (
    <Primitive.ComboboxGroup
      {...forwarded(rest)}
      className={classNames(props(styles.group, xstyle).className, className)}
    >
      {children}
    </Primitive.ComboboxGroup>
  );
}

/** The heading that names a group, which the arrow keys pass over. */
export component ComboboxGroupLabel(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.ComboboxGroupLabel {
  return (
    <Primitive.ComboboxGroupLabel
      {...forwarded(rest)}
      className={classNames(props(styles.groupLabel, xstyle).className, className)}
    >
      {children}
    </Primitive.ComboboxGroupLabel>
  );
}

/** What shows when nothing matches. */
export component ComboboxEmpty(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.ComboboxEmpty
      {...forwarded(rest)}
      className={classNames(props(styles.empty, xstyle).className, className)}
    >
      {children}
    </Primitive.ComboboxEmpty>
  );
}

/** The result count a reader hears, out of sight. Words of your own go inside. */
export component ComboboxStatus(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.ComboboxStatus
      {...forwarded(rest)}
      className={classNames(props(styles.hidden, xstyle).className, className)}
    >
      {children}
    </Primitive.ComboboxStatus>
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
