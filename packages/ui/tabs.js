// @flow
//
// Tabs, with the keyboard behaviour the pattern requires.
//
// A tab list is not a row of buttons. Only one tab is in the page's tab order —
// Tab moves *into* and *out of* the list, and the arrow keys move between the
// tabs inside it — because a list of twelve tabs that each take a Tab press
// makes everything after it unreachable for anyone not using a mouse. That is a
// roving `tabindex`, and it is the thing hand-written tabs almost always leave
// out.
//
// # Automatic and manual activation
//
// The second thing they leave out is the choice between them, and it is not a
// preference: it is about what a panel costs to show.
//
//   * **Automatic** — the default. Moving to a tab selects it, so reaching a
//     panel is one key press. This is what the pattern prescribes when the
//     panels are already in the document and showing one is free.
//   * **Manual** — arrow keys move focus and select nothing until `Enter` or
//     `Space`. This is what a panel that fetches, or that takes real work to
//     render, needs: with automatic activation a reader arrowing from the first
//     tab to the fourth starts three loads they did not ask for, and a screen
//     reader announces three panels they never wanted to hear about.
//
// # Which arrow keys
//
// `orientation` decides, and the keys it does *not* claim matter as much as the
// ones it does: `ArrowDown` in a horizontal tab list belongs to the page, and a
// component that swallows it has taken scrolling away from every reader who
// uses the keyboard to read.
//
// # Composition is type-checked
//
// `Tabs.List` takes `renders* TabsTab`, so putting a `<button>` in the list is
// a *type error* rather than a screen reader announcing "button" where the
// reader expected "tab, 2 of 5". A library written in TypeScript can document
// that constraint; Flow can state it.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useState,
} from "@uniflowed/react";

import { composeHandlers, withoutComposed } from "./internal/merge-props.js";
import { indexOfActive, itemsOf, movementFor, moveTo } from "./internal/roving-focus.js";
import { useControlled } from "./internal/controlled-state.js";
import type { Orientation } from "./internal/roving-focus.js";

/** When a tab becomes the selected one. */
export type ActivationMode = "automatic" | "manual";

type TabsState = {|
  readonly base: string,
  readonly selected: string,
  readonly select: (value: string) => void,
  readonly orientation: Orientation,
  readonly activation: ActivationMode,
  /** The panel values currently mounted, so a tab only claims one that exists. */
  readonly mounted: $ReadOnlyArray<string>,
  readonly registerPanel: (value: string, present: boolean) => void,
|};

const TabsContext: React.Context<TabsState | null> = createContext(null);

