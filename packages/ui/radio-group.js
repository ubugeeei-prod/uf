// @flow
//
// A radio group: several answers, where choosing one unchooses the rest.
//
// The group is the control, not the buttons in it. That is the sentence the
// whole component follows from: a reader is told "Plan, radio group" and then
// "Pro, radio button, 2 of 3, selected", the *set* takes one stop in the page's
// tab order, and the arrow keys move inside it. Twelve hand-written radios take
// twelve Tab presses to get past and announce themselves as twelve unrelated
// buttons, which is a different widget wearing the same paint.
//
// # It is a tab list with automatic activation, under another name
//
// `tabs.js` already implements almost all of this: one tab stop, arrows that
// move, and `activationMode="automatic"` where moving to an item selects it. A
// radio group is that with `role="radio"` in place of `role="tab"` and no
// manual mode at all — arrows in a radio group *always* check, because that is
// what the pattern says, and a radio group whose arrows only moved focus would
// leave a reader believing they had answered when they had not.
//
// The one thing it needs that no other set here does is **the initial tab
// stop**. `Tabs.Tab` computes `tabIndex={active ? 0 : -1}` from the selection,
// which is right for both — but `Tabs` always has a selection, because
// `defaultValue` is required, and a radio group with nothing chosen is the
// state every unanswered form starts in. With the tab stop derived from the
// selection alone, nothing carries `tabindex="0"`, and an unanswered radio
// group is unreachable from the keyboard: not hard to reach, not awkward —
// absent. `internal/roving-focus.js`'s `useFirstItem` is the answer, and it is
// there rather than here because a toggle group nobody has focused yet has the
// same hole.
//
// # Which arrow keys, and a deviation stated rather than hidden
//
// The WAI-ARIA practices list both pairs for a radio group: `ArrowDown` and
// `ArrowRight` both move to the next radio. This package gates on `orientation`
// instead, as its tab list and its menu do, and the reason is the one
// `internal/roving-focus.js` gives about the keys a component does *not* claim:
// `ArrowDown` scrolls the page, and a horizontal row of three radios that
// swallows it has taken reading away from everyone who reads with the keyboard
// to buy a second way to do what `ArrowRight` already does. `aria-orientation`
// says which pair is live, so a reader is told rather than left to guess, and
// the default is `vertical` because that is how a radio group is nearly always
// laid out.
//
// # Naming the group
//
// A radio group with no name is announced as "radio group" and nothing else,
// which tells a reader that three answers exist and not what the question was.
// There is no `RadioGroup.Label` part because `Field` already is one:
//
//     <Field.Root>
//       <Field.Label>Plan</Field.Label>
//       <Field.Control
//         render={(props) => (
//           <RadioGroup.Root {...props} defaultValue="free">…</RadioGroup.Root>
//         )}
//       />
//     </Field.Root>
//
// `Field.Control` hands over `aria-labelledby` pointing at the label it
// rendered, which is the wiring `Field` exists to get right, and a second
// spelling of it here would be a second thing to keep in step.
//
// # Space, and the key that is not handled
//
// `Space` checks the focused radio, and prevents its default so the page does
// not scroll and the browser's own click does not arrive afterwards and check
// it a second time. `Enter` is not handled: it reaches these items as the
// browser's own click on a `<button>` and checks them, which is the least
// surprising thing for it to do. Claiming it in order to *stop* it would leave
// the key dead — `type="button"` cannot submit a form either — which is worse
// than the practices being quiet about it.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useCallback, useContext, useId, useMemo, useRef } from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import { moveOnKey, useFirstItem } from "./internal/roving-focus.js";
import type { Orientation, RovingSet } from "./internal/roving-focus.js";
import { useControlled } from "./internal/controlled-state.js";

/**
 * What the keyboard steps across in a radio group, and what owns one.
 *
 * By role rather than by a `data-*` attribute of this package's own, because
 * that is the promise the component makes to a reader: whatever produced a
 * `role="radio"` inside this group is one of the answers, and the arrow keys
 * have to reach it. `ToggleGroup type="single"` renders through here and is the
 * reason that matters in practice rather than in principle.
 */
export function radioSet(orientation: Orientation): RovingSet {
  return {
    item: '[role="radio"]',
    owner: '[role="radiogroup"]',
    orientation,
    wrap: true,
  };
}

type RadioGroupState = {|
  readonly selected: string | null,
  readonly select: (value: string) => void,
  /** The item holding the tab stop while nothing is chosen; see `useFirstItem`. */
  readonly firstId: string | null,
|};

const RadioGroupContext: React.Context<RadioGroupState | null> = createContext(null);

/** Whether the surrounding item is the chosen one, for `RadioGroup.Indicator`. */
type RadioItemState = {| readonly checked: boolean |};

const RadioItemContext: React.Context<RadioItemState | null> = createContext(null);

