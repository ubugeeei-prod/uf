// @flow
//
// `@uniflowed/ui`'s interactions: `interactions.js`.
//
// Every case drives one input the way its platform does — a mouse, a finger
// followed by the mouse events a tap is followed by, a key, and a screen
// reader's click with nothing pointing at it first — and asserts on what the
// component was told. A case that only fired `click` would pass for the
// `onClick` this module exists to replace, so the sequences are written out.
//
// happy-dom has no default actions: nothing clicks a button when `Enter` goes
// down on it, and nothing moves focus on `mousedown`. Where a case depends on
// the browser doing one of those, it does it by hand and says so.

import * as React from "@uniflowed/react";
import { afterEach, describe, expect, it, uft } from "@uniflowed/test";
import { act, fireEvent, render, screen } from "@uniflowed/react-testing";

import type { PressEvent } from "@uniflowed/ui";
import {
  getInteractionModality,
  mergeProps,
  useFocusRing,
  useFocusVisible,
  useHover,
  useInteractionModality,
  useKeyboard,
  useLongPress,
  useMove,
  usePress,
} from "@uniflowed/ui";
import { bodyOf } from "../../tests/library/dom.js";

/**
 * The element a query found, as the `HTMLElement` that has `focus()` and `style`.
 *
 * A query answers `Element`, because an SVG element can have a role too.
 */
function htmlOf(element: Element): HTMLElement {
  if (!(element instanceof HTMLElement)) {
    throw new Error(`expected an HTML element, found <${element.tagName.toLowerCase()}>`);
  }
  return element;
}

/** A mouse's pointer with its primary button down, as a browser reports one. */
const MOUSE = {
  button: 0,
  buttons: 1,
  height: 1,
  isPrimary: true,
  pointerId: 1,
  pointerType: "mouse",
  pressure: 0.5,
  width: 1,
};

/** The same mouse, released. */
const MOUSE_UP = { ...MOUSE, buttons: 0, pressure: 0 };

/** A finger on a touchscreen: a contact area, and its own pointer id. */
const TOUCH = {
  button: 0,
  buttons: 1,
  height: 22,
  isPrimary: true,
  pointerId: 7,
  pointerType: "touch",
  pressure: 0.5,
  width: 22,
};

/** The same finger, lifted. */
const TOUCH_UP = { ...TOUCH, buttons: 0, pressure: 0 };

/** A mouse pressing and releasing an element, with every event a browser sends. */
function mouseClick(element: Element, modifiers?: {| readonly shiftKey?: boolean |}): void {
  fireEvent.pointerDown(element, { ...MOUSE, ...modifiers });
  fireEvent.mouseDown(element, { button: 0, buttons: 1, detail: 1, ...modifiers });
  fireEvent.pointerUp(element, { ...MOUSE_UP, ...modifiers });
  fireEvent.mouseUp(element, { button: 0, detail: 1, ...modifiers });
  fireEvent.click(element, { button: 0, detail: 1, ...modifiers });
}

/**
 * A finger tapping an element, and what follows the tap.
 *
 * iOS sends a second `pointerenter`, typed `"mouse"`, once the finger lifts
 * (WebKit bug 214609), and every browser then sends the compatibility mouse
 * events and the click.
 */
function tap(element: Element): void {
  fireEvent.pointerEnter(element, TOUCH);
  fireEvent.pointerDown(element, TOUCH);
  fireEvent.pointerUp(element, TOUCH_UP);
  fireEvent.pointerLeave(element, TOUCH_UP);
  fireEvent.pointerEnter(element, MOUSE_UP);
  fireEvent.mouseDown(element, { button: 0, buttons: 1, detail: 1 });
  fireEvent.mouseUp(element, { button: 0, detail: 1 });
  fireEvent.click(element, { button: 0, detail: 1 });
}

/** A screen reader activating an element: a click, and nothing pointing at it first. */
function virtualClick(element: Element): boolean {
  return fireEvent.click(element, { detail: 0 });
}

/** What a component was told, as `type:pointerType`, in order, and the function that records it. */
function recorder(): {|
  readonly events: Array<string>,
  readonly record: (event: { readonly type: string, readonly pointerType: string, ... }) => void,
|} {
  const events: Array<string> = [];
  return {
    events,
    record: (event) => {
      events.push(`${event.type}:${event.pointerType}`);
    },
  };
}

/** Every press event, from the mouse, in the order a press sends them. */
const MOUSE_PRESS = ["pressstart:mouse", "pressup:mouse", "pressend:mouse", "press:mouse"];

/**
 * One pressable element of each kind a caller renders a part onto.
 *
 * `data-pressed` is the `isPressed` a stylesheet would draw from.
 */
component PressProbe(
  onEvent: (event: PressEvent) => void,
  allowTextSelectionOnPress?: boolean = false,
  isDisabled?: boolean = false,
  kind?: "button" | "checkbox" | "div" | "link" | "submit" = "div",
  preventFocusOnPress?: boolean = false,
  shouldCancelOnPointerExit?: boolean = false,
) {
  const { isPressed, pressProps } = usePress({
    allowTextSelectionOnPress,
    isDisabled,
    onPress: onEvent,
    onPressEnd: onEvent,
    onPressStart: onEvent,
    onPressUp: onEvent,
    preventFocusOnPress,
    shouldCancelOnPointerExit,
  });
  const pressed = isPressed ? "true" : undefined;
  if (kind === "button") {
    return (
      <button {...pressProps} data-pressed={pressed} type="button">
        Save
      </button>
    );
  }
  if (kind === "submit") {
    return (
      <button {...pressProps} data-pressed={pressed} type="submit">
        Save
      </button>
    );
  }
  if (kind === "link") {
    return (
      <a {...pressProps} data-pressed={pressed} href="#saved">
        Save
      </a>
    );
  }
  if (kind === "checkbox") {
    return (
      <div {...pressProps} aria-checked="false" data-pressed={pressed} role="checkbox" tabIndex={0}>
        Save
      </div>
    );
  }
  return (
    <div {...pressProps} data-pressed={pressed} role="button" tabIndex={0}>
      Save
    </div>
  );
}

