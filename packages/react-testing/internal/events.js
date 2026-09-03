// @flow
//
// Making things happen to the page.
//
// Two layers, because tests want two different things.
//
// `fireEvent` dispatches one event. It is the right tool when the test is
// about the handler: "clicking calls onSelect once".
//
// `userEvent` performs what a person did, which is almost never one event.
// Clicking a button is pointerdown, mousedown, focus, pointerup, mouseup and
// click; typing is a keydown, an input and a keyup per character, with the
// value updated in between. A component that listens for `mousedown` — a menu
// that closes on outside press, say — behaves correctly under a real click and
// not at all under a bare `click` event, and a test that only fires `click`
// would pass while the feature was broken.

import { actively } from "./render.js";

/** Event constructors by DOM event name, with the right interface for each. */
const EVENT_TYPES: { +[string]: string } = {
  click: "MouseEvent",
  dblclick: "MouseEvent",
  mousedown: "MouseEvent",
  mouseup: "MouseEvent",
  mouseover: "MouseEvent",
  mouseout: "MouseEvent",
  mouseenter: "MouseEvent",
  mouseleave: "MouseEvent",
  mousemove: "MouseEvent",
  contextmenu: "MouseEvent",
  keydown: "KeyboardEvent",
  keyup: "KeyboardEvent",
  keypress: "KeyboardEvent",
  focus: "FocusEvent",
  blur: "FocusEvent",
  focusin: "FocusEvent",
  focusout: "FocusEvent",
  input: "InputEvent",
  pointerdown: "PointerEvent",
  pointerup: "PointerEvent",
  pointermove: "PointerEvent",
};

/** Events that do not bubble, whatever else is said about them. */
const NON_BUBBLING = new Set(["focus", "blur", "mouseenter", "mouseleave"]);

function construct(name: string, init: { +[string]: mixed }): Event {
  const interfaceName = EVENT_TYPES[name] ?? "Event";
  const Constructor = (globalThis: any)[interfaceName] ?? globalThis.Event;
  const options = {
    bubbles: !NON_BUBBLING.has(name),
    cancelable: true,
    ...init,
  };
  try {
    return new Constructor(name, options);
  } catch {
    // A host whose constructor is stricter than the init we were handed.
    return new globalThis.Event(name, options);
  }
}

/**
 * Dispatch one event, inside `act`.
 *
 * Returns whether the event ran to completion — `false` when a handler called
 * `preventDefault`, which is what `dispatchEvent` reports and what a test
 * asserting "the form did not submit" needs.
 */
export function dispatch(
  target: EventTarget,
  name: string,
  init?: { +[string]: mixed },
): boolean {
  const event = construct(name, init ?? {});
  let ran = true;
  actively(() => {
    ran = target.dispatchEvent(event);
  });
  return ran;
}

/**
 * `fireEvent.click(element)`, and one entry per event name.
 *
 * A proxy rather than a written-out table: the set of DOM events is long,
 * grows, and every entry would be the same line. `fireEvent(target, name)`
 * also works, for an event whose name is computed.
 */
export const fireEvent: any = new Proxy(
  (target: EventTarget, name: string, init?: { +[string]: mixed }) =>
    dispatch(target, name, init),
  {
    get(base, property) {
      if (typeof property !== "string") {
        return (base: any)[property];
      }
      if (property in base) {
        return (base: any)[property];
      }
      return (target: EventTarget, init?: { +[string]: mixed }) =>
        dispatch(target, property.toLowerCase(), init);
    },
  },
);

/** Set a control's value the way a browser does, so React sees the change. */
function setValue(element: HTMLElement, value: string): void {
  const target: any = element;
  // React tracks the last value it wrote on the node and skips an `input`
  // event whose value it believes it already knows. Writing through the
  // prototype's setter is what a browser does and what clears that.
  const prototype = Object.getPrototypeOf(target);
  const descriptor = Object.getOwnPropertyDescriptor(prototype, "value");
  if (descriptor?.set != null) {
    descriptor.set.call(target, value);
  } else {
    target.value = value;
  }
}

/** A key's `key`, `code` and printable text. */
function describeKey(key: string): {| key: string, code: string, text: string | null |} {
  const named: { +[string]: {| code: string, text: string | null |} } = {
    Enter: { code: "Enter", text: "\n" },
    Tab: { code: "Tab", text: null },
    Escape: { code: "Escape", text: null },
    Backspace: { code: "Backspace", text: null },
    Delete: { code: "Delete", text: null },
    ArrowUp: { code: "ArrowUp", text: null },
    ArrowDown: { code: "ArrowDown", text: null },
    ArrowLeft: { code: "ArrowLeft", text: null },
    ArrowRight: { code: "ArrowRight", text: null },
    Home: { code: "Home", text: null },
    End: { code: "End", text: null },
    " ": { code: "Space", text: " " },
  };
  const entry = named[key];
  if (entry != null) {
    return { key, code: entry.code, text: entry.text };
  }
  return { key, code: `Key${key.toUpperCase()}`, text: key };
}

