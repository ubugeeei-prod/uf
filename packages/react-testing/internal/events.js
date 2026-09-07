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

import { bodyOf, documentOf } from "./dom.js";
import { displayValue } from "./queries.js";
import { actively } from "./render.js";

/**
 * What a caller wants the event to carry.
 *
 * An indexer, because which properties are meaningful is decided by the event's
 * *interface* and the interface is decided by the name — `{ key: "Escape" }`
 * for a `keydown`, `{ clientX: 40 }` for a `pointermove` — and the name is a
 * string a caller computes. There is no type that says "the initialisers of
 * whichever interface `name` maps to", so this says what is true: names in,
 * and what each one means is the DOM's business.
 */
export type EventInit = { readonly [string]: mixed };

/** Event constructors by DOM event name, with the right interface for each. */
const EVENT_TYPES: { readonly [string]: string } = {
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

/**
 * The bubbling event React actually listens for, for each one that does not
 * bubble.
 *
 * React attaches every listener to the root container rather than to the
 * element, so it can only hear events that reach the root. `focus` and `blur`
 * never do. React's answer is to listen for `focusin` and `focusout` — which
 * are the same moments and do bubble — and surface them to a component as
 * `onFocus` and `onBlur`.
 *
 * So dispatching a bare `focus` calls nothing: the element's own listener, if
 * a test added one directly, and no React handler at all. The test then
 * asserts on a component that never re-rendered and reads as a component bug.
 * Firing the pair is what a browser does anyway — a real focus is a `focus`
 * and a `focusin` — so this is less a workaround than the missing half.
 */
const ALSO_BUBBLES: { readonly [string]: string } = {
  focus: "focusin",
  blur: "focusout",
};

/**
 * The event a name asks for, built by the document's own class for it.
 *
 * # Why `Constructor` is `any`
 *
 * Neither half of this can be typed.
 *
 * Reading it: `globalThis` is a namespace to the checker rather than an
 * object, and the value under a computed name is `mixed`, which cannot be
 * `new`ed — refining a `mixed` with `typeof x === "function"` gives a function
 * whose signature Flow says it does not know.
 *
 * Calling it: this is the half that is not uf's to fix. Flow's library
 * definitions declare every event initialiser dictionary with *writable*
 * properties — `MouseEvent$MouseEventInit` has `clientX?: number`, not
 * `readonly clientX?: number` — so those properties are invariant, and no
 * value whose type was computed rather than written inline can be passed to
 * one. `{ readonly [string]: mixed }` fails on variance before it fails on
 * anything else:
 *
 *     error[incompatible-variance]: property `bubbles` is read-only in
 *     `EventInit` but writable in `Event$Init`
 *     error[incompatible-type]: in property `clientX`: `unknown` is not
 *     exactly the same as `number`
 *
 * — twelve of those for `MouseEvent` alone, and Flow's own advice in the
 * message is to make the library definition's property readonly. So an
 * `EventInit` cannot reach an event constructor under any spelling, and this
 * stays one cast at one line rather than a dozen errors at every call.
 */
function construct(name: string, init: EventInit): Event {
  // One cast, covering the lookup and both constructions; see above. Suppressed
  // rather than left to fail `check:lib`, because the half that would have to
  // change is Flow's library definition for the event initialiser dictionaries.
  // uf-lint-disable-next-line flow/unclear-type
  const classes: any = globalThis;
  const Constructor = classes[EVENT_TYPES[name] ?? "Event"] ?? classes.Event;
  const options = optionsFor(name, init);
  try {
    return new Constructor(name, options);
  } catch {
    // A host whose constructor is stricter than the init we were handed.
    return new classes.Event(name, options);
  }
}

/**
 * The defaults an event is built with, under whatever the caller asked for.
 *
 * Written as a loop rather than `{ bubbles, cancelable, ...init }` because
 * spreading an indexer is something Flow declines to compute a type for —
 * "the indexer `string` may overwrite properties with explicit keys in a way
 * that Flow cannot track", which is precisely what this is for. Its
 * suggestion, spreading `init` first, would reverse the precedence and stop a
 * caller from passing `bubbles: false`; the loop keeps the caller on top,
 * which is what the spread said.
 */
function optionsFor(name: string, init: EventInit): EventInit {
  const options: { [string]: mixed } = {
    bubbles: !NON_BUBBLING.has(name),
    cancelable: true,
  };
  for (const key of Object.keys(init)) {
    options[key] = init[key];
  }
  return options;
}

/**
 * Dispatch one event, inside `act`.
 *
 * Returns whether the event ran to completion — `false` when a handler called
 * `preventDefault`, which is what `dispatchEvent` reports and what a test
 * asserting "the form did not submit" needs.
 */
export function dispatch(target: EventTarget, name: string, init?: EventInit): boolean {
  const event = construct(name, init ?? {});
  const paired = ALSO_BUBBLES[name];
  let ran = true;
  actively(() => {
    ran = target.dispatchEvent(event);
    if (paired != null) {
      // Both, in the order a browser sends them, and inside the same `act` so
      // the component re-renders once rather than twice.
      target.dispatchEvent(construct(paired, init ?? {}));
    }
  });
  return ran;
}

/** One event name's firer: the event that name stands for, at this target. */
export type Firer = (target: EventTarget, init?: EventInit) => boolean;

/**
 * `fireEvent.click(element)`, and one entry per event name.
 *
 * # Why the names are written out
 *
 * This was a `Proxy` over a function, answering to any property at all and
 * dispatching whatever it was asked for lowercased. That is the shortest thing
 * to write and it cannot be typed, so the published type was `any`:
 * `fireEvent.clcik(button)` was not a misspelling anybody's checker would find,
 * and neither was passing something that is not an `EventTarget`. A hole like
 * that in the package whose purpose is testing *typed* components is the same
 * argument ubugeeei-prod/uf#381 made about the queries, which is why
 * `screen`'s thirty-six names are written out too.
 *
 * Two shapes were tried against the proxy and neither works, for reasons no
 * spelling fixes. An indexer — `readonly [string]: Firer` beside the call
 * signature — Flow declines, and is right to: the value is a function, a
 * function has `name`, `length`, `call`, `apply` and `bind`, and the trap
 * handed those back as themselves because `property in base` was true for
 * them. None of the five is a DOM event, so the lie was unreachable from a real
 * call, but a type is not something to be right about on average. A written-out
 * table over the proxy failed earlier still — "functions without statics are
 * not compatible with objects" — because a `Proxy` over a function *is* a
 * function, and Flow will not treat one carrying no statics as an object with
 * properties, whatever those properties are.
 *
 * So the value changed rather than the annotation. The names below are real
 * properties on a real function, attached inside a builder the way
 * `@uniflowed/test`'s `describe` attaches its modifiers, and the type is the
 * same list written down: Flow has no template literal types, so `fireEvent`'s
 * hundred-odd names have to be listed for the type to exist at all. The list is
 * the one React and Testing Library publish, so a suite being ported already
 * has these spellings.
 *
 * # What that costs
 *
 * `fireEvent.somethingNobodyListedYet(el)` stops working, and a table stops
 * answering to the hundred-and-sixth name the way a proxy never did. The
 * escape hatch is the call signature this has always had:
 * `fireEvent(target, "somethingnobodylistedyet", init)` takes a computed name
 * and always did — it is what the proxy called into — and `dispatch` is
 * exported for the same reason. So nothing became impossible; one spelling of
 * it became a name a reader can look up.
 *
 * The table also takes the camel-cased spelling only. The proxy lowercased
 * whatever it was handed, so `fireEvent.keydown` and `fireEvent.KeyDown`
 * worked as well as `fireEvent.keyDown`; the first of those is the DOM's own
 * name and the loss is real, though nothing in this repository or in a suite
 * written against Testing Library uses it.
 *
 * The type is inexact, and has to be: a function carries `name`, `length`,
 * `call`, `apply` and `bind`, and an exact object type refuses one for
 * exactly that reason. Inexactness costs nothing that matters here — Flow
 * still reports a read of a property the type does not list, which is what
 * makes `fireEvent.clcik` an error.
 */
export type FireEvent = {
  (target: EventTarget, name: string, init?: EventInit): boolean,

  // The clipboard.
  readonly copy: Firer,
  readonly cut: Firer,
  readonly paste: Firer,

  // An input method editor composing a character.
  readonly compositionEnd: Firer,
  readonly compositionStart: Firer,
  readonly compositionUpdate: Firer,

  // Keys.
  readonly keyDown: Firer,
  readonly keyPress: Firer,
  readonly keyUp: Firer,

  // Focus. `focus` and `blur` are paired with the bubbling forms React listens for; see `ALSO_BUBBLES`.
  readonly blur: Firer,
  readonly focus: Firer,
  readonly focusIn: Firer,
  readonly focusOut: Firer,

  // Forms.
  readonly beforeInput: Firer,
  readonly change: Firer,
  readonly input: Firer,
  readonly invalid: Firer,
  readonly reset: Firer,
  readonly submit: Firer,

  // The mouse.
  readonly auxClick: Firer,
  readonly click: Firer,
  readonly contextMenu: Firer,
  readonly dblClick: Firer,
  readonly mouseDown: Firer,
  readonly mouseEnter: Firer,
  readonly mouseLeave: Firer,
  readonly mouseMove: Firer,
  readonly mouseOut: Firer,
  readonly mouseOver: Firer,
  readonly mouseUp: Firer,

  // Dragging.
  readonly drag: Firer,
  readonly dragEnd: Firer,
  readonly dragEnter: Firer,
  readonly dragLeave: Firer,
  readonly dragOver: Firer,
  readonly dragStart: Firer,
  readonly drop: Firer,

  // Pointers, which is what a component that works under both a mouse and a finger listens for.
  readonly gotPointerCapture: Firer,
  readonly lostPointerCapture: Firer,
  readonly pointerCancel: Firer,
  readonly pointerDown: Firer,
  readonly pointerEnter: Firer,
  readonly pointerLeave: Firer,
  readonly pointerMove: Firer,
  readonly pointerOut: Firer,
  readonly pointerOver: Firer,
  readonly pointerUp: Firer,

  // Touch.
  readonly touchCancel: Firer,
  readonly touchEnd: Firer,
  readonly touchMove: Firer,
  readonly touchStart: Firer,

  // Scrolling and the wheel.
  readonly scroll: Firer,
  readonly scrollEnd: Firer,
  readonly wheel: Firer,

  // Selection.
  readonly select: Firer,
  readonly selectionChange: Firer,

  // Media.
  readonly abort: Firer,
  readonly canPlay: Firer,
  readonly canPlayThrough: Firer,
  readonly durationChange: Firer,
  readonly emptied: Firer,
  readonly encrypted: Firer,
  readonly ended: Firer,
  readonly loadStart: Firer,
  readonly loadedData: Firer,
  readonly loadedMetadata: Firer,
  readonly pause: Firer,
  readonly play: Firer,
  readonly playing: Firer,
  readonly progress: Firer,
  readonly rateChange: Firer,
  readonly seeked: Firer,
  readonly seeking: Firer,
  readonly stalled: Firer,
  readonly suspend: Firer,
  readonly timeUpdate: Firer,
  readonly volumeChange: Firer,
  readonly waiting: Firer,

  // Loading a resource.
  readonly error: Firer,
  readonly load: Firer,

  // Animations and transitions.
  readonly animationCancel: Firer,
  readonly animationEnd: Firer,
  readonly animationIteration: Firer,
  readonly animationStart: Firer,
  readonly transitionCancel: Firer,
  readonly transitionEnd: Firer,
  readonly transitionRun: Firer,
  readonly transitionStart: Firer,

  // A dialog and a disclosure.
  readonly cancel: Firer,
  readonly close: Firer,
  readonly toggle: Firer,

  // The window and the document.
  readonly beforeUnload: Firer,
  readonly hashChange: Firer,
  readonly message: Firer,
  readonly messageError: Firer,
  readonly offline: Firer,
  readonly online: Firer,
  readonly pageHide: Firer,
  readonly pageShow: Firer,
  readonly popState: Firer,
  readonly readyStateChange: Firer,
  readonly resize: Firer,
  readonly storage: Firer,
  readonly unload: Firer,
  readonly visibilityChange: Firer,
  ...
};

/**
 * Build `fireEvent`: the callable, with one firer per name hung off it.
 *
 * Inside a builder rather than at the module's top level, because a shipped
 * module may only declare, import and export at its top level and
 * `fireEvent.click = …` out here is a statement that runs on import.
 */
function firing(): FireEvent {
  const fires =
    (name: string): Firer =>
    (target: EventTarget, init?: EventInit) =>
      dispatch(target, name, init);
  const api = (target: EventTarget, name: string, init?: EventInit) => dispatch(target, name, init);

  // The clipboard.
  api.copy = fires("copy");
  api.cut = fires("cut");
  api.paste = fires("paste");

  // An input method editor composing a character.
  api.compositionEnd = fires("compositionend");
  api.compositionStart = fires("compositionstart");
  api.compositionUpdate = fires("compositionupdate");

  // Keys.
  api.keyDown = fires("keydown");
  api.keyPress = fires("keypress");
  api.keyUp = fires("keyup");

  // Focus. `focus` and `blur` are paired with the bubbling forms React listens for; see `ALSO_BUBBLES`.
  api.blur = fires("blur");
  api.focus = fires("focus");
  api.focusIn = fires("focusin");
  api.focusOut = fires("focusout");

  // Forms.
  api.beforeInput = fires("beforeinput");
  api.change = fires("change");
  api.input = fires("input");
  api.invalid = fires("invalid");
  api.reset = fires("reset");
  api.submit = fires("submit");

  // The mouse.
  api.auxClick = fires("auxclick");
  api.click = fires("click");
  api.contextMenu = fires("contextmenu");
  api.dblClick = fires("dblclick");
  api.mouseDown = fires("mousedown");
  api.mouseEnter = fires("mouseenter");
  api.mouseLeave = fires("mouseleave");
  api.mouseMove = fires("mousemove");
  api.mouseOut = fires("mouseout");
  api.mouseOver = fires("mouseover");
  api.mouseUp = fires("mouseup");

  // Dragging.
  api.drag = fires("drag");
  api.dragEnd = fires("dragend");
  api.dragEnter = fires("dragenter");
  api.dragLeave = fires("dragleave");
  api.dragOver = fires("dragover");
  api.dragStart = fires("dragstart");
  api.drop = fires("drop");

  // Pointers, which is what a component that works under both a mouse and a finger listens for.
  api.gotPointerCapture = fires("gotpointercapture");
  api.lostPointerCapture = fires("lostpointercapture");
  api.pointerCancel = fires("pointercancel");
  api.pointerDown = fires("pointerdown");
  api.pointerEnter = fires("pointerenter");
  api.pointerLeave = fires("pointerleave");
  api.pointerMove = fires("pointermove");
  api.pointerOut = fires("pointerout");
  api.pointerOver = fires("pointerover");
  api.pointerUp = fires("pointerup");

  // Touch.
  api.touchCancel = fires("touchcancel");
  api.touchEnd = fires("touchend");
  api.touchMove = fires("touchmove");
  api.touchStart = fires("touchstart");

  // Scrolling and the wheel.
  api.scroll = fires("scroll");
  api.scrollEnd = fires("scrollend");
  api.wheel = fires("wheel");

  // Selection.
  api.select = fires("select");
  api.selectionChange = fires("selectionchange");

  // Media.
  api.abort = fires("abort");
  api.canPlay = fires("canplay");
  api.canPlayThrough = fires("canplaythrough");
  api.durationChange = fires("durationchange");
  api.emptied = fires("emptied");
  api.encrypted = fires("encrypted");
  api.ended = fires("ended");
  api.loadStart = fires("loadstart");
  api.loadedData = fires("loadeddata");
  api.loadedMetadata = fires("loadedmetadata");
  api.pause = fires("pause");
  api.play = fires("play");
  api.playing = fires("playing");
  api.progress = fires("progress");
  api.rateChange = fires("ratechange");
  api.seeked = fires("seeked");
  api.seeking = fires("seeking");
  api.stalled = fires("stalled");
  api.suspend = fires("suspend");
  api.timeUpdate = fires("timeupdate");
  api.volumeChange = fires("volumechange");
  api.waiting = fires("waiting");

  // Loading a resource.
  api.error = fires("error");
  api.load = fires("load");

  // Animations and transitions.
  api.animationCancel = fires("animationcancel");
  api.animationEnd = fires("animationend");
  api.animationIteration = fires("animationiteration");
  api.animationStart = fires("animationstart");
  api.transitionCancel = fires("transitioncancel");
  api.transitionEnd = fires("transitionend");
  api.transitionRun = fires("transitionrun");
  api.transitionStart = fires("transitionstart");

  // A dialog and a disclosure.
  api.cancel = fires("cancel");
  api.close = fires("close");
  api.toggle = fires("toggle");

  // The window and the document.
  api.beforeUnload = fires("beforeunload");
  api.hashChange = fires("hashchange");
  api.message = fires("message");
  api.messageError = fires("messageerror");
  api.offline = fires("offline");
  api.online = fires("online");
  api.pageHide = fires("pagehide");
  api.pageShow = fires("pageshow");
  api.popState = fires("popstate");
  api.readyStateChange = fires("readystatechange");
  api.resize = fires("resize");
  api.storage = fires("storage");
  api.unload = fires("unload");
  api.visibilityChange = fires("visibilitychange");
  return api;
}

/** Dispatch one event, by name. See [`FireEvent`]. */
export const fireEvent: FireEvent = firing();

/** Set a control's value the way a browser does, so React sees the change. */
function setValue(element: HTMLElement, value: string): void {
  // React tracks the last value it wrote on the node and skips an `input`
  // event whose value it believes it already knows. Writing through the
  // prototype's setter is what a browser does and what clears that.
  const descriptor = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(element), "value");
  const set = descriptor?.set;
  if (set != null) {
    set.call(element, value);
  } else if (
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement ||
    element instanceof HTMLSelectElement
  ) {
    // The fallback, for a host whose control classes keep `value` as an own
    // property rather than an accessor on the prototype. The three classes are
    // the ones `displayValue` reads, so what a test writes is what a
    // `ByDisplayValue` query can find.
    element.value = value;
  }
}

