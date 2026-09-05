// @flow
//
// The one contract every part of this package makes about state.
//
// Each primitive here has a value somebody may want to own: a dialog's open,
// a tab set's selection, a switch's checked, a combobox's text. A library that
// only supports one of the two arrangements is unusable in the other half of
// applications — a form library owns the value, and a page that just wants tabs
// does not — so every one of them is uncontrolled by default and controlled the
// moment the corresponding prop is passed.
//
// Written once because the failure mode of writing it six times is that five of
// them agree and one does not, and the one that does not is a component that
// silently ignores the parent's value on the second render. The rules it fixes:
//
//   * `undefined` means "not controlled", and `null` does not. A combobox with
//     no selection is `value={null}` and is still controlled.
//   * A controlled component never writes its internal state, so a parent that
//     rejects a change actually rejects it, rather than the component moving
//     and then being moved back on the next render.
//   * `onChange` is called for both arrangements, so a caller can observe
//     without taking ownership.
//
// # Why this is `internal/` and not a subpath
//
// A public `useControlled` would be a general-purpose hook, and general-purpose
// React hooks are `@uniflowed/hooks`' subject, not this package's. What lives
// here is narrower than that: the specific contract this package's components
// promise about their props. Exporting it would publish a second, weaker copy
// of somebody else's API.

import { useCallback, useState } from "@uniflowed/react";

/**
 * A value the caller may own, and the setter that respects the answer.
 *
 * `controlled` is the prop; `fallback` is the `defaultValue`-shaped initial
 * state used only while the caller owns nothing.
 */
export hook useControlled<T>(
  controlled: T | void,
  fallback: T,
  onChange: ((next: T) => mixed) | void,
): [T, (next: T) => void] {
  const [internal, setInternal] = useState<T>(fallback);
  // `=== undefined` rather than `== null`: `null` is a legitimate controlled
  // value — a combobox with nothing selected — and treating it as "give me the
  // uncontrolled behaviour" made a cleared selection reappear on the next
  // render.
  const owned = controlled === undefined;

  const set = useCallback(
    (next: T) => {
      if (owned) {
        setInternal(next);
      }
      // Both arrangements report, so a caller can watch a value it does not
      // own without having to take it over to do so.
      onChange?.(next);
    },
    [owned, onChange],
  );

  return [owned ? internal : (controlled as $FlowFixMe), set];
}