hook useRadioGroup(part: string): RadioGroupState {
  const state = useContext(RadioGroupContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a RadioGroup.Root`);
  }
  return state;
}

/**
 * The group, which is the control a reader is told about.
 *
 * `children` is `React.Node` rather than `renders* RadioGroupItem`, and the
 * reason is worth stating rather than leaving as an omission. `ToggleGroup
 * type="single"` is this component wearing segments: it renders a
 * `RadioGroup.Root` and passes its own items through. A `renders*` constraint
 * is a promise about the element a child produces, and a `ToggleGroup.Item`
 * cannot make it — it produces a radio in one mode and a toggle button in the
 * other — while naming both kinds here would make this module import the module
 * that imports it. `Tabs.List` keeps the tighter promise because nothing else
 * in this package renders a `tab`.
 *
 * `name` puts the answer where a form can submit it; see the hidden input
 * below.
 */
export component RadioGroupRoot(
  children: React.Node,
  defaultValue?: string | null = null,
  value?: string | null,
  onValueChange?: (value: string) => void,
  orientation?: Orientation = "vertical",
  name?: string,
  ...rest: Rest
) {
  // `onValueChange` promises a `string` while the group's *state* is
  // `string | null`, and the two meet here rather than being flattened into one
  // type that lies in one direction or the other: "nothing chosen yet" is a
  // state a radio group starts in, and it is not an event it can ever report,
  // because no gesture inside a radio group unchooses an answer. Widening the
  // prop to `string | null` would also make every `(plan: string) => void` a
  // caller already has a type error at the call site.
  const report = useCallback(
    (next: string | null) => {
      if (next != null) {
        onValueChange?.(next);
      }
    },
    [onValueChange],
  );
  const [selected, select] = useControlled<string | null>(value, defaultValue, report);
  const rootRef = useRef<HTMLElement | null>(null);
  // Only while nothing is chosen. Once there is an answer it holds the tab
  // stop, and asking the document which item comes first is work with no reader.
  const firstId = useFirstItem(rootRef, radioSet(orientation), selected == null);

  const state = useMemo(() => ({ selected, select, firstId }), [selected, select, firstId]);
  const passed = withoutComposed(rest, ["onKeyDown", "ref"]);

  return (
    <RadioGroupContext.Provider value={state}>
      <div
        {...passed}
        // A reader is told which axis this runs along, and it is also what says
        // which pair of arrow keys is live.
        aria-orientation={orientation}
        onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
          const group: $FlowFixMe = event.currentTarget;
          const next = moveOnKey(event, group, radioSet(orientation));
          if (next != null) {
            // Checking in the same key press is not a shortcut, it is the
            // pattern: a radio group whose arrows moved focus without checking
            // leaves a reader believing they have answered when they have not.
            select(next.getAttribute("data-value") ?? "");
          }
        })}
        ref={composeRefs(rest.ref, (element) => {
          rootRef.current = element;
        })}
        role="radiogroup"
      >
        {children}
        {/*
          A form submits `<input>` elements, and none of the parts above is one.
          Without this the group is a control a reader can operate and a form
          cannot read, which is the same hole `Combobox` still has.

          `type="hidden"` rather than a visually hidden real radio, because the
          buttons above already carry the whole of the accessible semantics: a
          second set of native radios would be announced as a second set of
          answers, and hiding them from the accessibility tree to stop that
          leaves elements a form's own validation would then point its
          "please choose one" at.
        */}
        {name == null ? null : <input name={name} type="hidden" value={selected ?? ""} />}
      </div>
    </RadioGroupContext.Provider>
  );
}

/**
 * One answer.
 *
 * A disabled item is `aria-disabled` rather than `disabled`, so it stays in the
 * accessibility tree: a reader is told "Enterprise, radio button, dimmed, 3 of
 * 3" and learns that the answer exists and is unavailable, where a native
 * `disabled` leaves a gap they cannot ask about. The arrow keys step over it
 * either way, and so does the search for the item that holds the tab stop.
 */
export component RadioGroupItem(
  value: string,
  children?: React.Node,
  disabled?: boolean = false,
  ...rest: Rest
) {
  const group = useRadioGroup("RadioGroup.Item");
  const id = useId();
  const checked = group.selected === value;
  const item = useMemo(() => ({ checked }), [checked]);
  const passed = withoutComposed(rest, ["onClick", "onKeyDown"]);

  return (
    <RadioItemContext.Provider value={item}>
      <button
        {...passed}
        aria-checked={checked ? "true" : "false"}
        aria-disabled={disabled ? "true" : undefined}
        // Read by the group's key handler, which finds items in the document
        // rather than in a registry and so needs each one to carry its value.
        data-value={value}
        id={id}
        onClick={composeHandlers(rest.onClick, () => {
          if (!disabled) {
            group.select(value);
          }
        })}
        onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
          if (disabled || event.key !== " ") {
            return;
          }
          // Stops `Space` scrolling the page — which is what makes a
          // hand-written radio feel broken even when it works — and stops the
          // browser's own click arriving afterwards to check this again.
          event.preventDefault();
          group.select(value);
        })}
        role="radio"
        // The roving tab stop: the chosen answer, or the first one while there
        // is no answer, so `Tab` reaches the group in either state and leaves
        // it in one press.
        tabIndex={checked || (group.selected == null && group.firstId === id) ? 0 : -1}
        type="button"
      >
        {children}
      </button>
    </RadioItemContext.Provider>
  );
}

/**
 * The mark inside the chosen answer, rendered only while it is chosen.
 *
 * `aria-hidden` because the item it sits in already says `aria-checked`: a dot
 * that also announced itself would have a reader hear the answer's state twice,
 * once as a state and once as a stray element. It exists so a caller can style
 * a mark that appears and disappears without reaching for
 * `[aria-checked="true"] > *`, and so the "only while chosen" part is not
 * something each caller reimplements.
 */
export component RadioGroupIndicator(children?: React.Node, ...rest: Rest) {
  const item = useContext(RadioItemContext);
  if (item == null) {
    throw new Error("RadioGroup.Indicator must be rendered inside a RadioGroup.Item");
  }
  if (!item.checked) {
    return null;
  }
  return (
    <span {...rest} aria-hidden="true">
      {children}
    </span>
  );
}
