// @flow
//
// A row of toggle buttons that behaves as one control.
//
// It looks like a styling decision — the same buttons, drawn joined up — and it
// is two: one tab stop for the whole set instead of one per button, and a
// choice about what a reader is told the set *is*.
//
// # `type` is two sets of semantics, not a flag
//
//   * **`multiple`** — a group of toggle buttons. `role="group"` around
//     `aria-pressed` buttons, any number of them pressed at once. Text
//     alignment left/centre/right is not this; bold/italic/underline is.
//   * **`single`** — a radio group drawn as segments. `role="radiogroup"` and
//     `role="radio"`, because a reader told "three pressed buttons" will
//     reasonably believe they may press all three, and they may not. Choosing
//     one unchooses the rest, and — as in any radio group — there is no gesture
//     that unchooses without choosing something else.
//
// So `single` is not a variant of `multiple` with a constraint bolted on: it is
// a different widget, and it is the one this package already ships.
// `radio-group.js` renders it — `ToggleGroup.Root` renders a `RadioGroup.Root`
// and `ToggleGroup.Item` renders a `RadioGroup.Item` — so the arrow keys that
// check as they move, the initial tab stop when nothing is chosen, and the
// `aria-checked` wiring have exactly one implementation and cannot drift into
// two that disagree. What is left here is the `multiple` mode and the choice
// between them.
//
// The one thing that reuse costs is stated in `radio-group.js`:
// `RadioGroup.Root` takes `React.Node` rather than `renders* RadioGroupItem`,
// because the items arriving through here produce a radio in one mode and a
// toggle button in the other, and no `renders*` promise can hold for both.
//
// # One value type for both modes
//
// `value` is always a `$ReadOnlyArray<string>` — the items that are on. In
// `single` mode the array holds at most one, and that invariant is the
// component's to keep rather than the caller's to remember. The alternative,
// `string | $ReadOnlyArray<string>` chosen by `type`, is a type that makes
// every caller narrow at every use for a shape they already know, and Flow
// cannot tie it to a sibling prop's value anyway; a type that has to be
// narrowed past is a type that has given up.
//
// # Naming the set
//
// A group with no name is announced as "group" and nothing else. Pass
// `aria-label`, or wire `Field.Label` through `Field.Control` the way
// `radio-group.js` shows.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useCallback,
  useContext,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  forwarded,
  withoutComposed,
} from "./internal/merge-props.js";
import { moveOnKey, useFirstItem } from "./internal/roving-focus.js";
import type { Orientation, RovingSet } from "./internal/roving-focus.js";
import { RadioGroupItem, RadioGroupRoot } from "./radio-group.js";
import { useControlled } from "./internal/controlled-state.js";

/** Whether the set holds one answer or any number of them. */
export type ToggleGroupType = "single" | "multiple";

/**
 * The empty selection, hoisted so it is one array rather than a new one per
 * render — a fresh `[]` default would be a new identity in every `useMemo`
 * dependency list that ever holds it.
 */
const NOTHING: $ReadOnlyArray<string> = [];

/**
 * What the keyboard steps across in a `multiple` group.
 *
 * `[aria-pressed]` rather than a `data-*` name of this package's own, for the
 * same reason `radio-group.js` looks for `[role="radio"]`: the attribute is
 * what makes an element one of these, so anything that carries it is something
 * the arrow keys must reach. A `single` group asks `radio-group.js` instead.
 */
function toggleSet(orientation: Orientation): RovingSet {
  return {
    item: "[aria-pressed]",
    owner: '[role="group"]',
    orientation,
    wrap: true,
    skipDisabled: true,
  };
}

type ToggleGroupState = {|
  readonly type: ToggleGroupType,
  readonly pressed: $ReadOnlyArray<string>,
  readonly toggle: (value: string) => void,
  /** The item focus last visited, which holds the tab stop for the set. */
  readonly activeId: string | null,
  readonly setActiveId: (id: string) => void,
  /** The item holding the tab stop before focus has visited any; see `useFirstItem`. */
  readonly firstId: string | null,
|};

const ToggleGroupContext: React.Context<ToggleGroupState | null> = createContext(null);