/** A key's `key`, `code` and printable text. */
function describeKey(key: string): {| key: string, code: string, text: string | null |} {
  const named: { readonly [string]: {| code: string, text: string | null |} } = {
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

/**
 * Elements the tab order includes, in document order.
 *
 * `instanceof HTMLElement` rather than a cast, and it is not only the
 * checker's question: the next thing done to one of these is `focus`, and
 * `focus` is a method of `HTMLElement`. The declared return type has always
 * said `HTMLElement` while the selector could match an `Element` — Flow said
 * so, "in array element: `Element` is incompatible with `HTMLElement`" — and
 * anything that reached here without being one would have been handed to a
 * `(element as any).focus?.()` that silently did nothing, swallowing the Tab.
 */
function tabbable(): Array<HTMLElement> {
  const selector =
    'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';
  const order = [];
  for (const element of documentOf().querySelectorAll(selector)) {
    if (element instanceof HTMLElement && element.getAttribute("aria-hidden") !== "true") {
      order.push(element);
    }
  }
  return order;
}

/**
 * Whether a control is disabled, for the elements that can be.
 *
 * The classes rather than `(element as any).disabled === true`, which was the
 * same question asked in a way that answered `any`. `disabled` belongs to the
 * form controls and to `fieldset`; on a `div` it is an attribute React put in
 * the markup and not a property, which is why the old test was `=== true`
 * rather than truthiness, and why the answer for one is still `false`.
 */
function isDisabled(element: HTMLElement): boolean {
  return (
    (element instanceof HTMLButtonElement ||
      element instanceof HTMLInputElement ||
      element instanceof HTMLSelectElement ||
      element instanceof HTMLTextAreaElement ||
      element instanceof HTMLFieldSetElement) &&
    element.disabled
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
  async click(element: HTMLElement, init?: EventInit): Promise<void> {
    if (isDisabled(element)) {
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
      // Nothing is typed into an element that shows no value; see `keyboard`
      // for what that used to do instead.
      const current = displayValue(element);
      if (printable != null && printable !== "\n" && current != null) {
        setValue(element, `${current}${printable}`);
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
    const target = documentOf().activeElement ?? bodyOf();
    for (const token of parseKeys(sequence)) {
      const { key, code, text } = describeKey(token);
      dispatch(target, "keydown", { key, code });
      // `displayValue` rather than `target.value !== undefined`.
      //
      // The two agree about every control a person can type into, and differ
      // about `<button>`, `<option>`, `<progress>` and the rest of the
      // elements that have a `value` property without showing one: pressing
      // Space at a focused button used to write `button.value = " "` and
      // dispatch an `input` event at it, which no browser does — Space on a
      // button is a click. Nothing in this repository's suite depended on it,
      // and `ui.test.js` presses Space at a switch on the way past.
      const current = displayValue(target);
      if (text != null && text !== "\n" && current != null) {
        setValue(target, `${current}${text}`);
        dispatch(target, "input", { data: text });
      }
      dispatch(target, "keyup", { key, code });
    }
    await settle();
  },

  /** Move focus the way the Tab key does. */
  async tab(options?: {| readonly shift?: boolean |}): Promise<void> {
    const order = tabbable();
    if (order.length === 0) {
      return;
    }
    const active = documentOf().activeElement;
    // `indexOf` needs an element; a document with nothing focused is the same
    // "not in the order" that `indexOf` answers `-1` to, said in front.
    const at = active == null ? -1 : order.indexOf(active);
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
    // Only a `select` has options; anything else has none, which is what
    // `select.options ?? []` used to say. The events are dispatched either
    // way, because a component listening for `change` on something that is not
    // a select is a component under test and not this function's business.
    if (element instanceof HTMLSelectElement) {
      for (const option of Array.from(element.options)) {
        option.selected = wanted.includes(option.value);
      }
    }
    dispatch(element, "input");
    dispatch(element, "change");
    await settle();
  },

  /** Move focus away, which is what makes a blur-validated field validate. */
  async tabAway(element: HTMLElement): Promise<void> {
    dispatch(element, "blur");
    element.blur();
    await settle();
  },
};

function focus(element: HTMLElement): void {
  if (documentOf().activeElement === element) {
    return;
  }
  actively(() => {
    element.focus();
  });
  if (documentOf().activeElement !== element) {
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