describe("usePress, from a mouse", () => {
  it("presses once, and tells the component in the order it happened", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    mouseClick(screen.getByRole("button", { name: "Save" }));
    expect(events).toEqual(MOUSE_PRESS);
  });

  it("is pressed while the button is down, and only then", () => {
    const { record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    expect(control).toHaveAttribute("data-pressed", "true");

    fireEvent.pointerUp(control, MOUSE_UP);
    fireEvent.click(control, { button: 0, detail: 1 });
    expect(control).not.toHaveAttribute("data-pressed");
  });

  it("does not press when the pointer is let go of somewhere else", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    fireEvent.pointerLeave(control, MOUSE);
    // The browser clicks the nearest ancestor both ends of the press were in,
    // which is not this element, so no click reaches it.
    fireEvent.pointerUp(bodyOf(), MOUSE_UP);

    expect(events).toEqual(["pressstart:mouse", "pressend:mouse"]);
    expect(control).not.toHaveAttribute("data-pressed");
  });

  it("starts again when the pointer comes back while it is still down", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    fireEvent.pointerLeave(control, MOUSE);
    expect(control).not.toHaveAttribute("data-pressed");
    fireEvent.pointerEnter(control, MOUSE);
    expect(control).toHaveAttribute("data-pressed", "true");
    fireEvent.pointerUp(control, MOUSE_UP);
    fireEvent.click(control, { button: 0, detail: 1 });

    expect(events).toEqual(["pressstart:mouse", "pressend:mouse", ...MOUSE_PRESS]);
  });

  it("takes the press back for good once the pointer leaves, when asked to", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} shouldCancelOnPointerExit />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    fireEvent.pointerLeave(control, MOUSE);
    fireEvent.pointerEnter(control, MOUSE);
    fireEvent.pointerUp(control, MOUSE_UP);
    // Back over the element, so the browser clicks it — and the click is
    // refused, which also stops a link from being followed.
    expect(fireEvent.click(control, { button: 0, detail: 1 })).toBe(false);

    expect(events).toEqual(["pressstart:mouse", "pressend:mouse"]);
  });

  it("answers the primary button and no other", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    // A right click opens a context menu. It does not also press what it was on.
    fireEvent.pointerDown(control, { ...MOUSE, button: 2, buttons: 2 });
    fireEvent.pointerUp(control, { ...MOUSE_UP, button: 2 });
    fireEvent.contextMenu(control, { button: 2, detail: 1 });

    expect(events).toEqual([]);
    expect(control).not.toHaveAttribute("data-pressed");
  });

  it("ends a press the browser cancels, without pressing", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    fireEvent.pointerCancel(control, MOUSE);

    expect(events).toEqual(["pressstart:mouse", "pressend:mouse"]);
    expect(control).not.toHaveAttribute("data-pressed");
  });

  it("ends a press that became a native drag, which Safari does not cancel", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    fireEvent.dragStart(control);
    fireEvent.pointerUp(bodyOf(), MOUSE_UP);

    expect(events).toEqual(["pressstart:mouse", "pressend:mouse"]);
  });

  it("moves focus to what it pressed, which Safari does not do for a button", () => {
    const { record } = recorder();
    render(<PressProbe kind="button" onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);

    expect(control).toHaveFocus();
  });

  it("leaves focus where it is when asked to", () => {
    const { events, record } = recorder();
    render(
      <>
        <input aria-label="Search" />
        <PressProbe kind="button" onEvent={record} preventFocusOnPress />
      </>,
    );
    const field = screen.getByRole("textbox", { name: "Search" });
    act(() => {
      htmlOf(field).focus();
    });
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    // `mousedown`'s default action is what would move focus, so it is prevented.
    expect(fireEvent.mouseDown(control, { button: 0, buttons: 1, detail: 1 })).toBe(false);
    fireEvent.pointerUp(control, MOUSE_UP);
    fireEvent.click(control, { button: 0, detail: 1 });

    expect(field).toHaveFocus();
    expect(events).toEqual(MOUSE_PRESS);
  });

  it("turns text selection off while the pointer is down, and back on after", () => {
    const { record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    expect(htmlOf(control).style.getPropertyValue("user-select")).toBe("none");

    fireEvent.pointerUp(control, MOUSE_UP);
    expect(htmlOf(control).style.getPropertyValue("user-select")).toBe("");
  });

  it("leaves text selection alone when asked to", () => {
    const { record } = recorder();
    render(<PressProbe allowTextSelectionOnPress onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    expect(htmlOf(control).style.getPropertyValue("user-select")).toBe("");
    fireEvent.pointerUp(control, MOUSE_UP);
  });

  it("carries the modifier keys that were held", () => {
    const seen: Array<boolean> = [];
    render(
      <PressProbe
        onEvent={(event) => {
          if (event.type === "press") {
            seen.push(event.shiftKey);
          }
        }}
      />,
    );
    mouseClick(screen.getByRole("button", { name: "Save" }), { shiftKey: true });
    expect(seen).toEqual([true]);
  });
});

describe("usePress, from a finger", () => {
  it("presses once for a tap, and not again for the mouse events after it", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    tap(screen.getByRole("button", { name: "Save" }));
    expect(events).toEqual(["pressstart:touch", "pressup:touch", "pressend:touch", "press:touch"]);
  });

  it("does not press when the page scrolled instead", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    // A finger that starts to scroll is cancelled by the browser, and no click
    // follows it.
    fireEvent.pointerDown(control, TOUCH);
    fireEvent.pointerCancel(control, TOUCH);

    expect(events).toEqual(["pressstart:touch", "pressend:touch"]);
  });
});

