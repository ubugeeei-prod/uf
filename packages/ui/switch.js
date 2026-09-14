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
// something that operates is what a reader expects — while `checkbox.js` turns
// `Enter` into the submission of the form the checkbox is in, which is what a
// native `<input type="checkbox">` does with the key and what a reader
// answering a question on their way to a submit button is asking for. That is
// the whole reason these are not one file with a flag.

"use client";

import * as React from "@uniflowed/react";

import type { RenderProp, Rest } from "./internal/merge-props.js";
import { withInteraction, withProps } from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";
import { usePress } from "./interactions.js";

/**
 * A two-state switch: on or off.
 *
 * `render` is the escape hatch, and on a switch it is what a caller with their
 * own `<Pressable>` or a `<div role="switch">` in a design system reaches for.
 * `type="button"` is the one thing it does not hand over, because that is true
 * of the element rather than of the switch.
 */
export component Switch(
  checked?: boolean,
  defaultChecked?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  children?: React.Node,
  render?: RenderProp,
  ...rest: Rest
) {
  const [on, setOn] = useControlled(checked, defaultChecked, onCheckedChange);
  // A press: `Space` as the key comes up and `Enter` as it goes down, once for a
  // held key. Claiming both keys is not decoration: it stops `Space` scrolling
  // the page — which is what makes a hand-written toggle feel broken even when
  // it works — and it stops the browser clicking the switch a second time. See
  // `interactions.js`.
  const { pressProps } = usePress({ isDisabled: disabled, onPress: () => setOn(!on) });
  const props = withProps(withInteraction(rest, pressProps, []), {
    "aria-checked": on ? "true" : "false",
    children,
    disabled,
    role: "switch",
  });

  if (render != null) {
    return render(props);
  }
  return <button {...props} type="button" />;
}
