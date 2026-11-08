"use flow";
//
// What a form submits for a control that is only a button.
//
// A checkbox and a switch render a `<button>`. A form never sees a button that
// is not a submitter, so `name` has to render the hidden input
// `form-value.js` argues for, and the two controls have to agree about when
// that input is omitted. The agreement lives here because the failure mode of
// writing it twice is a switch that posts while it is off and a checkbox that
// does not.
//
// The value is the caller's, and `"on"` when they did not give one, which is
// the string a native checkbox submits. Unchecked is omitted by disabling the
// input. An empty string would still be a field, and a clear checkbox is not
// one. Disabled is omitted the same way, because that is what the browser does
// with a disabled control.
//
// Reset is the form's event, and it is not the hidden input's memory. The
// browser puts that input back to the value it was mounted with, which is a
// different question from `defaultChecked` after the reader has toggled. The
// listener runs in a microtask: happy-dom resets before it dispatches, and a
// browser resets as the event's default action, after the listener returns.
// Either way the microtask is afterwards.
//
// An uncontrolled control returns to `defaultChecked`. A controlled one tells
// `onCheckedChange` and then puts the field back to the value still on screen,
// so a parent that keeps its value does not submit the browser's guess. A
// parent that applies the reset renders again and overwrites that write.

"use client";

import * as React from "@uniflowed/react";
import { useEffect, useRef } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import { FormValue } from "./form-value.js";

/**
 * The form a control submits with.
 *
 * A button knows, via `.form`. A `div` — a radio group — does not, so the
 * attribute is asked of the root the element is in, and then the ancestor
 * form. The root rather than the document, for the same reason
 * `checkbox.js` searches the root for a default button: a form in a shadow
 * tree has to find its own.
 */
function owningForm(element: HTMLElement, form: string | void): HTMLElement | null {
  const node: $FlowFixMe = element;
  const associated: HTMLElement | null = node.form ?? null;
  if (associated != null) {
    return associated;
  }
  if (typeof form === "string" && form !== "") {
    const root: $FlowFixMe = element.getRootNode?.() ?? element.ownerDocument;
    const named: HTMLElement | null = root?.getElementById?.(form) ?? null;
    if (named != null) {
      return named;
    }
  }
  return node.closest?.("form") ?? null;
}

/**
 * Run `restore` once a form reset has finished putting native fields back.
 *
 * `form` is the control's `form` attribute, when it has one. It is a
 * dependency because that string changing is the owner changing.
 */
export hook useFormReset(
  control: { current: HTMLElement | null },
  restore: () => void,
  form   : string | void,
): void {
  useEffect(() => {
    const element = control.current;
    const owner = element == null ? null : owningForm(element, form);
    if (owner == null) {
      return;
    }
    const onReset = () => {
      queueMicrotask(restore);
    };
    owner.addEventListener("reset", onReset);
    return () => owner.removeEventListener("reset", onReset);
  }, [control, form, restore]);
}

/**
 * The hidden field a checkable control submits, and the reset that owns it.
 *
 * `owned` is "the caller did not pass `checked`". The setter is
 * `useControlled`'s: it writes internal state only while `owned`, and it
 * always reports.
 */
export hook useNamedCheck(
  name          : string | void,
  value         : string,
  checked       : boolean,
  disabled      : boolean,
  defaultChecked: boolean,
  owned         : boolean,
  setChecked    : (next: boolean) => void,
  form          : string | void,
): {| field: React.Node, ref: (element: HTMLElement | null) => void |} {
  const controlRef = useRef<HTMLElement | null>(null);
  const restore    = useStableCallback(() => {
    setChecked(defaultChecked);
    if (owned) {
      return;
    }
    // The parent was told. Until it renders, the field would show whatever
    // the browser just restored, which is not the value still announced.
    const input: $FlowFixMe = controlRef.current?.querySelector?.("input[type='hidden']");
    if (input == null) {
      return;
    }
    input.disabled = disabled || !checked;
    input.value = value;
  });
  useFormReset(controlRef, restore, form);
  const ref = (element: HTMLElement | null) => {
    // Read later by the reset listener, not by this render.
    controlRef.current = element;
  };
  const field =
    name == null ? null : (
      <FormValue disabled={disabled || !checked} form={form} name={name} value={value} />
    );
  return { field, ref };
}
