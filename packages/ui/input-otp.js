// @flow
//
// A one-time-code field: six boxes that are one `<input>`.
//
// # What it gives that one plain `<input>` does not
//
// Less, if it is written the obvious way — and that is the whole point of this
// module's shape. `<input autocomplete="one-time-code" inputmode="numeric">` is
// already an excellent one-time-code field: the operating system offers the
// code it just received by SMS, the phone shows a number pad, a password
// manager fills it, and a screen reader announces one field with one name. Six
// `<input maxlength="1">`es lose every one of those, and they lose them
// silently: the autofill simply never appears.
//
// So the decision this component *is*: there is **one real `<input>`**, and the
// boxes are a picture of its value. The slots are `aria-hidden` `<div>`s with a
// character in them. Everything the platform does keeps working because the
// platform is still looking at the field it expects:
//
//   * `autocomplete="one-time-code"`, which is the single most valuable thing
//     about the component and the first thing a hand-written version loses.
//   * `inputmode` from `kind`, so a phone shows a number pad for a numeric code
//     and a keyboard for an alphanumeric one.
//   * **Pasting fills every box**, because pasting into one input is just an
//     `input` event carrying the whole string. There is nothing to distribute.
//   * **`Backspace`, the arrow keys, `Home` and `End` are the browser's.** In
//     one input they are text editing, which the browser already does
//     correctly. A six-input version has to reimplement all four — and the
//     selection, and what a paste into the fourth box means — and gets one of
//     them wrong.
//   * **One accessible name**, and one value a `<form>` submits under one
//     `name` — the joined code rather than six of them. No hidden input is
//     needed here, which is why `internal/form-value.js` is not imported: the
//     field the reader types into is the field the form reads.
//
// # Which box is lit
//
// The one the next character goes in: `data-active` follows the *length of the
// code*, not the caret. That is a deliberately small claim. Chasing
// `selectionStart` would need a `selectionchange` listener, would still have no
// answer for a reader who has selected all six characters, and would be
// describing something a reader filling in a code from a text message never
// does. Editing still works exactly as the browser does it — the arrow keys
// move the caret, `Backspace` deletes — and the highlight goes back a box when
// the code gets shorter, which is the part somebody looking at the screen sees.
//
// # What it costs
//
// The input has to be drawn over the slots by the styling layer — transparent
// text, a caret positioned by the design, or the whole input made invisible and
// the caret drawn by `data-active`. That is a real cost and it is the reason
// this component takes its DOM props on the *input* rather than on a wrapper:
// the input is the field, so `Field.Control`'s `id`, `aria-describedby` and
// `aria-invalid` land where they mean something. `InputOtp.Root` renders no
// wrapper at all, so the layout around the slots is the caller's, whole.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useMemo, useState } from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, withoutComposed } from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";

/**
 * What a code is made of.
 *
 * A union rather than a `pattern` string, because the answer decides three
 * things at once — which characters survive typing and pasting, which keyboard
 * a phone shows, and what the field's `pattern` says — and a caller who writes
 * `kind="numberic"` should be told at the call rather than discover that their
 * numeric field takes letters.
 */
export type InputOtpKind = "numeric" | "alphanumeric";

type InputOtpState = {|
  readonly value: string,
  readonly length: number,
  readonly focused: boolean,
|};

const InputOtpContext: React.Context<InputOtpState | null> = createContext(null);

/**
 * The field a part belongs to.
 *
 * Raising rather than returning null, for the reason `useDialog` gives: an
 * `InputOtp.Slot` outside a root would render an empty box that never fills,
 * and it would look correct until somebody typed.
 */
hook useInputOtp(part: string): InputOtpState {
  const state = useContext(InputOtpContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside an InputOtp.Root`);
  }
  return state;
}

/** Everything but the code, removed as it arrives — typed or pasted. */
function clean(raw: string, kind: InputOtpKind, length: number): string {
  const kept = match (kind) {
    "numeric" => raw.replace(/[^0-9]/g, ""),
    "alphanumeric" => raw.replace(/[^0-9A-Za-z]/g, ""),
  };
  return kept.slice(0, length);
}

/**
 * The field: one input, and the slots that draw it.
 *
 * Renders no element of its own beyond the input — see the module header. The
 * caller's props go on the input, because the input is the field.
 */
export component InputOtpRoot(
  children: React.Node,
  defaultValue?: string = "",
  disabled?: boolean = false,
  kind?: InputOtpKind = "numeric",
  label: string,
  length: number,
  name?: string,
  onComplete?: (code: string) => void,
  onValueChange?: (value: string) => void,
  value?: string,
  ...rest: Rest
) {
  const [code, setCode] = useControlled(value, defaultValue, onValueChange);
  const [focused, setFocused] = useState(false);
  const passed = withoutComposed(rest, ["onBlur", "onChange", "onFocus"]);
  // `aria-labelledby` wins over `aria-label`, so a field wired through
  // `Field.Control` keeps the label the caller actually rendered.
  const named = rest["aria-labelledby"] != null;

  const state = useMemo(() => ({ focused, length, value: code }), [focused, length, code]);

  return (
    <InputOtpContext.Provider value={state}>
      <input
        {...passed}
        aria-label={named ? undefined : label}
        // The reason the component exists. Without it the operating system has
        // no field to offer the code it just received by SMS to.
        autoComplete="one-time-code"
        disabled={disabled}
        inputMode={kind === "numeric" ? "numeric" : "text"}
        maxLength={length}
        name={name}
        onBlur={composeHandlers(rest.onBlur, () => setFocused(false))}
        onChange={composeHandlers(rest.onChange, (event: $FlowFixMe) => {
          // One event whether a character was typed or six were pasted, which
          // is why "pasting fills every box" needs no code of its own.
          const next = clean(String(event.currentTarget?.value ?? ""), kind, length);
          setCode(next);
          if (next.length === length) {
            onComplete?.(next);
          }
        })}
        onFocus={composeHandlers(rest.onFocus, () => setFocused(true))}
        pattern={kind === "numeric" ? "[0-9]*" : "[0-9A-Za-z]*"}
        type="text"
        value={code}
      />
      {children}
    </InputOtpContext.Provider>
  );
}

/**
 * A run of slots, drawn together.
 *
 * `aria-hidden`, like everything else that draws the value: the field has
 * already been announced, and a reader told "group, 3, group, 4" is being read
 * a picture.
 */
export component InputOtpGroup(children: React.Node, ...rest: Rest) {
  return (
    <div {...rest} aria-hidden="true">
      {children}
    </div>
  );
}

/**
 * One box.
 *
 * `data-active` is the box the next character goes in and `data-filled` is
 * whether there is one in it already, so a stylesheet can draw a cursor and a
 * border without this module having an opinion about either.
 */
export component InputOtpSlot(children?: React.Node, index: number, ...rest: Rest) {
  const otp = useInputOtp("InputOtp.Slot");
  const character = otp.value[index] ?? "";
  // The box the next character goes in, clamped so a full code lights its last
  // box rather than nothing at all.
  const active = otp.focused && index === Math.min(otp.value.length, otp.length - 1);

  return (
    <div
      {...rest}
      aria-hidden="true"
      data-active={active ? "true" : undefined}
      data-filled={character === "" ? undefined : "true"}
      data-index={String(index)}
    >
      {children ?? character}
    </div>
  );
}

/** The dash between two groups. Decoration, and it says so. */
export component InputOtpSeparator(children?: React.Node, ...rest: Rest) {
  return (
    <div {...rest} aria-hidden="true">
      {children}
    </div>
  );
}