describe("usePress, from a screen reader", () => {
  it("takes a click with nothing pointing at it for a whole press", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    virtualClick(screen.getByRole("button", { name: "Save" }));
    expect(events).toEqual([
      "pressstart:virtual",
      "pressup:virtual",
      "pressend:virtual",
      "press:virtual",
    ]);
  });

  it("recognises the pointer VoiceOver on iOS sends, which has no size", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, { ...TOUCH, height: 0, width: 0 });
    fireEvent.pointerUp(control, { ...TOUCH_UP, height: 0, width: 0 });
    virtualClick(control);

    expect(events).toEqual([
      "pressstart:virtual",
      "pressup:virtual",
      "pressend:virtual",
      "press:virtual",
    ]);
  });

  it("leaves a pointer's click alone when no press came before it", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    // A click that counts itself is a pointer's. With no press before it, the
    // pointer went down somewhere else.
    fireEvent.click(screen.getByRole("button", { name: "Save" }), { detail: 1 });
    expect(events).toEqual([]);
  });
});

/** Every press event, from the keyboard, in the order a press sends them. */
const KEY_PRESS = [
  "pressstart:keyboard",
  "pressup:keyboard",
  "pressend:keyboard",
  "press:keyboard",
];

describe("usePress, from the keyboard", () => {
  it("presses an element that is not a button on Enter, as the key goes down", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    // Claimed, so nothing else on the page acts on the same key.
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(false);
    expect(events).toEqual(KEY_PRESS);

    fireEvent.keyUp(control, { key: "Enter" });
    expect(events).toEqual(KEY_PRESS);
  });

  it("presses it on Space as the key comes up, the way a button does", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    // Claimed on the way down, or `Space` scrolls the page.
    expect(fireEvent.keyDown(control, { key: " " })).toBe(false);
    expect(events).toEqual(["pressstart:keyboard"]);
    expect(control).toHaveAttribute("data-pressed", "true");

    expect(fireEvent.keyUp(control, { key: " " })).toBe(false);
    expect(events).toEqual(KEY_PRESS);
    expect(control).not.toHaveAttribute("data-pressed");
  });

  it("presses once for a held key, however often the key repeats", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.keyDown(control, { key: " " });
    for (let repeat = 0; repeat < 3; repeat += 1) {
      expect(fireEvent.keyDown(control, { key: " ", repeat: true })).toBe(false);
    }
    fireEvent.keyUp(control, { key: " " });
    expect(events).toEqual(KEY_PRESS);

    fireEvent.keyDown(control, { key: "Enter" });
    fireEvent.keyDown(control, { key: "Enter", repeat: true });
    fireEvent.keyUp(control, { key: "Enter" });
    expect(events).toEqual([...KEY_PRESS, ...KEY_PRESS]);
  });

  it("ends a keyboard press in one click, so a click handler hears the keyboard too", () => {
    const { events, record } = recorder();
    const clicks: Array<string> = [];
    component Clickable() {
      const { pressProps } = usePress({
        onPress: record,
        onPressEnd: record,
        onPressStart: record,
        onPressUp: record,
      });
      return (
        <div
          {...mergeProps(pressProps, { onClick: () => clicks.push("click") })}
          role="button"
          tabIndex={0}
        >
          Save
        </div>
      );
    }
    render(<Clickable />);
    const control = screen.getByRole("button", { name: "Save" });

    // A `<div>` is never clicked by the browser for a key. The press is
    // completed by the click a button would have made — once, on key down for
    // `Enter` and on key up for `Space`, and not again for a held key.
    fireEvent.keyDown(control, { key: "Enter" });
    fireEvent.keyDown(control, { key: "Enter", repeat: true });
    fireEvent.keyUp(control, { key: "Enter" });
    expect(clicks).toEqual(["click"]);

    fireEvent.keyDown(control, { key: " " });
    expect(clicks).toEqual(["click"]);
    fireEvent.keyUp(control, { key: " " });
    expect(clicks).toEqual(["click", "click"]);

    expect(events).toEqual([...KEY_PRESS, ...KEY_PRESS]);
  });

  it("does not press when focus leaves before Space comes up", () => {
    const { events, record } = recorder();
    render(<PressProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.keyDown(control, { key: " " });
    fireEvent.blur(control);
    fireEvent.keyUp(bodyOf(), { key: " " });

    expect(events).toEqual(["pressstart:keyboard", "pressend:keyboard"]);
    expect(control).not.toHaveAttribute("data-pressed");
  });

  it("presses a button on its own keys, and claims them so the browser does not click it again", () => {
    const { events, record } = recorder();
    render(<PressProbe kind="button" onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    // A `type="button"` button's click does nothing but press, so its keys are
    // handled here and their default action — that click — is prevented.
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(false);
    expect(events).toEqual(KEY_PRESS);

    expect(fireEvent.keyDown(control, { key: " " })).toBe(false);
    expect(fireEvent.keyUp(control, { key: " " })).toBe(false);
    expect(events).toEqual([...KEY_PRESS, ...KEY_PRESS]);
  });

  it("leaves a submit button's Enter to the browser, which submits, and presses once", () => {
    const { events, record } = recorder();
    render(<PressProbe kind="submit" onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    // Not claimed: the click the browser sends for `Enter` is what submits the
    // form the button is in, and a press cannot do that instead.
    expect(fireEvent.keyDown(control, { key: "Enter" })).toBe(true);
    expect(events).toEqual(["pressstart:keyboard", "pressup:keyboard", "pressend:keyboard"]);

    // What the browser does next, and this DOM does not.
    fireEvent.click(control, { detail: 0 });
    expect(events).toEqual(KEY_PRESS);
  });

  it("presses a submit button on Space through the click the key makes as it comes up", () => {
    const { events, record } = recorder();
    render(<PressProbe kind="submit" onEvent={record} />);
    const control = screen.getByRole("button", { name: "Save" });

    expect(fireEvent.keyDown(control, { key: " " })).toBe(true);
    expect(fireEvent.keyUp(control, { key: " " })).toBe(true);
    expect(events).toEqual(["pressstart:keyboard", "pressup:keyboard", "pressend:keyboard"]);

    fireEvent.click(control, { detail: 0 });
    expect(events).toEqual(KEY_PRESS);
  });

  it("keeps a link's keyboard: Enter follows it, and Space scrolls the page", () => {
    const { events, record } = recorder();
    render(<PressProbe kind="link" onEvent={record} />);
    const link = screen.getByRole("link", { name: "Save" });

    expect(fireEvent.keyDown(link, { key: " " })).toBe(true);
    fireEvent.keyUp(link, { key: " " });
    expect(events).toEqual([]);

    expect(fireEvent.keyDown(link, { key: "Enter" })).toBe(true);
    fireEvent.click(link, { detail: 0 });
    expect(events).toEqual(KEY_PRESS);
  });

  it("keeps a checkbox's keyboard: Space presses it, and Enter is the form's", () => {
    const { events, record } = recorder();
    render(<PressProbe kind="checkbox" onEvent={record} />);
    const box = screen.getByRole("checkbox", { name: "Save" });

    expect(fireEvent.keyDown(box, { key: "Enter" })).toBe(true);
    expect(events).toEqual([]);

    fireEvent.keyDown(box, { key: " " });
    fireEvent.keyUp(box, { key: " " });
    expect(events).toEqual(KEY_PRESS);
  });

  it("leaves the keys of a field inside it to the field", () => {
    const { events, record } = recorder();
    component Card() {
      const { pressProps } = usePress({ onPress: record, onPressStart: record });
      return (
        <div {...pressProps} aria-label="Card" role="group">
          <input aria-label="Title" />
        </div>
      );
    }
    render(<Card />);
    const field = screen.getByRole("textbox", { name: "Title" });

    expect(fireEvent.keyDown(field, { key: "Enter" })).toBe(true);
    expect(fireEvent.keyDown(field, { key: " " })).toBe(true);
    fireEvent.keyUp(field, { key: " " });

    expect(events).toEqual([]);
  });
});

describe("usePress, disabled", () => {
  it("presses nothing, and stops the link it is rendered as", () => {
    const { events, record } = recorder();
    render(<PressProbe isDisabled kind="link" onEvent={record} />);
    const link = screen.getByRole("link", { name: "Save" });

    mouseClick(link);
    fireEvent.keyDown(link, { key: "Enter" });
    // A screen reader's click on a disabled link must not follow it either.
    expect(virtualClick(link)).toBe(false);

    expect(events).toEqual([]);
  });

  it("ends a press that is disabled in the middle of it", () => {
    const { events, record } = recorder();
    component Switchable() {
      const [disabled, setDisabled] = React.useState(false);
      return (
        <>
          <PressProbe isDisabled={disabled} onEvent={record} />
          <button onClick={() => setDisabled(true)} type="button">
            Disable
          </button>
        </>
      );
    }
    render(<Switchable />);
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerDown(control, MOUSE);
    fireEvent.click(screen.getByRole("button", { name: "Disable" }));

    expect(events).toEqual(["pressstart:mouse", "pressend:mouse"]);
    expect(control).not.toHaveAttribute("data-pressed");
  });
});

describe("usePress, one inside another", () => {
  component Nested(inner: (event: PressEvent) => void, outer: (event: PressEvent) => void) {
    const outerPress = usePress({ onPress: outer });
    const innerPress = usePress({
      onPress: inner,
      onPressEnd: inner,
      onPressStart: inner,
      onPressUp: inner,
    });
    return (
      <div {...outerPress.pressProps} aria-label="Card" role="button" tabIndex={0}>
        <button {...innerPress.pressProps} type="button">
          Delete
        </button>
      </div>
    );
  }

  it("belongs to the innermost one", () => {
    const inner = recorder();
    const outer = recorder();
    render(<Nested inner={inner.record} outer={outer.record} />);
    const remove = screen.getByRole("button", { name: "Delete" });

    mouseClick(remove);
    virtualClick(remove);

    expect(inner.events).toEqual([
      ...MOUSE_PRESS,
      "pressstart:virtual",
      "pressup:virtual",
      "pressend:virtual",
      "press:virtual",
    ]);
    expect(outer.events).toEqual([]);
  });

  it("reaches the outer one too when the inner one passes it on", () => {
    const inner = recorder();
    const outer = recorder();
    const passOn = (event: PressEvent) => {
      event.continuePropagation();
      inner.record(event);
    };
    render(<Nested inner={passOn} outer={outer.record} />);

    mouseClick(screen.getByRole("button", { name: "Delete" }));

    expect(inner.events).toEqual(MOUSE_PRESS);
    expect(outer.events).toEqual(["press:mouse"]);
  });
});

/** A button that reports its hover as `data-hovered`, and tells a recorder each change. */
component HoverProbe(
  onEvent: (event: { readonly type: string, readonly pointerType: string, ... }) => void,
  isDisabled?: boolean = false,
) {
  const { hoverProps, isHovered } = useHover({
    isDisabled,
    onHoverEnd: onEvent,
    onHoverStart: onEvent,
  });
  return (
    <button {...hoverProps} data-hovered={isHovered ? "true" : undefined} type="button">
      Preview
    </button>
  );
}

describe("useHover", () => {
  it("is a mouse's", () => {
    const { events, record } = recorder();
    render(<HoverProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Preview" });

    fireEvent.pointerEnter(control, MOUSE_UP);
    expect(control).toHaveAttribute("data-hovered", "true");
    fireEvent.pointerLeave(control, MOUSE_UP);

    expect(events).toEqual(["hoverstart:mouse", "hoverend:mouse"]);
    expect(control).not.toHaveAttribute("data-hovered");
  });

  it("is a pen's too", () => {
    const { events, record } = recorder();
    render(<HoverProbe onEvent={record} />);
    fireEvent.pointerEnter(screen.getByRole("button", { name: "Preview" }), {
      ...MOUSE_UP,
      pointerType: "pen",
    });
    expect(events).toEqual(["hoverstart:pen"]);
  });

  it("is never a finger's", () => {
    const { events, record } = recorder();
    render(<HoverProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Preview" });

    fireEvent.pointerEnter(control, TOUCH);
    fireEvent.pointerDown(control, TOUCH);

    expect(events).toEqual([]);
    expect(control).not.toHaveAttribute("data-hovered");
  });

  it("ignores the mouse pointer iOS sends after a tap, and hovers again once that has passed", () => {
    uft.useFakeTimers();
    const { events, record } = recorder();
    render(<HoverProbe onEvent={record} />);
    const control = screen.getByRole("button", { name: "Preview" });

    // The tap ends with a `pointerenter` typed "mouse". Believed, it would
    // start a hover no pointer is ever going to end.
    tap(control);
    expect(events).toEqual([]);
    expect(control).not.toHaveAttribute("data-hovered");

    act(() => {
      uft.advanceTimersByTime(499);
    });
    fireEvent.pointerEnter(control, MOUSE_UP);
    expect(events).toEqual([]);

    act(() => {
      uft.advanceTimersByTime(1);
    });
    fireEvent.pointerEnter(control, MOUSE_UP);
    expect(events).toEqual(["hoverstart:mouse"]);
  });

  it("ends a hover when it is disabled", () => {
    const { events, record } = recorder();
    component Switchable() {
      const [disabled, setDisabled] = React.useState(false);
      return (
        <>
          <HoverProbe isDisabled={disabled} onEvent={record} />
          <button onClick={() => setDisabled(true)} type="button">
            Disable
          </button>
        </>
      );
    }
    render(<Switchable />);

    fireEvent.pointerEnter(screen.getByRole("button", { name: "Preview" }), MOUSE_UP);
    fireEvent.click(screen.getByRole("button", { name: "Disable" }));

    expect(events).toEqual(["hoverstart:mouse", "hoverend:mouse"]);
  });

  it("ends a hover when the pointer turns up somewhere else without leaving", () => {
    const { events, record } = recorder();
    render(<HoverProbe onEvent={record} />);

    fireEvent.pointerEnter(screen.getByRole("button", { name: "Preview" }), MOUSE_UP);
    // The element moved out from under the pointer, or something covered it:
    // no `pointerleave` reaches it, and the pointer is over something else.
    fireEvent.pointerOver(bodyOf(), MOUSE_UP);

    expect(events).toEqual(["hoverstart:mouse", "hoverend:mouse"]);
  });
});

/** A button whose ring is `data-focus-visible`, and whose focus is `data-focused`. */
component RingButton(label?: string = "Bold") {
  const { focusProps, isFocused, isFocusVisible } = useFocusRing();
  return (
    <button
      {...focusProps}
      data-focus-visible={isFocusVisible ? "true" : undefined}
      data-focused={isFocused ? "true" : undefined}
      type="button"
    >
      {label}
    </button>
  );
}

/** A text field with a ring. */
component RingField() {
  const { focusProps, isFocusVisible } = useFocusRing();
  return (
    <input
      {...focusProps}
      aria-label="Title"
      data-focus-visible={isFocusVisible ? "true" : undefined}
    />
  );
}

/** A group whose ring is drawn while anything inside it has focus. */
component RingGroup() {
  const { focusProps, isFocused, isFocusVisible } = useFocusRing({ within: true });
  return (
    <div
      {...focusProps}
      aria-label="Formatting"
      data-focus-visible={isFocusVisible ? "true" : undefined}
      data-focused={isFocused ? "true" : undefined}
      role="group"
    >
      <button type="button">Bold</button>
      <button type="button">Italic</button>
    </div>
  );
}

/** The input that came last, and whether a ring would be drawn, as text. */
component ModalityProbe() {
  const modality = useInteractionModality();
  const { isFocusVisible } = useFocusVisible();
  return <output>{`${modality ?? "none"} ${isFocusVisible ? "visible" : "hidden"}`}</output>;
}

/** Move focus the way a script or the browser does, inside `act`. */
function focusOn(element: Element): void {
  act(() => {
    htmlOf(element).focus();
  });
}

describe("useFocusRing, and the input that came last", () => {
  it("draws a ring before anybody has done anything, so focus on load is visible", () => {
    render(<RingButton />);
    const control = screen.getByRole("button", { name: "Bold" });
    focusOn(control);
    expect(control).toHaveAttribute("data-focus-visible", "true");
  });

  it("draws a ring for focus that arrives from the keyboard", () => {
    render(<RingButton />);
    const control = screen.getByRole("button", { name: "Bold" });

    fireEvent.keyDown(bodyOf(), { key: "Tab" });
    focusOn(control);

    expect(control).toHaveAttribute("data-focused", "true");
    expect(control).toHaveAttribute("data-focus-visible", "true");
  });

  it("draws none for focus a pointer brought, and one as soon as a key is used", () => {
    render(<RingButton />);
    const control = screen.getByRole("button", { name: "Bold" });

    fireEvent.pointerDown(control, MOUSE);
    focusOn(control);
    expect(control).toHaveAttribute("data-focused", "true");
    expect(control).not.toHaveAttribute("data-focus-visible");

    fireEvent.keyDown(control, { key: "ArrowRight" });
    expect(control).toHaveAttribute("data-focus-visible", "true");
  });

  it("does not take a modifier on its own, or a shortcut, for the keyboard", () => {
    render(<RingButton />);
    const control = screen.getByRole("button", { name: "Bold" });
    fireEvent.pointerDown(control, MOUSE);
    focusOn(control);

    fireEvent.keyDown(control, { key: "Shift", shiftKey: true });
    fireEvent.keyDown(control, { ctrlKey: true, key: "c" });
    fireEvent.keyDown(control, { key: "c", metaKey: true });

    expect(control).not.toHaveAttribute("data-focus-visible");
  });

  it("does not take typing for the keyboard, except the keys that leave a field", () => {
    render(<RingField />);
    const field = screen.getByRole("textbox", { name: "Title" });
    fireEvent.pointerDown(field, MOUSE);
    focusOn(field);

    fireEvent.keyDown(field, { key: "a" });
    fireEvent.keyDown(field, { key: " " });
    expect(field).not.toHaveAttribute("data-focus-visible");

    fireEvent.keyDown(field, { key: "Tab" });
    expect(field).toHaveAttribute("data-focus-visible", "true");
  });

  it("takes a click with nothing pointing at it for a screen reader's, and a pointer's own click for the pointer's", () => {
    const { container } = render(
      <>
        <ModalityProbe />
        <button type="button">Other</button>
      </>,
    );
    const output = container.querySelector("output");
    const other = screen.getByRole("button", { name: "Other" });

    fireEvent.pointerDown(other, MOUSE);
    // The click a pointer went on to make, which a harness sends with a count
    // of nought: still the pointer's.
    fireEvent.click(other, { detail: 0 });
    expect(output?.textContent).toBe("pointer hidden");
    expect(getInteractionModality()).toBe("pointer");

    fireEvent.click(bodyOf(), { detail: 0 });
    expect(output?.textContent).toBe("virtual visible");

    fireEvent.keyDown(other, { key: "Tab" });
    expect(output?.textContent).toBe("keyboard visible");
  });

  it("counts focus anywhere inside with within, and none outside it", () => {
    render(
      <>
        <RingGroup />
        <RingButton label="Elsewhere" />
      </>,
    );
    const group = screen.getByRole("group", { name: "Formatting" });

    fireEvent.keyDown(bodyOf(), { key: "Tab" });
    focusOn(screen.getByRole("button", { name: "Bold" }));
    expect(group).toHaveAttribute("data-focused", "true");
    expect(group).toHaveAttribute("data-focus-visible", "true");

    focusOn(screen.getByRole("button", { name: "Italic" }));
    expect(group).toHaveAttribute("data-focused", "true");

    focusOn(screen.getByRole("button", { name: "Elsewhere" }));
    expect(group).not.toHaveAttribute("data-focused");
    expect(group).not.toHaveAttribute("data-focus-visible");
  });

  it("stops listening, and forgets what it saw, once nothing is asking", () => {
    component Toggle() {
      const [shown, setShown] = React.useState(true);
      return (
        <>
          {shown ? <ModalityProbe /> : null}
          <button onClick={() => setShown(false)} type="button">
            Hide
          </button>
        </>
      );
    }
    render(<Toggle />);

    fireEvent.keyDown(bodyOf(), { key: "Tab" });
    expect(getInteractionModality()).toBe("keyboard");

    fireEvent.click(screen.getByRole("button", { name: "Hide" }));
    // Nothing was watching in between, so what was last seen may no longer be
    // true, and it is not reported as if it were.
    expect(getInteractionModality()).toBe(null);
  });
});

/** A link a long press is on, with an ordinary press beside it. */
component LongPressProbe(
  onEvent: (event: { readonly type: string, readonly pointerType: string, ... }) => void,
  description?: string,
  onPress?: (event: PressEvent) => void,
  pointerTypes?: $ReadOnlyArray<"mouse" | "pen" | "touch">,
) {
  const { longPressProps } = useLongPress({
    accessibilityDescription: description,
    onLongPress: onEvent,
    onLongPressEnd: onEvent,
    onLongPressStart: onEvent,
    pointerTypes,
  });
  const { pressProps } = usePress({ onPress });
  return (
    <a {...mergeProps(pressProps, longPressProps)} href="#row">
      Row
    </a>
  );
}

/** Advance the fake clock inside `act`. */
function advance(millis: number): void {
  act(() => {
    uft.advanceTimersByTime(millis);
  });
}

describe("useLongPress", () => {
  it("fires once the threshold passes, with the finger still down, and takes the press with it", () => {
    uft.useFakeTimers();
    const { events, record } = recorder();
    const pressed = recorder();
    render(<LongPressProbe onEvent={record} onPress={pressed.record} />);
    const link = screen.getByRole("link", { name: "Row" });

    fireEvent.pointerDown(link, TOUCH);
    advance(499);
    expect(events).toEqual(["longpressstart:touch"]);
    advance(1);
    expect(events).toEqual(["longpressstart:touch", "longpress:touch", "longpressend:touch"]);

    fireEvent.pointerUp(link, TOUCH_UP);
    // The release still clicks the link. The click is refused, so the press
    // beside the long press never hears it and the link is not followed.
    expect(fireEvent.click(link, { button: 0, detail: 1 })).toBe(false);
    expect(pressed.events).toEqual([]);
  });

  it("is an ordinary press when it is let go of sooner", () => {
    uft.useFakeTimers();
    const { events, record } = recorder();
    const pressed = recorder();
    render(<LongPressProbe onEvent={record} onPress={pressed.record} />);
    const link = screen.getByRole("link", { name: "Row" });

    fireEvent.pointerDown(link, TOUCH);
    advance(200);
    fireEvent.pointerUp(link, TOUCH_UP);
    fireEvent.click(link, { button: 0, detail: 1 });
    advance(1000);

    expect(events).toEqual(["longpressstart:touch", "longpressend:touch"]);
    expect(pressed.events).toEqual(["press:touch"]);
  });

  it("answers only the pointers it was told to", () => {
    uft.useFakeTimers();
    const { events, record } = recorder();
    const pressed = recorder();
    render(<LongPressProbe onEvent={record} onPress={pressed.record} pointerTypes={["touch"]} />);
    const link = screen.getByRole("link", { name: "Row" });

    // A mouse held down on a row is starting a selection or a drag.
    fireEvent.pointerDown(link, MOUSE);
    advance(1000);
    fireEvent.pointerUp(link, MOUSE_UP);
    fireEvent.click(link, { button: 0, detail: 1 });

    expect(events).toEqual([]);
    expect(pressed.events).toEqual(["press:mouse"]);
  });

  it("ends when the finger leaves before the threshold", () => {
    uft.useFakeTimers();
    const { events, record } = recorder();
    render(<LongPressProbe onEvent={record} />);
    const link = screen.getByRole("link", { name: "Row" });

    fireEvent.pointerDown(link, TOUCH);
    fireEvent.pointerLeave(link, TOUCH);
    advance(1000);

    expect(events).toEqual(["longpressstart:touch", "longpressend:touch"]);
  });

  it("keeps the platform's own menu away from a finger held down, and only then", () => {
    uft.useFakeTimers();
    const { record } = recorder();
    render(<LongPressProbe onEvent={record} />);
    const link = screen.getByRole("link", { name: "Row" });

    fireEvent.pointerDown(link, TOUCH);
    expect(fireEvent.contextMenu(link)).toBe(false);
    fireEvent.pointerUp(link, TOUCH_UP);
    fireEvent.click(link, { button: 0, detail: 1 });

    expect(fireEvent.contextMenu(link)).toBe(true);
  });

  it("never comes from a key or from a screen reader", () => {
    uft.useFakeTimers();
    const { events, record } = recorder();
    render(<LongPressProbe onEvent={record} />);
    const link = screen.getByRole("link", { name: "Row" });

    fireEvent.keyDown(link, { key: "Enter" });
    virtualClick(link);
    advance(1000);

    expect(events).toEqual([]);
  });

  it("tells a reader what a long press does, through a description that exists", () => {
    const { record } = recorder();
    render(<LongPressProbe description="Long press for more actions" onEvent={record} />);
    const link = screen.getByRole("link", { name: "Row" });

    const id = link.getAttribute("aria-describedby") ?? "";
    expect(id).not.toBe("");
    expect(document.getElementById(id)?.textContent).toBe("Long press for more actions");
  });
});

/** A handle a drag moves, reporting every move event it is told about. */
component MoveProbe(
  onEvent: (event: { readonly type: string, readonly pointerType: string, ... }) => void,
) {
  const { moveProps } = useMove({ onMove: onEvent, onMoveEnd: onEvent, onMoveStart: onEvent });
  return <div {...moveProps} data-testid="handle" tabIndex={0} />;
}

describe("useMove", () => {
  it("reports how far a mouse dragged, from its first movement to its release", () => {
    const deltas: Array<string> = [];
    render(
      <MoveProbe
        onEvent={(event) => {
          const moved: $FlowFixMe = event;
          deltas.push(
            event.type === "move"
              ? `move ${String(moved.deltaX)},${String(moved.deltaY)}`
              : `${event.type}:${event.pointerType}`,
          );
        }}
      />,
    );
    const handle = screen.getByTestId("handle");

    fireEvent.pointerDown(handle, { ...MOUSE, clientX: 10, clientY: 10 });
    expect(deltas).toEqual([]);
    fireEvent.pointerMove(handle, { ...MOUSE, clientX: 15, clientY: 10 });
    // Off the element, which a drag always is before long: still this drag.
    fireEvent.pointerMove(bodyOf(), { ...MOUSE, clientX: 15, clientY: 30 });
    fireEvent.pointerUp(bodyOf(), { ...MOUSE_UP, clientX: 15, clientY: 30 });

    expect(deltas).toEqual(["movestart:mouse", "move 5,0", "move 0,20", "moveend:mouse"]);
  });

  it("is not a drag when nothing moved", () => {
    const { events, record } = recorder();
    render(<MoveProbe onEvent={record} />);
    const handle = screen.getByTestId("handle");

    fireEvent.pointerDown(handle, { ...MOUSE, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(handle, { ...MOUSE, clientX: 10, clientY: 10 });
    fireEvent.pointerUp(handle, { ...MOUSE_UP, clientX: 10, clientY: 10 });

    expect(events).toEqual([]);
  });

  it("is started by the primary button and no other", () => {
    const { events, record } = recorder();
    render(<MoveProbe onEvent={record} />);
    const handle = screen.getByTestId("handle");

    fireEvent.pointerDown(handle, { ...MOUSE, button: 2, buttons: 2, clientX: 10 });
    fireEvent.pointerMove(handle, { ...MOUSE, buttons: 2, clientX: 40 });

    expect(events).toEqual([]);
  });

  it("ends when the browser cancels the pointer", () => {
    const { events, record } = recorder();
    render(<MoveProbe onEvent={record} />);
    const handle = screen.getByTestId("handle");

    fireEvent.pointerDown(handle, { ...TOUCH, clientX: 10 });
    fireEvent.pointerMove(handle, { ...TOUCH, clientX: 20 });
    fireEvent.pointerCancel(handle, TOUCH);
    fireEvent.pointerMove(handle, { ...TOUCH, clientX: 30 });

    expect(events).toEqual(["movestart:touch", "move:touch", "moveend:touch"]);
  });

  it("ends when a mouse moves with no button held, which is a release it never heard", () => {
    const { events, record } = recorder();
    render(<MoveProbe onEvent={record} />);
    const handle = screen.getByTestId("handle");

    fireEvent.pointerDown(handle, { ...MOUSE, clientX: 10 });
    fireEvent.pointerMove(handle, { ...MOUSE, clientX: 20 });
    // The menu a right click opened took the `pointerup`. Without this the
    // handle would follow a pointer nobody is holding down.
    fireEvent.pointerMove(bodyOf(), { ...MOUSE_UP, clientX: 60 });
    fireEvent.pointerMove(bodyOf(), { ...MOUSE_UP, clientX: 90 });

    expect(events).toEqual(["movestart:mouse", "move:mouse", "moveend:mouse"]);
  });

  it("follows only the pointer that started it", () => {
    const { events, record } = recorder();
    render(<MoveProbe onEvent={record} />);
    const handle = screen.getByTestId("handle");

    fireEvent.pointerDown(handle, { ...TOUCH, clientX: 10 });
    fireEvent.pointerMove(handle, { ...TOUCH, clientX: 50, pointerId: 8 });

    expect(events).toEqual([]);
  });

  it("moves one step for an arrow key, and keeps the key from the page and from what is around it", () => {
    const deltas: Array<string> = [];
    const around: Array<string> = [];
    render(
      <div onKeyDown={(event) => around.push(String(event.key))}>
        <MoveProbe
          onEvent={(event) => {
            const moved: $FlowFixMe = event;
            deltas.push(
              event.type === "move"
                ? `move ${String(moved.deltaX)},${String(moved.deltaY)}`
                : `${event.type}:${event.pointerType}`,
            );
          }}
        />
      </div>,
    );
    const handle = screen.getByTestId("handle");

    expect(fireEvent.keyDown(handle, { key: "ArrowLeft" })).toBe(false);
    fireEvent.keyDown(handle, { key: "ArrowDown" });
    fireEvent.keyDown(handle, { key: "Home" });

    expect(deltas).toEqual([
      "movestart:keyboard",
      "move -1,0",
      "moveend:keyboard",
      "movestart:keyboard",
      "move 0,1",
      "moveend:keyboard",
    ]);
    expect(around).toEqual(["Home"]);
  });
});

/** A field whose keys a handler hears, inside a parent that hears whatever gets past. */
component KeyProbe(
  onKey: (event: {
    readonly key: string,
    readonly continuePropagation: () => void,
    readonly preventDefault: () => void,
    ...
  }) => mixed,
  onPast: (key: string) => mixed,
  isDisabled?: boolean = false,
) {
  const { keyboardProps } = useKeyboard({ isDisabled, onKeyDown: onKey });
  return (
    <div
      onKeyDown={(event) => onPast(String(event.key))}
      onKeyUp={(event) => onPast(`up ${String(event.key)}`)}
    >
      <input {...keyboardProps} aria-label="Name" />
    </div>
  );
}

describe("useKeyboard", () => {
  it("stops a key at the element unless the handler passes it on", () => {
    const heard: Array<string> = [];
    const past: Array<string> = [];
    render(
      <KeyProbe
        onKey={(event) => {
          heard.push(event.key);
          if (event.key === "a") {
            event.continuePropagation();
          }
        }}
        onPast={(key) => past.push(key)}
      />,
    );
    const field = screen.getByRole("textbox", { name: "Name" });

    fireEvent.keyDown(field, { key: "Delete" });
    fireEvent.keyDown(field, { key: "a" });

    expect(heard).toEqual(["Delete", "a"]);
    expect(past).toEqual(["a"]);
  });

  it("prevents the default only when asked, and attaches no handler it was not given", () => {
    const past: Array<string> = [];
    render(
      <KeyProbe
        onKey={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
          }
        }}
        onPast={(key) => past.push(key)}
      />,
    );
    const field = screen.getByRole("textbox", { name: "Name" });

    expect(fireEvent.keyDown(field, { key: "Enter" })).toBe(false);
    expect(fireEvent.keyDown(field, { key: "x" })).toBe(true);
    // No `onKeyUp` was given, so no key up is stopped.
    fireEvent.keyUp(field, { key: "x" });

    expect(past).toEqual(["up x"]);
  });

  it("hears nothing and stops nothing while disabled", () => {
    const heard: Array<string> = [];
    const past: Array<string> = [];
    render(
      <KeyProbe
        isDisabled
        onKey={(event) => heard.push(event.key)}
        onPast={(key) => past.push(key)}
      />,
    );

    fireEvent.keyDown(screen.getByRole("textbox", { name: "Name" }), { key: "Delete" });

    expect(heard).toEqual([]);
    expect(past).toEqual(["Delete"]);
  });
});

describe("mergeProps", () => {
  it("calls every handler, in the order the props were given", () => {
    const calls: Array<string> = [];
    const merged: $FlowFixMe = mergeProps(
      { onClick: (value: string) => calls.push(`first ${value}`) },
      { onClick: (value: string) => calls.push(`second ${value}`) },
    );
    merged.onClick("x");
    expect(calls).toEqual(["first x", "second x"]);
  });

  it("joins class names, and lets the last of anything else win unless it is undefined", () => {
    expect(
      mergeProps(
        { className: "a", id: "one", role: "button", title: "kept" },
        { className: "b", id: "two", title: undefined },
        null,
      ),
    ).toEqual({ className: "a b", id: "two", role: "button", title: "kept" });
  });

  it("puts a press, a hover and a ring on one element, and leaves an engine nothing to report", async () => {
    uft.useFakeTimers();
    component Control() {
      const { isPressed, pressProps } = usePress({});
      const { hoverProps, isHovered } = useHover({});
      const { focusProps, isFocusVisible } = useFocusRing();
      return (
        <div
          {...mergeProps(pressProps, hoverProps, focusProps)}
          data-focus-visible={isFocusVisible ? "true" : undefined}
          data-hovered={isHovered ? "true" : undefined}
          data-pressed={isPressed ? "true" : undefined}
          role="button"
          tabIndex={0}
        >
          Save
        </div>
      );
    }
    const { container } = render(
      <main>
        <h1>Draft</h1>
        <Control />
      </main>,
    );
    const control = screen.getByRole("button", { name: "Save" });

    fireEvent.pointerEnter(control, MOUSE_UP);
    fireEvent.pointerDown(control, MOUSE);
    expect(control).toHaveAttribute("data-hovered", "true");
    expect(control).toHaveAttribute("data-pressed", "true");
    // Focus moved to what was pressed, and a pointer brought it: no ring.
    expect(control).toHaveFocus();
    expect(control).not.toHaveAttribute("data-focus-visible");
    fireEvent.pointerUp(control, MOUSE_UP);
    fireEvent.click(control, { button: 0, detail: 1 });

    uft.useRealTimers();
    await expect(container).toHaveNoAxeViolations();
  });
});

afterEach(() => {
  uft.useRealTimers();
});
