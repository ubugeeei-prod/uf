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
// * **Keep `Combobox.Status` in.** It is visually hidden, and it is how a reader
//   hears "3 results available".
// * **Draw the cursor.** However an option is restyled, `data-active` needs a
//   difference a sighted keyboard user can see, since focus never leaves the
//   field: `accent` on `accentSoft` here, a pair
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.
// * **Label it.** `Combobox.Label` names the field; a placeholder is not a
//   label.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { Align, LogicalSide } from "@uniflowed/ui";
import { Combobox } from "@uniflowed/ui";

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
    borderRadius: ufTokens.radiusMd,
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
    // Enter: it fades in while travelling 4px out of its trigger, from the
    // side `data-side` says it opened on, so the eye is led from the button
    // to what it opened. `durationBase` on the decelerating curve: most of
    // the distance is covered at once, so it is legible before it has
    // settled. Under reduced motion it only fades.
    "--uf-enter-x": {
      default: "0px",
      ":is([data-side=left])": "4px",
      ":is([data-side=right])": "-4px",
    },
    "--uf-enter-y": {
      default: "0px",
      ":is([data-side=top])": "4px",
      ":is([data-side=bottom])": "-4px",
    },
    opacity: { default: 1, "@starting-style": 0 },
    transform: {
      default: "none",
      "@starting-style": "translate(var(--uf-enter-x), var(--uf-enter-y))",
    },
    transitionProperty: {
      default: "opacity, transform",
      "@media (prefers-reduced-motion: reduce)": "opacity",
    },
    transitionDuration: ufTokens.durationBase,
    transitionTimingFunction: ufTokens.easingEnter,
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
component ComboboxRoot(
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
    <Combobox.Root
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
    </Combobox.Root>
  );
}

/** The field's name. */
component ComboboxLabel(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Combobox.Label
      {...forwarded(rest)}
      className={classNames(props(styles.label, xstyle).className, className)}
    >
      {children}
    </Combobox.Label>
  );
}

/** The text field. */
component ComboboxInput(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Combobox.Input
      {...forwarded(rest)}
      className={classNames(props(styles.input, xstyle).className, className)}
    />
  );
}

/** The list of options, under the field and at least as wide as it. */
component ComboboxList(
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
    <Combobox.List
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
    </Combobox.List>
  );
}

/** One option, with the check that shows when it is the chosen one. */
component ComboboxOption(
  value: string,
  children: React.Node,
  label?: string,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Combobox.Option {
  const styled = props(styles.option, disabled && styles.optionDisabled, xstyle);
  return (
    <Combobox.Option
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
    </Combobox.Option>
  );
}

/** A named group of options. */
component ComboboxGroup(
  children: renders* (ComboboxOption | ComboboxGroupLabel),
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Combobox.Group {
  return (
    <Combobox.Group
      {...forwarded(rest)}
      className={classNames(props(styles.group, xstyle).className, className)}
    >
      {children}
    </Combobox.Group>
  );
}

/** The heading that names a group, which the arrow keys pass over. */
component ComboboxGroupLabel(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Combobox.GroupLabel {
  return (
    <Combobox.GroupLabel
      {...forwarded(rest)}
      className={classNames(props(styles.groupLabel, xstyle).className, className)}
    >
      {children}
    </Combobox.GroupLabel>
  );
}

/** What shows when nothing matches. */
component ComboboxEmpty(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Combobox.Empty
      {...forwarded(rest)}
      className={classNames(props(styles.empty, xstyle).className, className)}
    >
      {children}
    </Combobox.Empty>
  );
}

/** The result count a reader hears, out of sight. Words of your own go inside. */
component ComboboxStatus(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Combobox.Status
      {...forwarded(rest)}
      className={classNames(props(styles.hidden, xstyle).className, className)}
    >
      {children}
    </Combobox.Status>
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

/**
 * The parts, under the names `import * as Combobox from "./combobox.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Combobox.Root>`
 * and `<Combobox.Label>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `ComboboxRoot` rather than `Root`.
 */
export {
  ComboboxRoot as Root,
  ComboboxLabel as Label,
  ComboboxInput as Input,
  ComboboxList as List,
  ComboboxOption as Option,
  ComboboxGroup as Group,
  ComboboxGroupLabel as GroupLabel,
  ComboboxEmpty as Empty,
  ComboboxStatus as Status,
};