/** Elements the tab order includes, in document order. */
function tabbable(): Array<HTMLElement> {
  const selector =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
  return Array.from(globalThis.document.querySelectorAll(selector)).filter(
    (element: any) => element.getAttribute("aria-hidden") !== "true",
  );
}

/**
 * What a person did, rather than what the DOM emitted.
 *
 * Every method is async because that is what makes a test written with it
 * correct as it grows: the moment an interaction leads to something awaited —
 * a fetch, a transition, a lazily loaded panel — a synchronous helper would
 * return before the result existed, and the test would need a sleep. Awaiting
 * from the start means adding that behaviour later changes nothing.
 */
export const userEvent = {
  /** Press and release, with the events a real click produces, in order. */
  async click(element: HTMLElement, init?: { +[string]: mixed }): Promise<void> {
    if ((element: any).disabled === true) {
      return;
    }
    dispatch(element, "pointerdown", init);
    dispatch(element, "mousedown", init);
    focus(element);
    dispatch(element, "pointerup", init);
    dispatch(element, "mouseup", init);
    dispatch(element, "click", init);
    await settle();
  },

  /** Two clicks and a dblclick. */
  async dblClick(element: HTMLElement): Promise<void> {
    await userEvent.click(element);
    await userEvent.click(element);
    dispatch(element, "dblclick");
    await settle();
  },

  /**
   * Type into a control, one character at a time.
   *
   * Per character rather than setting the value once, because a component
   * that reacts to each keystroke — a search box that filters, a field that
   * rejects a character — behaves differently, and the difference is the thing
   * usually being tested.
   */
  async type(element: HTMLElement, text: string): Promise<void> {
    focus(element);
    for (const character of text) {
      const { key, code, text: printable } = describeKey(character);
      dispatch(element, "keydown", { key, code });
      if (printable != null && printable !== "\n") {
        setValue(element, `${(element: any).value ?? ""}${printable}`);
        dispatch(element, "input", { data: printable });
      }
      dispatch(element, "keyup", { key, code });
    }
    await settle();
  },

  /** Empty a control, the way selecting everything and deleting would. */
  async clear(element: HTMLElement): Promise<void> {
    focus(element);
    setValue(element, "");
    dispatch(element, "input", {});
    await settle();
  },

  /** Press keys at whatever has focus. Named keys go in braces: `{Enter}`. */
  async keyboard(sequence: string): Promise<void> {
    const target: any = globalThis.document.activeElement ?? globalThis.document.body;
    for (const token of parseKeys(sequence)) {
      const { key, code, text } = describeKey(token);
      dispatch(target, "keydown", { key, code });
      if (text != null && text !== "\n" && target.value !== undefined) {
        setValue(target, `${target.value ?? ""}${text}`);
        dispatch(target, "input", { data: text });
      }
      dispatch(target, "keyup", { key, code });
    }
    await settle();
  },

  /** Move focus the way the Tab key does. */
  async tab(options?: {| +shift?: boolean |}): Promise<void> {
    const order = tabbable();
    if (order.length === 0) {
      return;
    }
    const active: any = globalThis.document.activeElement;
    const at = order.indexOf(active);
    const shift = options?.shift ?? false;
    const next =
      at < 0
        ? shift
          ? order[order.length - 1]
          : order[0]
        : order[(at + (shift ? -1 : 1) + order.length) % order.length];
    focus(next);
    await settle();
  },

  /** Choose options in a select. */
  async selectOptions(
    element: HTMLElement,
    values: string | $ReadOnlyArray<string>,
  ): Promise<void> {
    const wanted = typeof values === "string" ? [values] : values;
    const select: any = element;
    for (const option of Array.from(select.options ?? [])) {
      (option: any).selected = wanted.includes((option: any).value);
    }
    dispatch(element, "input");
    dispatch(element, "change");
    await settle();
  },

  /** Move focus away, which is what makes a blur-validated field validate. */
  async tabAway(element: HTMLElement): Promise<void> {
    dispatch(element, "blur");
    (element: any).blur?.();
    await settle();
  },
};

function focus(element: HTMLElement): void {
  const previous: any = globalThis.document.activeElement;
  if (previous === element) {
    return;
  }
  actively(() => {
    (element: any).focus?.();
  });
  if (globalThis.document.activeElement !== element) {
    // A host whose `focus` does not move `activeElement`; the events are what
    // components listen for, so dispatch them regardless.
    dispatch(element, "focus");
  }
}

/** `{Enter}` and `{Escape}` as single tokens; everything else per character. */
function parseKeys(sequence: string): Array<string> {
  const keys = [];
  let index = 0;
  while (index < sequence.length) {
    if (sequence[index] === "{") {
      const close = sequence.indexOf("}", index);
      if (close > index) {
        keys.push(sequence.slice(index + 1, close));
        index = close + 1;
        continue;
      }
    }
    keys.push(sequence[index]);
    index += 1;
  }
  return keys;
}

/** Let React finish anything the interaction started. */
async function settle(): Promise<void> {
  await actively(() => Promise.resolve());
}
