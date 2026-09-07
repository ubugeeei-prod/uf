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
//
// Which button that is, and whether a form with no button submits at all, are
// both the specification's questions rather than this component's, and both are
// answered below: a submit button belongs to the form that *owns* it rather
// than to the form it sits inside, and a form with no submit button submits
// itself only while at most one of its fields blocks implicit submission.

"use client";

import * as React from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, withoutComposed } from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";

/**
 * The button a form would submit itself through, or nothing.
 *
 * The specification's "default button" is the first submit button in tree order
 * **whose form owner is this form**, and neither half of that is "a
 * descendant". A control's form owner is the `form` attribute when it has one
 * and the nearest ancestor `<form>` otherwise, so the two cases a subtree
 * search gets wrong are both real markup:
 *
 *   * `<button type="submit" form="signup">` *beside* the form — the pattern a
 *     dialog's footer is written in — is the default button and a subtree
 *     search never sees it. Missing it is not a small error: it falls through
 *     to the no-submitter branch, which is the exact `event.submitter` this
 *     component exists to answer.
 *   * `<button type="submit" form="other">` *inside* the form belongs to the
 *     other one, and handing it to `requestSubmit` throws `NotFoundError` —
 *     which reaches a reader as a key that does nothing and a console the page
 *     did not write.
 *
 * So the search is over the form's root and each candidate is asked which form
 * it belongs to. The root rather than the document, because a form in a shadow
 * tree, or one rendered but not yet inserted, has to find its own buttons and
 * only its own.
 *
 * A `<button>` with no `type` is a submit button, which is the case most easily
 * missed. A disabled one is skipped, because the browser skips it — implicit
 * submission through a button nobody could press is not a thing the platform
 * does.
 */
function defaultButtonOf(form: HTMLElement): HTMLElement | null {
  const searched: $FlowFixMe = (form as $FlowFixMe).getRootNode?.() ?? form.ownerDocument;
  if (searched == null || typeof searched.querySelectorAll !== "function") {
    return null;
  }
  const candidates = searched.querySelectorAll(
    'button:not([type]), button[type="submit"], input[type="submit"], input[type="image"]',
  );
  for (const candidate of candidates) {
    const button: $FlowFixMe = candidate;
    if (button.form === form && button.disabled !== true) {
      return button as $FlowFixMe;
    }
  }
  return null;
}

/**
 * The `<input>` types that block implicit submission.
 *
 * The specification's list, copied rather than reasoned about, because what is
 * being reproduced is what the browser does. A checkbox, a radio, a hidden
 * field, a `<select>` and a `<textarea>` are not on it.
 */
const BLOCKING_TYPES: Set<string> = new Set([
  "date",
  "datetime-local",
  "email",
  "month",
  "number",
  "password",
  "search",
  "tel",
  "text",
  "time",
  "url",
  "week",
]);

/**
 * Whether the platform would decline to submit `form` from the form itself.
 *
 * The other half of the implicit submission rule, and the half a script never
 * meets: a form with **no** submit button is submitted implicitly only when at
 * most one of its fields blocks implicit submission. A login form with a
 * username and a password and no button is the everyday case — `Enter` in
 * either field does nothing at all in every browser.
 *
 * `requestSubmit()` does not apply that rule, and is right not to: it is the
 * route a script takes to submit deliberately. A component reproducing the
 * *implicit* mechanism has to apply it here, or `Enter` on a checkbox submits
 * forms that `Enter` in the text field beside it would not — which is the same
 * class of divergence this whole change is about, pointing the other way.
 */
function moreThanOneFieldBlocks(form: $FlowFixMe): boolean {
  const fields: $FlowFixMe = form.elements;
  if (fields == null) {
    return false;
  }
  let blocking = 0;
  for (const field of fields) {
    const control: $FlowFixMe = field;
    // `.type` rather than the attribute: it is missing on `<input>` and any
    // value the specification does not know is the Text state, and both of
    // those block.
    if (control.tagName === "INPUT" && BLOCKING_TYPES.has(String(control.type))) {
      blocking += 1;
      if (blocking > 1) {
        return true;
      }
    }
  }
  return false;
}

/**
 * Submit the form this control is in, the way `Enter` on a native checkbox does.
 *
 * Does nothing when there is no form, when the browser has no `requestSubmit` —
 * it is everywhere current, and a checkbox that threw on an old one would be
 * worse than a key that does nothing — or when the form is one the platform
 * would not submit implicitly either, which is the rule
 * `moreThanOneFieldBlocks` states.
 */
function submitImplicitly(control: HTMLElement): void {
  const form: $FlowFixMe = (control as $FlowFixMe).form;
  if (form == null || typeof form.requestSubmit !== "function") {
    return;
  }
  const submitter = defaultButtonOf(form);
  if (submitter == null) {
    // A form with no submit button submits itself, but only under the rule
    // `moreThanOneFieldBlocks` carries — `requestSubmit()` will not apply it,
    // so this does.
    if (!moreThanOneFieldBlocks(form)) {
      form.requestSubmit();
    }
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