hook useToggleGroup(part: string): ToggleGroupState {
  const state = useContext(ToggleGroupContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a ToggleGroup.Root`);
  }
  return state;
}

/**
 * The set.
 *
 * Uncontrolled by default and controlled the moment `value` is passed, like
 * everything else here.
 */
export component ToggleGroupRoot(
  children: renders* ToggleGroupItem,
  type?: ToggleGroupType = "multiple",
  defaultValue?: $ReadOnlyArray<string> = NOTHING,
  value?: $ReadOnlyArray<string>,
  onValueChange?: (value: $ReadOnlyArray<string>) => void,
  orientation?: Orientation = "horizontal",
  ...rest: Rest
) {
  const [pressed, setPressed] = useControlled<$ReadOnlyArray<string>>(
    value,
    defaultValue,
    onValueChange,
  );
  const rootRef = useRef<HTMLElement | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  // Only for `multiple`, and only until focus has been somewhere: a `single`
  // group's tab stop is `radio-group.js`'s business, and once focus has landed
  // the item it landed on holds it.
  const firstId = useFirstItem(
    rootRef,
    toggleSet(orientation),
    type === "multiple" && activeId == null,
  );

  const toggle = useCallback(
    (item: string) => {
      setPressed(
        pressed.includes(item) ? pressed.filter((each) => each !== item) : [...pressed, item],
      );
    },
    [pressed, setPressed],
  );
  // Stable, so the radio group below does not get a new `onValueChange` on
  // every render and hand every one of its items a new context with it.
  const chooseOne = useCallback((next: string) => setPressed([next]), [setPressed]);

  const state = useMemo(
    () => ({ type, pressed, toggle, activeId, setActiveId, firstId }),
    [type, pressed, toggle, activeId, firstId],
  );

  if (type === "single") {
    // The whole of the single mode. `radio-group.js` owns the arrow keys that
    // check as they move, the tab stop while nothing is chosen, the
    // `aria-checked` wiring and the `role="radiogroup"` container; a second
    // copy of any of it here would be a second thing to keep correct.
    return (
      <ToggleGroupContext.Provider value={state}>
        <RadioGroupRoot
          {...forwarded(rest)}
          onValueChange={chooseOne}
          orientation={orientation}
          value={pressed[0] ?? null}
        >
          {children}
        </RadioGroupRoot>
      </ToggleGroupContext.Provider>
    );
  }

  const passed = withoutComposed(rest, ["onKeyDown", "ref"]);

  return (
    <ToggleGroupContext.Provider value={state}>
      <div
        {...passed}
        aria-orientation={orientation}
        onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
          const group: $FlowFixMe = event.currentTarget;
          // Moves and nothing else. Pressing every button the arrows pass over
          // is what a `single` group does, and doing it here would apply half a
          // dozen commands on the way to the one the reader wanted.
          moveOnKey(event, group, toggleSet(orientation));
        })}
        ref={composeRefs(rest.ref, (element) => {
          rootRef.current = element;
        })}
        role="group"
      >
        {children}
      </div>
    </ToggleGroupContext.Provider>
  );
}

/**
 * One button of the set.
 *
 * A disabled item is `aria-disabled` rather than `disabled`, the opposite of
 * the lone `Toggle`: an unavailable button *in a set* has to stay announced, or
 * a reader is told the set has two members when it has three and cannot ask
 * where the third went. The arrow keys step over it either way.
 */
export component ToggleGroupItem(
  value: string,
  children?: React.Node,
  disabled?: boolean = false,
  ...rest: Rest
) {
  const group = useToggleGroup("ToggleGroup.Item");
  // Before the branch, because it is a hook: which mode the set is in is not
  // allowed to change how many of them run.
  const id = useId();

  if (group.type === "single") {
    // A radio, whole. The `aria-checked` state, the roving tab stop and the
    // arrow keys that check as they move all belong to the radio group that
    // `ToggleGroup.Root` rendered around this.
    return (
      <RadioGroupItem {...forwarded(rest)} disabled={disabled} value={value}>
        {children}
      </RadioGroupItem>
    );
  }

  const on = group.pressed.includes(value);
  const passed = withoutComposed(rest, ["onClick", "onFocus", "onKeyDown"]);
  const setActiveId = group.setActiveId;

  return (
    <button
      {...passed}
      aria-disabled={disabled ? "true" : undefined}
      aria-pressed={on ? "true" : "false"}
      id={id}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          group.toggle(value);
        }
      })}
      // The roving tab stop follows real focus rather than leading it, so a
      // pointer that moves focus and an arrow key that moves focus agree
      // without the two having to be kept in step by hand.
      onFocus={composeHandlers(rest.onFocus, () => setActiveId(id))}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (disabled || (event.key !== " " && event.key !== "Enter")) {
          return;
        }
        // Stops `Space` scrolling the page, and stops the browser's own click
        // arriving afterwards and pressing this back to where it started.
        event.preventDefault();
        group.toggle(value);
      })}
      tabIndex={group.activeId === id || (group.activeId == null && group.firstId === id) ? 0 : -1}
      type="button"
    >
      {children}
    </button>
  );
}
