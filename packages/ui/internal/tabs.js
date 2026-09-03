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
  +register: (value: string, element: HTMLElement | null) => void,
  /** Focus the tab `pick` chooses, given where we are and how many there are. */
  +focusBy: (from: string, pick: (at: number, count: number) => number) => void,
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

  const register = useCallback((tab: string, element: HTMLElement | null) => {
    if (element == null) {
      order.current = order.current.filter((entry) => entry !== tab);
      delete elements.current[tab];
      return;
    }
    if (!order.current.includes(tab)) {
      order.current.push(tab);
    }
    elements.current[tab] = element;
  }, []);

  const focusAt = useCallback(
    (index: number) => {
      const tabs = order.current;
      if (tabs.length === 0) {
        return;
      }
      const wrapped = ((index % tabs.length) + tabs.length) % tabs.length;
      const tab = tabs[wrapped];
      select(tab);
      // Selection follows focus, which is the pattern for tabs whose panels
      // are already in the document: it means one key press per tab rather
      // than an arrow and then a space.
      elements.current[tab]?.focus();
    },
    [select],
  );

  const state = useMemo(
    () => ({
      base,
      selected,
      select,
      register,
      focusBy: (from: string, pick: (at: number, count: number) => number) => {
        focusAt(pick(order.current.indexOf(from), order.current.length));
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
    <div role="tablist" {...rest}>
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

  return (
    <button
      aria-controls={`${tabs.base}-panel-${value}`}
      aria-selected={active ? "true" : "false"}
      disabled={disabled}
      id={`${tabs.base}-tab-${value}`}
      onClick={() => {
        if (!disabled) {
          tabs.select(value);
        }
      }}
      onKeyDown={(event) => {
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
            "previous" => at - 1,
            "next" => at + 1,
            "first" => 0,
            "last" => count - 1,
          },
        );
      }}
      ref={(element) => tabs.register(value, element)}
      role="tab"
      // The roving tabindex: Tab reaches the selected tab and nothing else in
      // the list, so it moves past the whole set in one press.
      tabIndex={active ? 0 : -1}
      type="button"
      {...rest}
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
      aria-labelledby={`${tabs.base}-tab-${value}`}
      id={`${tabs.base}-panel-${value}`}
      role="tabpanel"
      // The panel itself is focusable so that Tab out of the tab list lands on
      // the content the tab describes, which is where the reader expects to go.
      tabIndex={0}
      {...rest}
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
