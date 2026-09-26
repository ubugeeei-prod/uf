// @flow
//
// A toggle button: an action that stays applied.
//
// This is the third of the package's two-state controls, and the argument that
// separates them is already half written in `switch.js` and `checkbox.js`. What
// a reader is told is the whole of it:
//
//   * **`role="switch"`** — "notifications, switch, on". A *setting*. It takes
//     effect where it is, and `switch.js` explains why it is not a checkbox.
//   * **`role="checkbox"`** — "subscribe, checkbox, checked". An *answer* to a
//     question, usually in a form, usually not doing anything until the form is
//     submitted, and the only one of the three with a third state.
//   * **`aria-pressed`** — "bold, toggle button, pressed". An *action* that
//     stays applied. Bold in a toolbar. Pressing it does the thing immediately,
//     and the pressed state is a report of what was done rather than a value
//     anybody is going to submit.
//
// A toggle button announced as a checkbox tells a reader they are answering a
// question, and one announced as a switch tells them they are configuring
// something; a toolbar full of either is a toolbar nobody can use by ear. The
// three are separate files rather than one with a `role` prop because the
// keyboard, the states and the reason to reach for each of them differ, and a
// flag would let a caller pick the wrong one without ever being told what the
// difference was.
//
// # `Space` and `Enter` both press it
//
// Because a toggle button is a button, and a button activates on both. That is
// the same reasoning `switch.js` gives for `Enter`, and the opposite of
// `checkbox.js`, where `Enter` submits the form rather than touching the
// control — because a checkbox is something a reader answers on their way to
// submitting a form, and that is what the native one does with the key.
//
// # It is `disabled`, not `aria-disabled`
//
// A lone toggle button that cannot be pressed should be out of the tab order,
// like the `<button>` it is — nothing is lost, because a reader who never
// reaches it never wonders where the rest of the set went. `ToggleGroup.Item`
// makes the opposite choice for the opposite reason: an unavailable item *in a
// set* has to stay announced, or a reader finds a gap they cannot ask about.

"use client";

import * as React from "@uniflowed/react";

import type { PartEvent, RenderProp, Rest } from "./internal/merge-props.js";
import { composeHandlers, withProps, withoutComposed } from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";

/**
 * A button whose state stays applied: pressed or not.
 *
 * `render` is the escape hatch. A caller-rendered element gets `role="button"`
 * because it may be a link or a `div`; the native button branch keeps the role
 * implicit and only adds `type="button"`, which is true of the element rather
 * than of the toggle behaviour.
 */
export component Toggle(
  pressed?: boolean,
  defaultPressed?: boolean = false,
  onPressedChange?: (pressed: boolean) => void,
  disabled?: boolean = false,
  children?: React.Node,
  render?: RenderProp,
  ...rest: Rest
) {
  const [on, setOn] = useControlled(pressed, defaultPressed, onPressedChange);
  const passed = withoutComposed(rest, ["onClick", "onKeyDown"]);
  const semantics = {
    "aria-pressed": on ? "true" : "false",
    children,
    disabled,
    onClick: composeHandlers(rest.onClick, (_event: PartEvent) => {
      if (!disabled) {
        setOn(!on);
      }
    }),
    onKeyDown: composeHandlers(rest.onKeyDown, (event: PartEvent) => {
      if (disabled || (event.key !== " " && event.key !== "Enter")) {
        return;
      }
      // Preventing the default stops `Space` scrolling the page, and stops
      // the browser's own click arriving after this handler and pressing the
      // button a second time — back to where it started, which reads as the
      // key having done nothing at all.
      event.preventDefault();
      setOn(!on);
    }),
  };

  if (render != null) {
    return render(withProps(passed, { ...semantics, role: "button" }));
  }

  return (
    <button
      {...withProps(passed, semantics)}
      // No `role`: this *is* a button, and `aria-pressed` is what makes it a
      // toggle one. Adding `role="button"` to a `<button>` would be noise, and
      // adding any other role would be a lie about what pressing it does.
      type="button"
    />
  );
}
