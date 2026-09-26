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
// # Where the selected tab is, for an indicator that slides
//
// An underline that moves from one tab to the next, rather than one that goes
// out under the old tab and comes on under the new, needs one number no
// stylesheet can work out: where the selected tab is inside the list. So
// `Tabs.List` measures it and writes four custom properties on itself:
//
//   --uf-tabs-indicator-left     --uf-tabs-indicator-top
//   --uf-tabs-indicator-width    --uf-tabs-indicator-height
//
// The selected tab's box, relative to the list's padding box — the box an
// absolutely positioned child of the list is placed in — in CSS pixels. They
// are **numbers without a unit**, which is the one form a stylesheet can use
// both ways: `calc(var(--uf-tabs-indicator-left) * 1px)` is a length, and
// `scaleX(var(--uf-tabs-indicator-width))` stretches a one-pixel bar to the
// tab's width, which moves the indicator with `transform` alone and lays
// nothing out while it travels. They are measured again whenever the list
// renders and whenever the list or a tab changes size, and removed when no tab
// is selected. As everywhere in this package, the numbers are the component's
// and the look is the stylesheet's: nothing is drawn here.
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
  useRef,
  useState,
} from "@uniflowed/react";

import type { PartEvent, RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";
import { itemsOf, moveOnKey } from "./internal/roving-focus.js";
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
component TabsRoot(
  children: React.Node,
  defaultValue: string,
  value?: string,
  onValueChange?: (value: string) => void,
  activationMode?: ActivationMode = "automatic",
  orientation?: Orientation = "horizontal",
  render?: RenderProp,
  ...rest: Rest
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

  const props = withProps(rest, { children });

  return (
    <TabsContext.Provider value={state}>
      {render == null ? <div {...props} /> : render(props)}
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
component TabsList(children: renders* TabsTab, render?: RenderProp, ...rest: Rest) {
  const tabs = useTabs("Tabs.List");
  const listRef = useRef<HTMLElement | null>(null);
  const selected = tabs.selected;

  // Every render, with no dependency list, for the reason
  // `internal/disclosure.js` gives for its height: the tabs a caller renders
  // can change on any render, and what the indicator follows is where the
  // selected one ended up. Every write is compared first, so a render that
  // moved nothing writes nothing.
  useEffect(() => {
    const list = listRef.current;
    if (list != null) {
      placeIndicator(list);
    }
  });

  // A tab can move without anything here rendering: a web font arriving, a
  // label a sibling component translated, the window narrowing a list that
  // wraps. Keyed on `selected` so the observer is watching the tabs there are
  // now, which is when a new one may have arrived.
  useEffect(() => {
    const list = listRef.current;
    const view = list?.ownerDocument?.defaultView;
    if (list == null || view == null) {
      return;
    }
    // Read off the window, for the reason `internal/anchor.js` gives.
    const host: $FlowFixMe = view;
    if (typeof host.ResizeObserver !== "function") {
      return;
    }
    const sizes = new host.ResizeObserver(() => placeIndicator(list));
    sizes.observe(list);
    for (const tab of itemsOf(list, TAB, TAB_LIST)) {
      sizes.observe(tab);
    }
    return () => sizes.disconnect();
  }, [selected]);

  const props = withProps(withoutComposed(rest, ["onKeyDown", "ref"]), {
    // A screen reader announces the axis, and it is also what tells a reader
    // which arrow keys to try.
    "aria-orientation": tabs.orientation,
    children,
    onKeyDown: composeHandlers(rest.onKeyDown, (event: PartEvent) => {
      const list: $FlowFixMe = event.currentTarget;
      // The list the key arrived on carries the answer to both halves of
      // this: which items there are, and which way the page reads — so a tab
      // set inside somebody else's `dir="rtl"` walks the right way without
      // the caller having had to know it needed to say so.
      const next = moveOnKey(event, list, {
        item: TAB,
        owner: TAB_LIST,
        orientation: tabs.orientation,
        wrap: true,
        skipDisabled: true,
      });
      if (next != null && tabs.activation === "automatic") {
        tabs.select(next.getAttribute("data-value") ?? "");
      }
    }),
    // React calls callback refs during commit; the indicator effects read it later.
    // uf-lint-disable-next-line react-compiler/refs
    ref: composeRefs(rest.ref, (element: HTMLElement | null) => {
      listRef.current = element;
    }),
    role: "tablist",
  });

  if (render != null) {
    return render(props);
  }
  return <div {...props} />;
}

/** A tab, and the list that owns it, as the arrow keys and the indicator find them. */
const TAB = '[role="tab"]';
const TAB_LIST = '[role="tablist"]';

/** The four properties the module header describes, in the order they are written. */
const INDICATOR = [
  "--uf-tabs-indicator-left",
  "--uf-tabs-indicator-top",
  "--uf-tabs-indicator-width",
  "--uf-tabs-indicator-height",
];

/**
 * Write the selected tab's box on `list`, or take it away when none is.
 *
 * Measured from the two bounding boxes rather than from `offsetLeft`, which is
 * relative to the nearest *positioned* ancestor — the list only if a
 * stylesheet happened to position it. The list's border is taken off and its
 * scroll added back, so the numbers are in the padding box an absolutely
 * positioned indicator is placed in, and a tab scrolled into view in a long
 * list is still underlined where it is.
 */
function placeIndicator(list: HTMLElement): void {
  const tab = itemsOf(list, TAB, TAB_LIST).find(
    (each) => each.getAttribute("aria-selected") === "true",
  );
  const style = list.style;
  if (tab == null) {
    for (const name of INDICATOR) {
      style.removeProperty(name);
    }
    return;
  }
  const outer = list.getBoundingClientRect();
  const inner = tab.getBoundingClientRect();
  const values = [
    inner.left - outer.left - list.clientLeft + list.scrollLeft,
    inner.top - outer.top - list.clientTop + list.scrollTop,
    inner.width,
    inner.height,
  ];
  INDICATOR.forEach((name, index) => {
    // Two decimal places: enough for a device pixel at any zoom, and a number
    // that does not change in its fifteenth digit between two renders.
    const value = String(Math.round(values[index] * 100) / 100);
    if (style.getPropertyValue(name) !== value) {
      style.setProperty(name, value);
    }
  });
}

/**
 * One tab. Exactly one of them is in the page's tab order.
 *
 * A disabled tab is `aria-disabled` rather than `disabled`, so it stays in the
 * accessibility tree: a reader is told "Billing, tab, dimmed, 3 of 5" and knows
 * the section exists and is unavailable, where a native `disabled` would leave a
 * gap they cannot ask about. The keyboard steps over it either way.
 */
component TabsTab(
  value: string,
  children: React.Node,
  disabled?: boolean = false,
  render?: RenderProp,
  ...rest: Rest
) {
  const tabs = useTabs("Tabs.Tab");
  const active = tabs.selected === value;
  // The caller's props first, and everything this component owns after them. A
  // caller `onClick` used to replace the selection handler, so clicking a tab
  // did nothing at all. `render` is handed this same object in this same
  // order, which is why a tab rendered as somebody else's element is not a
  // weaker tab.
  const props = withProps(withoutComposed(rest, ["onClick", "onKeyDown"]), {
    "aria-disabled": disabled ? "true" : undefined,
    // Only when the panel is actually mounted. Panels are rendered on demand,
    // and a tab pointing `aria-controls` at an id that is not in the document
    // tells a reader there is somewhere to go and then has nowhere to send
    // them.
    "aria-controls": tabs.mounted.includes(value) ? `${tabs.base}-panel-${value}` : undefined,
    "aria-selected": active ? "true" : "false",
    children,
    // Read by the list's key handler, which finds tabs in the document rather
    // than in a registry and so needs each one to carry its own value.
    "data-value": value,
    id: `${tabs.base}-tab-${value}`,
    onClick: composeHandlers(rest.onClick, () => {
      if (!disabled) {
        tabs.select(value);
      }
    }),
    onKeyDown: composeHandlers(rest.onKeyDown, (event: PartEvent) => {
      // Manual activation's other half: the arrows moved focus here without
      // selecting, and this is how the reader says they meant it.
      if (event.key !== "Enter" && event.key !== " ") {
        return;
      }
      event.preventDefault();
      if (!disabled) {
        tabs.select(value);
      }
    }),
    role: "tab",
    // The roving tabindex: Tab reaches the selected tab and nothing else in
    // the list, so it moves past the whole set in one press.
    tabIndex: active ? 0 : -1,
  });

  if (render != null) {
    return render(props);
  }
  return <button {...props} type="button" />;
}

/**
 * The panel a tab controls, rendered only while its tab is selected.
 *
 * It registers itself with the root while it is mounted, which is what lets
 * `Tabs.Tab` decide whether it has a panel to name. That has to be a real
 * subscription rather than "the selected value equals mine", because a caller
 * may render a subset of panels, or none at all until data arrives.
 */
component TabsPanel(value: string, children: React.Node, render?: RenderProp, ...rest: Rest) {
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

  const props = withProps(rest, {
    "aria-labelledby": `${tabs.base}-tab-${value}`,
    children,
    id: `${tabs.base}-panel-${value}`,
    role: "tabpanel",
    // The panel itself is focusable so that Tab out of the tab list lands on
    // the content the tab describes, which is where the reader expects to go
    // and where a panel of plain prose has nothing else to offer.
    tabIndex: 0,
  });

  if (render != null) {
    return render(props);
  }
  return <div {...props} />;
}

/**
 * The parts, under the names the `Tabs` namespace gives them.
 *
 * `index.js` re-exports this module whole — `export * as Tabs from "./tabs.js"` —
 * so a caller writes `<Tabs.Root>`, and the namespace is the prefix. Each
 * part is still *declared* as `TabsRoot`, so React DevTools, a component
 * stack and an error name the part a reader can find rather than one of forty
 * `Root`s.
 */
export { TabsRoot as Root, TabsList as List, TabsTab as Tab, TabsPanel as Panel };
