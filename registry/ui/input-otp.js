"use client";
// @flow
//
// Input OTP: a one-time code typed into one field and drawn as a row of boxes,
// one character in each.
//
// `uf ui add input-otp` wrote this file into the project, and it is the
// project's from then on. `uf ui diff input-otp` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The boxes, the gap between groups, the separator, and the one field laid
// invisibly over them so that a click anywhere in the row lands in it.
// `@uniflowed/ui`'s `InputOtp` owns the field itself: one real `<input>` with
// `autocomplete="one-time-code"`, so a phone can offer the code from a
// message, the characters a `kind` allows, `onComplete` once every box is
// full, and boxes that are `aria-hidden` because the field already says
// everything. The box the next character goes into is drawn from
// `data-active`, which is only there while the field has focus, so it is the
// focus ring.
//
// # What to keep true when you change it
//
// * **One field.** However the boxes are restyled, typing, pasting and
//   autofill go into the single `<input>`; do not give each box its own.
// * **Keep the ring.** The field is invisible, so the active box's outline is
//   the only sign of focus a sighted keyboard user gets.
// * **Name it.** `label` is what a reader hears; the boxes say nothing.
// * **Text stays on measured pairs.** `ink` on `surface`, and `muted` on
//   `sunken` while disabled, which `crates/uf_stylex/src/tests/preset.rs`
//   holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import { createContext, useContext } from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { InputOtpKind } from "@uniflowed/ui";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

/** Whether the row is disabled, for the boxes, which cannot see the field. */
const DisabledContext = createContext<boolean>(false);

const styles = stylex.create({
  root: {
    position: "relative",
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space2,
    fontFamily: ufTokens.fontSans,
    color: ufTokens.ink,
  },
  // Over the whole row, so a click on any box focuses the field, and invisible,
  // because the boxes draw what it holds.
  input: {
    position: "absolute",
    insetBlockStart: 0,
    insetInlineStart: 0,
    zIndex: 1,
    boxSizing: "border-box",
    width: "100%",
    height: "100%",
    margin: 0,
    padding: 0,
    borderWidth: 0,
    opacity: 0,
    cursor: { default: "text", ":disabled": "not-allowed" },
  },
  group: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space1,
  },
  slot: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    boxSizing: "border-box",
    width: "40px",
    height: "44px",
    fontSize: "1.125rem",
    fontWeight: ufTokens.weightMedium,
    fontVariantNumeric: "tabular-nums",
    lineHeight: ufTokens.leadingTight,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: ufTokens.border, ":is([data-active=true])": ufTokens.accent },
    borderRadius: ufTokens.radiusSm,
    outlineWidth: { default: "0", ":is([data-active=true])": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "1px",
  },
  slotDisabled: {
    color: ufTokens.muted,
    backgroundColor: ufTokens.sunken,
  },
  separator: {
    width: "10px",
    height: "2px",
    borderRadius: "1px",
    backgroundColor: ufTokens.muted,
  },
});

/**
 * The row. `label` names the one field under the boxes; `length` is how many
 * characters the code has.
 */
export component InputOtp(
  children: React.Node,
  label: string,
  length: number,
  value?: string,
  defaultValue?: string = "",
  onValueChange?: (value: string) => void,
  onComplete?: (code: string) => void,
  kind?: InputOtpKind = "numeric",
  disabled?: boolean = false,
  name?: string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <DisabledContext.Provider value={disabled}>
      <div className={classNames(props(styles.root, xstyle).className, className)}>
        <Primitive.InputOtpRoot
          {...forwarded(rest)}
          className={props(styles.input).className}
          defaultValue={defaultValue}
          disabled={disabled}
          kind={kind}
          label={label}
          length={length}
          name={name}
          onComplete={onComplete}
          onValueChange={onValueChange}
          value={value}
        >
          {children}
        </Primitive.InputOtpRoot>
      </div>
    </DisabledContext.Provider>
  );
}

/** Boxes kept together, such as the two halves of a six-digit code. */
export component InputOtpGroup(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.InputOtpGroup
      {...forwarded(rest)}
      className={classNames(props(styles.group, xstyle).className, className)}
    >
      {children}
    </Primitive.InputOtpGroup>
  );
}

/** The box that shows the character at `index`. */
export component InputOtpSlot(
  index: number,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const disabled = useContext(DisabledContext);
  const styled = props(styles.slot, disabled && styles.slotDisabled, xstyle);
  return (
    <Primitive.InputOtpSlot
      {...forwarded(rest)}
      className={classNames(styled.className, className)}
      index={index}
    />
  );
}

/** A short dash between groups, silent to a reader. */
export component InputOtpSeparator(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  return (
    <Primitive.InputOtpSeparator
      {...forwarded(rest)}
      className={classNames(props(styles.separator, xstyle).className, className)}
    />
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
