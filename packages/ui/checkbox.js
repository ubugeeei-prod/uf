// @flow
//
// A checkbox that is not an `<input>`, with the third state a checkbox has.
//
// A styled checkbox is almost always a `div` with a tick drawn in it, and the
// moment it stops being a real control it stops being announced, stops toggling
// on `Space`, and stops being reachable by `Tab`. This keeps all three while
// shipping no styles.
//
// # The third state is the reason this is not `switch.js`
//
// A checkbox has three states — checked, unchecked and *mixed* — and a switch
// has two. `aria-checked="mixed"` is what a "select all" box says when some of
// its rows are selected, and there is no way to express it with a switch, which
// is why this is a component of its own rather than `switch.js` with a
// different `role`.
//
// Mixed is the caller's to own. A control cannot decide on its own that it is
// no longer partly selected — that is a fact about the rows it summarises — so
// `indeterminate` is a prop, and clicking a mixed checkbox reports `true`,
// which is the state a reader expects "select all" to move to.
//
// # `Enter` submits the form; it does not toggle
//
// This is the difference between a control that answers a question and one that
// operates a thing — `switch.js` takes `Enter` because a switch is the second
// kind — and for a long time this header claimed it by *leaving the key alone*.
// Neither half of that claim survived contact with the component, which is
// ubugeeei-prod/uf#324:
//
//   * **A `<button type="button">` never submits a form.** That is the whole of
//     what `type="button"` means, and this renders one. So the form was not
//     submitted by anybody.
//   * **And leaving a key unhandled does not make it inert.** It lets the
//     *default action* happen, and the default action of `Enter` on a focused
//     `<button>` is a click — which this component's own `onClick` turns into a
//     toggle. So a focused checkbox both failed to submit the form and changed
//     its own state, which is the opposite of the intent on both counts.
//
// What a native `<input type="checkbox">` does with `Enter` is *implicit
// submission*: the key does not touch the checkbox, and the form around it is
// submitted as though its default button had been pressed. That is the
// behaviour this replaces, so it is the behaviour it owes, and it is written
// out here because a styled substitute that silently drops it is exactly the
// kind of regression `index.js` says this package exists to prevent.
//
// So `Enter` is claimed — `preventDefault()`, which is what stops the browser's
// click and the toggle behind it — and turned back into a submission of the
// `<form>` the control is in. Outside a form it does nothing at all, which is
// again what the native control does. `requestSubmit` rather than `submit`
// because only the first fires the `submit` event and runs constraint
// validation, and a `<form onSubmit>` that never heard about the submission is
// the failure this would otherwise trade for the old one.
//
// The default button is passed to it rather than left out. `requestSubmit()`
// with no argument submits with *no* submitter, so a form whose handler reads
// `event.submitter` — a Server Action's `formAction`, a "save" and a "save and
// close" beside each other — would be told nobody pressed anything. Implicit
// submission names the default button, so this does too.

"use client";

import * as React from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, withoutComposed } from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";

/**
 * The button a form would submit itself through, or nothing.
 *
 * The specification's "default button" is the first submit button in tree
 * order; a `<button>` with no `type` is one, which is the case most easily
 * missed. A disabled one is skipped, because the browser skips it — implicit
 * submission through a button nobody could press is not a thing the platform
 * does.
 */
function defaultButtonOf(form: HTMLElement): HTMLElement | null {
  const candidates = form.querySelectorAll(
    'button:not([type]), button[type="submit"], input[type="submit"], input[type="image"]',
  );
  for (const candidate of candidates) {
    if ((candidate as $FlowFixMe).disabled !== true) {
      return candidate as $FlowFixMe;
    }
  }
  return null;
}

/**
 * Submit the form this control is in, the way `Enter` on a native checkbox does.
 *
 * Does nothing when there is no form, when the browser has no `requestSubmit` —
 * it is everywhere current, and a checkbox that threw on an old one would be
 * worse than a key that does nothing — or when the form has no way of being
 * submitted at all, which the platform also treats as "no implicit submission".
 */
function submitImplicitly(control: HTMLElement): void {
  const form: $FlowFixMe = (control as $FlowFixMe).form;
  if (form == null || typeof form.requestSubmit !== "function") {
    return;
  }
  const submitter = defaultButtonOf(form);
  if (submitter == null) {
    // A form with no submit button still submits implicitly when it has exactly
    // one field the browser calls "blocking"; that rule is the browser's to
    // apply and `requestSubmit()` with no submitter is how it is asked.
    form.requestSubmit();
    return;
  }
  form.requestSubmit(submitter);
}

/** A checkbox, which may also be mixed. */
export component Checkbox(
  checked?: boolean,
  defaultChecked?: boolean = false,
  indeterminate?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  children?: React.Node,
  ...rest: Rest
) {
  const [on, setOn] = useControlled(checked, defaultChecked, onCheckedChange);
  // A mixed checkbox moves to checked, not to "the opposite of the boolean
  // underneath it": a half-selected "select all" that clears itself on the
  // first click is the behaviour every table in every application gets wrong.
  const next = indeterminate ? true : !on;
  const passed = withoutComposed(rest, ["onClick", "onKeyDown"]);

  return (
    <button
      {...passed}
      aria-checked={indeterminate ? "mixed" : on ? "true" : "false"}
      disabled={disabled}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          setOn(next);
        }
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (disabled) {
          return;
        }
        if (event.key === " ") {
          // Stops `Space` scrolling the page, and stops the browser's own click
          // arriving afterwards and toggling this a second time.
          event.preventDefault();
          setOn(next);
          return;
        }
        if (event.key === "Enter") {
          // Claimed, and *not* to make the key inert: the default action here
          // is a click on this button, and a click on this button toggles. See
          // the module header for the whole of it.
          event.preventDefault();
          submitImplicitly(event.currentTarget as $FlowFixMe);
        }
      })}
      role="checkbox"
      type="button"
    >
      {children}
    </button>
  );
}
