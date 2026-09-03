// @flow
//
// A switch, and a checkbox that is not an `<input>`.
//
// Both exist because the styled version of a checkbox is almost always a `div`
// with a tick drawn in it, and the moment it stops being an `<input>` it stops
// being announced, stops toggling on Space, and stops being reachable by Tab.
// These keep all three: the role, the `aria-checked` state, and the keys.
//
// A switch is not a checkbox. A checkbox has three states — on, off and
// indeterminate — and a switch has two; a screen reader says "on"/"off" for one
// and "checked"/"unchecked" for the other. Using the wrong one is the kind of
// mistake that is invisible until somebody uses the thing.

import * as React from "@uniflowed/react";
import { useCallback, useState } from "@uniflowed/react";

import { composeHandlers, withoutComposed } from "./props.js";

/** Space toggles, and so does Enter, because both do on a native control. */
function toggleKeys(
  event: SyntheticKeyboardEvent<HTMLElement>,
  toggle: () => void,
): void {
  if (event.key !== " " && event.key !== "Enter") {
    return;
  }
  // Space scrolls the page otherwise, which is what makes a hand-written
  // toggle feel broken even when it works.
  event.preventDefault();
  toggle();
}

/** State for a control that may be controlled or not. */
function useToggle(
  checked: boolean | void,
  defaultChecked: boolean,
  onCheckedChange: ((checked: boolean) => void) | void,
): [boolean, () => void] {
  const [internal, setInternal] = useState(defaultChecked);
  const current = checked ?? internal;

  const toggle = useCallback(() => {
    const next = !current;
    if (checked == null) {
      setInternal(next);
    }
    onCheckedChange?.(next);
  }, [checked, current, onCheckedChange]);

  return [current, toggle];
}

/** A two-state switch: on or off. */
export component Switch(
  checked?: boolean,
  defaultChecked?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  children?: React.Node,
  ...rest: { +[string]: mixed }
) {
  const [on, toggle] = useToggle(checked, defaultChecked, onCheckedChange);
  const passed = withoutComposed(rest, ["onClick", "onKeyDown"]);

  return (
    <button
      {...passed}
      aria-checked={on ? "true" : "false"}
      disabled={disabled}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          toggle();
        }
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (!disabled) {
          toggleKeys(event, toggle);
        }
      })}
      role="switch"
      type="button"
    >
      {children}
    </button>
  );
}

/** A checkbox, which may also be indeterminate. */
export component Checkbox(
  checked?: boolean,
  defaultChecked?: boolean = false,
  indeterminate?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  children?: React.Node,
  ...rest: { +[string]: mixed }
) {
  const [on, toggle] = useToggle(checked, defaultChecked, onCheckedChange);
  const passed = withoutComposed(rest, ["onClick", "onKeyDown"]);

  return (
    <button
      {...passed}
      // "mixed" is the third state, and it is why a checkbox cannot simply be
      // a switch with a different label.
      aria-checked={indeterminate ? "mixed" : on ? "true" : "false"}
      disabled={disabled}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          toggle();
        }
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (!disabled) {
          toggleKeys(event, toggle);
        }
      })}
      role="checkbox"
      type="button"
    >
      {children}
    </button>
  );
}
