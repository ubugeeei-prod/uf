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
// # `Enter` is deliberately not handled
//
// `Space` toggles; `Enter` is left alone, so a checkbox inside a form still
// submits it. That is the difference between a control that answers a question
// and one that operates a thing — `switch.js` takes `Enter` because a switch is
// the second kind.

"use client";

import * as React from "@uniflowed/react";

import { composeHandlers, withoutComposed } from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";

/** A checkbox, which may also be mixed. */
export component Checkbox(
  checked?: boolean,
  defaultChecked?: boolean = false,
  indeterminate?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  children?: React.Node,
  ...rest: { readonly [string]: mixed }
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
        if (disabled || event.key !== " ") {
          return;
        }
        // Stops `Space` scrolling the page, and stops the browser's own click
        // arriving afterwards and toggling this a second time.
        event.preventDefault();
        setOn(next);
      })}
      role="checkbox"
      type="button"
    >
      {children}
    </button>
  );
}
