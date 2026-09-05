// @flow
//
// A switch: two states, and a screen reader that says which.
//
// It exists because the styled version of an on/off control is almost always a
// `div` with a knob drawn in it, and the moment it stops being a real control it
// stops being announced, stops toggling on `Space`, and stops being reachable by
// `Tab`. This keeps all three — the role, the `aria-checked` state, and the
// keys — while shipping no styles at all.
//
// # A switch is not a checkbox
//
// A screen reader says "on" and "off" for a switch and "checked" and
// "unchecked" for a checkbox, and the two are not interchangeable: a checkbox
// answers a question ("include me in the mailing list") and a switch operates a
// thing ("notifications, on"). A checkbox also has a third state that a switch
// does not, which is why `checkbox.js` is a separate component rather than this
// one with a different `role`.
//
// The keyboard follows from the same distinction. `Space` toggles both. `Enter`
// toggles a *switch*, because a switch is an operation and pressing Enter on
// something that operates is what a reader expects — while `checkbox.js`
// deliberately leaves `Enter` alone so that a checkbox inside a form still
// submits it. That is the whole reason these are not one file with a flag.

"use client";

import * as React from "@uniflowed/react";

import { composeHandlers, withoutComposed } from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";

/** A two-state switch: on or off. */
export component Switch(
  checked?: boolean,
  defaultChecked?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  children?: React.Node,
  ...rest: { readonly [string]: mixed }
) {
  const [on, setOn] = useControlled(checked, defaultChecked, onCheckedChange);
  const passed = withoutComposed(rest, ["onClick", "onKeyDown"]);

  return (
    <button
      {...passed}
      aria-checked={on ? "true" : "false"}
      disabled={disabled}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          setOn(!on);
        }
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (disabled || (event.key !== " " && event.key !== "Enter")) {
          return;
        }
        // Preventing the default is not decoration. It stops `Space` scrolling
        // the page — which is what makes a hand-written toggle feel broken even
        // when it works — and it stops the browser's own click from arriving
        // after this handler and toggling the switch a second time.
        event.preventDefault();
        setOn(!on);
      })}
      role="switch"
      type="button"
    >
      {children}
    </button>
  );
}