hook useTabs(part: string): TabsState {
  const state = useContext(TabsContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Tabs.Root`);
  }
  return state;
}

/**
 * The tab set.
 *
 * Uncontrolled by default and controlled when `value` is given, which is the
 * distinction every one of these components needs: a form library owns the
 * value, and a page that just wants tabs does not.
 */
export component TabsRoot(
  children: React.Node,
  defaultValue: string,
  value?: string,
  onValueChange?: (value: string) => void,
  activationMode?: ActivationMode = "automatic",
  orientation?: Orientation = "horizontal",
  ...rest: { readonly [string]: mixed }
) {
  const base = useId();
  const [selected, select] = useControlled(value, defaultValue, onValueChange);
  const [mounted, setMounted] = useState<$ReadOnlyArray<string>>([]);

  // Functional updates, so two panels mounting in the same commit do not each
  // overwrite the other's registration with a list computed before it existed.
  const registerPanel = useCallback((panel: string, present: boolean) => {
    setMounted((current) => {
      const has = current.includes(panel);
      if (present === has) {
        return current;
      }
      return present ? [...current, panel] : current.filter((each) => each !== panel);
    });
  }, []);

  const state = useMemo(
    () => ({
      base,
      selected,
      select,
      orientation,
      activation: activationMode,
      mounted,
      registerPanel,
    }),
    [base, selected, select, orientation, activationMode, mounted, registerPanel],
  );

  return (
    <TabsContext.Provider value={state}>
      <div {...rest}>{children}</div>
    </TabsContext.Provider>
  );
}

/**
 * The row of tabs, and the one place the arrow keys are handled.
 *
 * The handler is here rather than on each tab because the keys are about the
 * *set*: "the next tab" is a question only the list can answer, and answering it
 * from the DOM at the moment of the press means a tab added, removed or
 * reordered since the last render is still in the right place. A registry the
 * tabs push themselves into as they mount answers with mount order, which stops
 * being document order the first time a tab is conditional.
 */
export component TabsList(children: renders* TabsTab, ...rest: { readonly [string]: mixed }) {
  const tabs = useTabs("Tabs.List");
  const passed = withoutComposed(rest, ["onKeyDown"]);

  return (
    <div
      {...passed}
      // A screen reader announces the axis, and it is also what tells a reader
      // which arrow keys to try.
      aria-orientation={tabs.orientation}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        const list: $FlowFixMe = event.currentTarget;
        const movement = movementFor(event.key, tabs.orientation);
        if (movement == null) {
          return;
        }
        const items = itemsOf(list, '[role="tab"]', '[role="tablist"]');
        const active = list.ownerDocument?.activeElement;
        const next = moveTo(items, indexOfActive(items, active), movement, true);
        if (next == null) {
          return;
        }
        // Before moving, or the arrow also scrolls the page under the tab that
        // just took focus.
        event.preventDefault();
        next.focus();
        if (tabs.activation === "automatic") {
          tabs.select(next.getAttribute("data-value") ?? "");
        }
      })}
      role="tablist"
    >
      {children}
    </div>
  );
}

/**
 * One tab. Exactly one of them is in the page's tab order.
 *
 * A disabled tab is `aria-disabled` rather than `disabled`, so it stays in the
 * accessibility tree: a reader is told "Billing, tab, dimmed, 3 of 5" and knows
 * the section exists and is unavailable, where a native `disabled` would leave a
 * gap they cannot ask about. The keyboard steps over it either way.
 */
export component TabsTab(
  value: string,
  children: React.Node,
  disabled?: boolean = false,
  ...rest: { readonly [string]: mixed }
) {
  const tabs = useTabs("Tabs.Tab");
  const active = tabs.selected === value;
  const passed = withoutComposed(rest, ["onClick", "onKeyDown"]);

  return (
    <button
      // `passed` first, and everything this component owns after it. A caller
      // `onClick` used to replace the selection handler, so clicking a tab did
      // nothing at all.
      {...passed}
      aria-disabled={disabled ? "true" : undefined}
      // Only when the panel is actually mounted. Panels are rendered on demand,
      // and a tab pointing `aria-controls` at an id that is not in the document
      // tells a reader there is somewhere to go and then has nowhere to send
      // them.
      aria-controls={tabs.mounted.includes(value) ? `${tabs.base}-panel-${value}` : undefined}
      aria-selected={active ? "true" : "false"}
      // Read by the list's key handler, which finds tabs in the document rather
      // than in a registry and so needs each one to carry its own value.
      data-value={value}
      id={`${tabs.base}-tab-${value}`}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          tabs.select(value);
        }
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        // Manual activation's other half: the arrows moved focus here without
        // selecting, and this is how the reader says they meant it.
        if (event.key !== "Enter" && event.key !== " ") {
          return;
        }
        event.preventDefault();
        if (!disabled) {
          tabs.select(value);
        }
      })}
      role="tab"
      // The roving tabindex: Tab reaches the selected tab and nothing else in
      // the list, so it moves past the whole set in one press.
      tabIndex={active ? 0 : -1}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The panel a tab controls, rendered only while its tab is selected.
 *
 * It registers itself with the root while it is mounted, which is what lets
 * `Tabs.Tab` decide whether it has a panel to name. That has to be a real
 * subscription rather than "the selected value equals mine", because a caller
 * may render a subset of panels, or none at all until data arrives.
 */
export component TabsPanel(
  value: string,
  children: React.Node,
  ...rest: { readonly [string]: mixed }
) {
  const tabs = useTabs("Tabs.Panel");
  const register = tabs.registerPanel;
  const selected = tabs.selected === value;

  useEffect(() => {
    if (!selected) {
      return;
    }
    register(value, true);
    return () => register(value, false);
  }, [register, value, selected]);

  if (!selected) {
    return null;
  }

  return (
    <div
      {...rest}
      aria-labelledby={`${tabs.base}-tab-${value}`}
      id={`${tabs.base}-panel-${value}`}
      role="tabpanel"
      // The panel itself is focusable so that Tab out of the tab list lands on
      // the content the tab describes, which is where the reader expects to go
      // and where a panel of plain prose has nothing else to offer.
      tabIndex={0}
    >
      {children}
    </div>
  );
}
