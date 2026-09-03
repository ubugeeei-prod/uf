// @flow
//
// Tabs, with the keyboard behaviour the pattern requires.
//
// A tab list is not a row of buttons. Only one tab is in the page's tab order —
// Tab moves *into* and *out of* the list, and the arrow keys move between the
// tabs inside it — because a list of twelve tabs that each take a Tab press
// makes the rest of the page unreachable for anyone not using a mouse. That is
// a roving `tabindex`, and it is the thing hand-written tabs almost always
// leave out.
//
// This is also where Flow says something no other type system can. `Tabs.List`
// takes `renders* TabsTab`, so putting a `<button>` in the list is a type
// error rather than a screen reader announcing "button" where the user expects
// "tab, 2 of 5".

import * as React from "@uniflowed/react";

import { composeHandlers, composeRefs, withoutComposed } from "./props.js";
import {
  createContext,
  useCallback,
  useContext,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";

type TabsState = {|
  +base: string,
  +selected: string,
  +select: (value: string) => void,
  +register: (value: string, element: HTMLElement | null, disabled: boolean) => void,
  /**
   * Focus the tab `pick` chooses, given where we are and how many there are.
   *
   * `pick` returns the index to aim for and the direction to keep searching in
   * when that tab is disabled. The direction cannot be inferred from the
   * index: `End` aims at the last tab and, if it is disabled, has to walk
   * *backwards* to the last enabled one — inferring "forwards" from the target
   * being ahead of us wrapped around to the first tab instead.
   */
  +focusBy: (
    from: string,
    pick: (at: number, count: number) => [number, 1 | -1],
  ) => void,
|};

const TabsContext: React.Context<TabsState | null> = createContext(null);

function useTabs(part: string): TabsState {
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
  ...rest: { +[string]: mixed }
) {
  const base = useId();
  const [internal, setInternal] = useState(defaultValue);
  const selected = value ?? internal;
  // The order tabs were mounted in, which is document order, and is what the
  // arrow keys move through.
  const order = useRef<Array<string>>([]);
  const elements = useRef<{ [string]: HTMLElement }>({});

  const select = useCallback(
    (next: string) => {
      if (value == null) {
        setInternal(next);
      }
      onValueChange?.(next);
    },
    [value, onValueChange],
  );

  const disabledTabs = useRef<{ [string]: boolean }>({});

  const register = useCallback(
    (tab: string, element: HTMLElement | null, disabled: boolean) => {
      if (element == null) {
        order.current = order.current.filter((entry) => entry !== tab);
        delete elements.current[tab];
        delete disabledTabs.current[tab];
        return;
      }
      if (!order.current.includes(tab)) {
        order.current.push(tab);
      }
      elements.current[tab] = element;
      disabledTabs.current[tab] = disabled;
    },
    [],
  );

  /**
   * Focus and select the first enabled tab at or after `index`.
   *
   * Disabled tabs are stepped over rather than landed on. Selecting one meant
   * the panel changed to a tab that cannot take focus, so focus stayed where
   * it was and the next arrow press started from the wrong place — after which
   * the tabs beyond the disabled one were unreachable by keyboard.
   */
  const focusAt = useCallback(
    (index: number, step: number = 1) => {
      const tabs = order.current;
      if (tabs.length === 0) {
        return;
      }
      const wrap = (at: number) => ((at % tabs.length) + tabs.length) % tabs.length;
      const direction = step === 0 ? 1 : step;

      for (let tried = 0; tried < tabs.length; tried += 1) {
        const tab = tabs[wrap(index + tried * direction)];
        if (disabledTabs.current[tab] === true) {
          continue;
        }
        select(tab);
        // Selection follows focus, which is the pattern for tabs whose panels
        // are already in the document: one key press per tab rather than an
        // arrow and then a space.
        elements.current[tab]?.focus();
        return;
      }
      // Every tab is disabled, so there is nowhere to go.
    },
    [select],
  );

  const state = useMemo(
    () => ({
      base,
      selected,
      select,
      register,
      focusBy: (
        from: string,
        pick: (at: number, count: number) => [number, 1 | -1],
      ) => {
        const [target, direction] = pick(
          order.current.indexOf(from),
          order.current.length,
        );
        focusAt(target, direction);
      },
    }),
    [base, selected, select, register, focusAt],
  );

  return (
    <TabsContext.Provider value={state}>
      <div {...rest}>{children}</div>
    </TabsContext.Provider>
  );
}

/**
 * The row of tabs.
 *
 * `renders* TabsTab` is the constraint: the children have to be tabs. A
 * `<button>` here would be announced as a button inside a tablist, which is
 * how a keyboard user ends up unable to tell where they are.
 */
export component TabsList(children: renders* TabsTab, ...rest: { +[string]: mixed }) {
  return (
    <div {...rest} role="tablist">
      {children}
    </div>
  );
}

/** One tab. Exactly one of them is in the page's tab order. */
export component TabsTab(
  value: string,
  children: React.Node,
  disabled?: boolean = false,
  ...rest: { +[string]: mixed }
) {
  const tabs = useTabs("Tabs.Tab");
  const active = tabs.selected === value;

  const passed = withoutComposed(rest, ["onClick", "onKeyDown", "ref"]);

  return (
    <button
      // `passed` first, and everything this component owns after it. A caller
      // `ref` used to replace the registration ref, which took the tab out of
      // the keyboard order without any sign that it had.
      {...passed}
      aria-controls={`${tabs.base}-panel-${value}`}
      aria-selected={active ? "true" : "false"}
      disabled={disabled}
      id={`${tabs.base}-tab-${value}`}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          tabs.select(value);
        }
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        const intent = arrowKey(event.key);
        if (intent == null) {
          return;
        }
        // Prevent the default before moving, or the arrow also scrolls the
        // page under the tab that just took focus.
        event.preventDefault();
        // `match` is an expression, so it computes the index to move to rather
        // than performing the four movements — which also means adding a key
        // to `arrowKey` stops compiling here until it is handled.
        tabs.focusBy(value, (at, count) =>
          match (intent) {
            "previous" => [at - 1, -1],
            "next" => [at + 1, 1],
            "first" => [0, 1],
            "last" => [count - 1, -1],
          },
        );
      })}
      ref={composeRefs(rest.ref, (element) => tabs.register(value, element, disabled))}
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

/** The panel a tab controls, rendered only while its tab is selected. */
export component TabsPanel(
  value: string,
  children: React.Node,
  ...rest: { +[string]: mixed }
) {
  const tabs = useTabs("Tabs.Panel");
  if (tabs.selected !== value) {
    return null;
  }
  return (
    <div
      {...rest}
      aria-labelledby={`${tabs.base}-tab-${value}`}
      id={`${tabs.base}-panel-${value}`}
      role="tabpanel"
      // The panel itself is focusable so that Tab out of the tab list lands on
      // the content the tab describes, which is where the reader expects to go.
      tabIndex={0}
    >
      {children}
    </div>
  );
}

/** Which movement a key asks for, or nothing if the key is not ours. */
function arrowKey(key: string): "previous" | "next" | "first" | "last" | null {
  return match (key) {
    "ArrowLeft" | "ArrowUp" => "previous",
    "ArrowRight" | "ArrowDown" => "next",
    "Home" => "first",
    "End" => "last",
    _ => null,
  };
}
