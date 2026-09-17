// @flow
//
// Interactions: the press, the hover, the focus ring, the long press, the drag
// and the key, each written once, so that every component above them inherits
// one answer instead of writing its own.
//
// The DOM has no "press". It has `pointerdown`, `mousedown`, `keydown`, `keyup`
// and `click`, and which of those arrive — in which order, carrying which
// `pointerType` — depends on the device, the browser, and whether assistive
// technology made the gesture up. A component that listens for one of them is
// right about one of those cases, and the cases it is wrong about are the ones
// nobody meets with a mouse on a laptop:
//
//   * **`onClick` alone** is right for a mouse and for a screen reader, and
//     silent at the keyboard on anything that is not a `<button>` or a link.
//     `render` hands a part's props to whatever element a caller renders, so a
//     `Dialog.Trigger` rendered as a `<span tabIndex={0}>` opened for a pointer
//     and did nothing at all for `Enter` or `Space`.
//   * **`onPointerDown` alone** fires for the right button, fires for a press
//     the reader dragged away from and let go of somewhere else, and never
//     fires for a screen reader, which clicks without a pointer.
//   * **`Space` on `keydown`** presses on the way down, and a held key repeats
//     its `keydown` — so holding `Space` on a switch flips it at the keyboard's
//     repeat rate, where a native button waits for the key to come up and
//     presses once.
//   * **`pointerenter` as hover** counts a finger as a mouse. And on iOS a tap
//     is followed by a *second* `pointerenter` whose `pointerType` is `"mouse"`
//     (WebKit bug 214609), so a tooltip that ignores `"touch"` and trusts
//     `"mouse"` opens after the very tap it promised not to open on — and stays
//     open, because no pointer is ever going to leave.
//   * **`:focus-visible`** is the platform's answer to "draw a focus ring?", and
//     for focus that a *script* moved — which is how every menu, select and
//     dialog in this package puts focus inside itself — the specification
//     leaves the answer to each browser's heuristics rather than to the input
//     the reader actually used.
//
// React Aria's interactions package is the reference for getting all of these
// right at once. This module is written against its documented semantics, and
// where it differs the difference is deliberate and argued where it is made.
//
// # A press
//
// `usePress` turns every route to activation into four events — `pressstart`,
// `pressup`, `pressend` and `press` — each saying which input made it, and an
// `isPressed` for a stylesheet to draw from. The rules, and what each prevents:
//
//   * **A press ends where it started.** A pointer that goes down on the element
//     and comes up somewhere else ends the press without pressing. Leaving ends
//     it too, and coming back while still down starts it again — unless
//     `shouldCancelOnPointerExit`, which takes the press back for good the
//     moment the pointer leaves.
//   * **Only the primary button.** A right click opens a context menu; it does
//     not also press whatever was under it.
//   * **A pointer press completes on the browser's own click.** `press` fires
//     from the `click` that follows `pointerup`, not from `pointerup`. Firing on
//     `pointerup` is the classic way to cause a ghost click: `onPress` removes
//     the element, and the click the browser sends next lands on whatever was
//     underneath it. The click also carries what the platform decided — a
//     press released over a different child, a link followed, a form submitted
//     — so the press and the platform cannot disagree about whether one
//     happened.
//   * **A click nothing pointed at is assistive technology's.** VoiceOver, JAWS,
//     NVDA and TalkBack activate an element by clicking it with no pointer
//     before the click, and that click is a whole press — `pressstart`,
//     `pressup`, `pressend` and `press` together, with `pointerType: "virtual"`.
//   * **Keys belong to the element that has focus.** `Enter` and `Space` press
//     the focused element and never an ancestor that happens to contain it, so
//     a link inside a pressable card is the link's.
//   * **An element whose activation does more than press keeps it.** A link
//     follows, a submit or reset button submits or resets its form, a checkable
//     `<input>` checks and a `<summary>` opens its details, and the browser does
//     each of those from the click it sends for `Enter` or `Space`. On those the
//     press completes from that click, which is the only way `Enter` on a
//     submit button can both press and submit — and nothing fires twice,
//     because nothing but that click ever fires `press` for them.
//   * **Everything else is given a button's keyboard, and the browser's click
//     is claimed.** On a `<button type="button">`, whose click does nothing but
//     press, and on a `<div>` or a `<span>` — anything `render` might put a part
//     on — `Enter` presses on key down and `Space` on key up, the timing a
//     native button has, and the keys' default actions are prevented so a
//     native button is not clicked a second time. The press then completes
//     through one click dispatched at the element — the click a button would
//     have made — so every route to activation ends in exactly one click, and a
//     component's click handler hears the keyboard as well as the pointer.
//     `role="link"` keeps a link's keyboard (`Enter` only) and
//     `role="checkbox"` and `role="radio"` keep a checkbox's (`Space` only;
//     `Enter` is the form's).
//   * **A held key is one press.** Its repeats are claimed and ignored.
//   * **Focus moves to what was pressed**, without scrolling, which is what
//     every browser but Safari already does for a button. `preventFocusOnPress`
//     is how a control whose focus must stay put — an option in a list whose
//     focus lives in the field — says so.
//   * **Text selection is off while a pointer is down**, so a press that lasts
//     does not select the label on its way — `allowTextSelectionOnPress` for
//     the element that wants it.
//   * **A press inside a pressable is the inner element's.** The outer one does
//     not also press, unless a handler of the inner one calls
//     `continuePropagation()`.
//
// Two of those differ from React Aria on purpose. React Aria completes `Enter`
// on key *up*; a native button completes it on key down, and a part rendered
// as a `<div>` must not press at a different moment from the same part rendered
// as a `<button>`. And React Aria keeps nested presses apart by stopping the
// event's propagation; this records which element answered the event instead,
// because a press stopped at the element never reaches a document listener in
// the bubbling phase — `@uniflowed/hooks/dom`'s `useClickOutside` is one — and
// "a press outside closes it" is a promise a component makes to a reader who
// pressed something else, not to the thing they pressed.
//
// # A press outside
//
// `useInteractOutside` is the other half of a press, and the one an overlay
// asks for: not "was this element pressed?" but "did the reader press
// somewhere else?". A `pointerdown` alone cannot answer it. On a touchscreen
// that event is the first of *every* touch, the one that scrolls the page
// included; the browser cancels that pointer as the scroll begins and never
// sends a click, so an overlay that dismissed on `pointerdown` had already
// gone by the time the reader's finger moved. It is wrong for a mouse in two
// smaller ways as well: a press that starts outside and is released inside
// dismissed, and so did the right button, which is asking for a context menu
// beside an overlay rather than asking for it to go away.
//
// So an outside interaction is a whole gesture: a `pointerdown` *and* the
// release that ends the same pointer, both outside, from the primary button,
// with the target still in the document. A gesture the browser cancels has no
// end and dismisses nothing. The listeners are the document's and in the
// capture phase, so a component that stops the event inside the page cannot
// hide a press from the overlay above it — "a press outside closes it" is a
// promise made to the reader who pressed something else.
//
// # Hover
//
// `useHover` is a mouse's and a pen's. A finger has no hover, so a touch
// pointer is ignored outright, and so is a `"mouse"` pointer arriving within
// half a second of a touch — the emulated events a tap is followed by, which
// on iOS include that second `pointerenter`. The window is short on purpose: a
// laptop with a touchscreen whose reader taps and then reaches for the trackpad
// should get hover back.
//
// # Which input came last
//
// `useFocusRing` answers "is the ring drawn?" from one fact kept for the whole
// document: which kind of input the reader used most recently. A key makes it
// the keyboard's, a pointer going down makes it the pointer's, and a click
// with nothing before it makes it assistive technology's. The ring shows for
// anything but a pointer, and three refinements keep that honest:
//
//   * a modifier on its own, or a shortcut, is not the keyboard taking over —
//     `⌘C` after a click must not draw rings across the page;
//   * typing into a text field is not either, except `Tab` and `Escape`, which
//     are how a reader leaves one;
//   * before anybody has done anything the answer is "not a pointer", so a
//     field focused as the page loads shows its ring.
//
// React Aria also counts focus that arrives with no event before it as
// assistive technology's, and to tell that apart from a script calling
// `focus()` it replaces `HTMLElement.prototype.focus` for the whole page. This
// package will not rewrite a platform method underneath every other script on
// somebody's page, so focus that arrives with no event keeps whatever the last
// input was. What that costs is a screen reader moving focus without any event
// while the last input was a pointer: no ring is drawn for a reader who is
// almost never the one looking for it.
//
// # A long press, a drag, and a key
//
// `useLongPress` is `usePress` with a clock. When the clock runs out the press
// underneath is cancelled — a `pointercancel` at the element, which every
// press and every drag in this module ends on — and the click that the release
// sends is refused, so a long press on a link does not also follow it. It has
// no keyboard and cannot have one: a long press is a pointer asking for a
// second action, and the component offering that action owes a key for it too.
// `ContextMenu` has `Shift+F10`; `accessibilityDescription` is how a reader is
// told which.
//
// `useMove` reports how far a pointer travelled since the last event rather
// than where it is, which is what makes it the same for a mouse, a finger and
// an arrow key. It listens on the document once a drag begins, so a pointer
// that leaves the element is still dragging it. It ends on `pointerup` and
// `pointercancel`, and it also ends on a mouse move with no button held, which
// is what a drag looks like after its `pointerup` was swallowed — the menu a
// right click opens does exactly that — and which otherwise leaves a thumb
// following a pointer nobody is holding down.
//
// `useKeyboard` hands a handler the key and stops the event at the element
// unless the handler calls `continuePropagation()`. That is the opposite of the
// DOM's default and the right one for a control: a key a slider used must not
// also move the roving focus of a list around it, and a key it did not use is
// passed on by saying so rather than by remembering not to stop it.
//
// # Why this is a subpath, when `internal/` is not
//
// Every module under `internal/` is a rule about markup this package builds —
// that a tab lives under a `tablist`, that a part's props go on last — and its
// promise only holds because the components build both halves of it. These are
// rules about input devices instead. They hold for any element, and a control a
// consumer builds with them is not a weaker copy of a part here: it is the same
// press, the same hover and the same ring, from the same code.
//
// `mergeProps` is the one helper beside them, and it is not
// `internal/merge-props.js`. That module decides whose props win between a part
// and its caller. This one chains the handlers of several of these hooks aimed
// at one element the caller owns — `usePress`, `useHover` and `useFocusRing` on
// one button all want `onPointerDown`, `onPointerEnter` or `onFocus`, and a
// spread would keep only the last.
//
// # What these promise React
//
// Nothing here reads the DOM or writes a ref during a render. Listeners that a
// gesture needs are attached by the handler of the event that began it and
// removed when it ends or when the component unmounts; the two listeners that
// watch the whole document — which input came last, and whether a touch just
// happened — are attached in an effect by the first component that asks and
// removed by the last one to go. Which input came last is a store, read through
// `useSyncExternalStore` with a server snapshot of "nobody has done anything
// yet", so a prerender and the hydrating render agree.

"use client";

import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

/** The input that produced an interaction. */
export type PointerType = "mouse" | "pen" | "touch" | "keyboard" | "virtual";

/** Which kind of input a reader used most recently. */
export type Modality = "keyboard" | "pointer" | "virtual";

/** A pointer with a position: what a hover, a drag or a long press can come from. */
export type PhysicalPointer = "mouse" | "pen" | "touch";

/**
 * The part of an event these hooks read, from React or from the DOM.
 *
 * Inexact and named for the reason `internal/merge-props.js` gives for
 * `PartEvent`: what arrives is React's synthetic event, uf does not merge Flow's
 * `jsx.js` environment so nothing models one, and these are the members the
 * handlers here actually read. A handler written for this type accepts a
 * native event as well, which is what lets a listener on the document share
 * its reading of a key or a pointer with a handler on the element.
 */
export type InteractionEvent = {
  readonly type: string,
  readonly target: mixed,
  readonly currentTarget: mixed,
  readonly defaultPrevented: boolean,
  readonly nativeEvent?: mixed,
  readonly preventDefault: () => mixed,
  readonly stopPropagation: () => mixed,
  readonly key?: string,
  readonly code?: string,
  readonly repeat?: boolean,
  readonly button?: number,
  readonly buttons?: number,
  readonly detail?: number,
  readonly pointerId?: number,
  readonly pointerType?: string,
  readonly clientX?: number,
  readonly clientY?: number,
  readonly width?: number,
  readonly height?: number,
  readonly pressure?: number,
  readonly relatedTarget?: mixed,
  readonly altKey?: boolean,
  readonly ctrlKey?: boolean,
  readonly metaKey?: boolean,
  readonly shiftKey?: boolean,
  ...
};

/**
 * Props on their way onto an element, as `mergeProps` returns them.
 *
 * `key` is named out of the indexer for the reason `Rest` in
 * `internal/merge-props.js` gives at length: an indexer answers `mixed` for
 * every name, React's `key` is `string | number`, and spreading one onto an
 * element is rejected for a property that cannot be there.
 */
export type InteractionProps = { readonly key?: empty, readonly [string]: mixed };

/** The modifier keys held while something happened. */
type Modifiers = {|
  readonly altKey: boolean,
  readonly ctrlKey: boolean,
  readonly metaKey: boolean,
  readonly shiftKey: boolean,
|};

/** Where something happened, relative to the element's own box. */
type Point = {| readonly x: number, readonly y: number |};

/**
 * How long a `"mouse"` pointer is disbelieved after a touch, in milliseconds.
 *
 * Long enough to cover the emulated events a tap is followed by — which arrive
 * in the same task on iOS, and after the click delay a page that allows zooming
 * still has — and far shorter than a reader takes to move a hand from a
 * touchscreen to a trackpad.
 */
const EMULATED_MOUSE_WINDOW = 500;

/** How long a press has to last to be a long press, in milliseconds. */
const LONG_PRESS_THRESHOLD = 500;

/** Every pointer a long press can come from, which is the default. */
const EVERY_POINTER: $ReadOnlyArray<PhysicalPointer> = ["mouse", "pen", "touch"];

/** `<input>` types a reader does not type into. */
const NON_TEXT_INPUTS: Set<string> = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);

/**
 * The elements a pointer focuses when it presses them.
 *
 * A browser focuses these on `mousedown`, and any element with a `tabindex` —
 * including `-1`, which is what every roving item in this package carries.
 */
const FOCUSABLE_BY_POINTER =
  "a[href], area[href], button, input, select, textarea, summary, iframe, [contenteditable=''], [contenteditable='true']";

/** The modifier keys an event reports, with a missing one read as not held. */
function modifiersOf(source: mixed): Modifiers {
  const event: $FlowFixMe = source;
  return {
    altKey: event?.altKey === true,
    ctrlKey: event?.ctrlKey === true,
    metaKey: event?.metaKey === true,
    shiftKey: event?.shiftKey === true,
  };
}

/**
 * The pointer an event names, as one of the three this module distinguishes.
 *
 * An empty or missing `pointerType` is a mouse: it is what a host without
 * pointer types reports for one, and what a click synthesised by a test
 * harness carries.
 */
function physicalPointerOf(pointerType: mixed): PhysicalPointer {
  return pointerType === "touch" ? "touch" : pointerType === "pen" ? "pen" : "mouse";
}

/** The element an event's `currentTarget` is, which is always an element here. */
function elementOf(target: mixed): HTMLElement {
  return target as $FlowFixMe;
}

/** Whether `node` is `container` or inside it, for any value an event carried. */
function contains(container: HTMLElement, node: mixed): boolean {
  const candidate: $FlowFixMe = node;
  return (
    candidate != null && typeof candidate.nodeType === "number" && container.contains(candidate)
  );
}

/** Where an event happened relative to an element's box, or its corner when it has no position. */
function pointOf(source: mixed, element: HTMLElement): Point {
  const event: $FlowFixMe = source;
  const box = element.getBoundingClientRect();
  const x = typeof event?.clientX === "number" ? event.clientX : box.left;
  const y = typeof event?.clientY === "number" ? event.clientY : box.top;
  return { x: x - box.left, y: y - box.top };
}

/** The user agent string, or nothing on a host without a navigator. */
function userAgent(): string {
  const host: $FlowFixMe = globalThis;
  const agent = host.navigator?.userAgent;
  return typeof agent === "string" ? agent : "";
}

/** Whether this is Android, whose screen reader reports its clicks differently. */
function isAndroid(): boolean {
  return /Android/i.test(userAgent());
}

/**
 * Whether this is an Apple platform, where `Option` types characters.
 *
 * On macOS and iOS `Option+e` is how an accent is typed, so it is typing; on
 * every other platform `Alt` held with a key is a shortcut, and a shortcut is
 * not the keyboard taking over from the pointer.
 */
function isApplePlatform(): boolean {
  const host: $FlowFixMe = globalThis;
  const platform = host.navigator?.userAgentData?.platform ?? host.navigator?.platform;
  return typeof platform === "string" && /mac|iphone|ipad|ipod/i.test(platform);
}

/**
 * Whether a pointer event is one assistive technology made up.
 *
 * The two shapes React Aria documents. VoiceOver on iOS sends pointer events
 * with no contact area at all; TalkBack's double tap on Android sends a mouse
 * pointer of unit size, with no pressure and no click count — a shape a real
 * mouse on Android does not have, and one Safari's real mouse *does* have
 * (Safari reports no pressure), which is why that half is asked only on
 * Android.
 */
function isVirtualPointer(event: InteractionEvent): boolean {
  if (isAndroid()) {
    return (
      event.width === 1 &&
      event.height === 1 &&
      event.pressure === 0 &&
      event.detail === 0 &&
      event.pointerType === "mouse"
    );
  }
  return event.width === 0 && event.height === 0;
}

/** The event the DOM dispatched, under whichever wrapper React put on it. */
function nativeOf(event: InteractionEvent): Event {
  return (event.nativeEvent ?? event) as $FlowFixMe;
}

/**
 * Whether a click came from something that did not point.
 *
 * A pointer's click counts itself: `detail` is how many clicks in a row it
 * was, and it is never nought. A click with a count of nought and no pointer
 * type is a screen reader's, or a script's. Firefox reports JAWS and NVDA
 * clicks with an empty `pointerType` from a trusted event, and TalkBack on
 * Android reports its click with the button still held.
 */
function isVirtualClick(event: InteractionEvent): boolean {
  const native: $FlowFixMe = nativeOf(event);
  if (native.pointerType === "" && native.isTrusted === true) {
    return true;
  }
  if (isAndroid() && typeof native.pointerType === "string" && native.pointerType !== "") {
    return native.buttons === 1;
  }
  return native.detail === 0 && !native.pointerType;
}

/** Whether a reader types into this element, so the keys it receives are text. */
function isTextEntry(target: mixed): boolean {
  const element: $FlowFixMe = target;
  if (element == null || typeof element.tagName !== "string") {
    return false;
  }
  if (element.isContentEditable === true) {
    return true;
  }
  const tag = element.tagName.toUpperCase();
  if (tag === "TEXTAREA") {
    return true;
  }
  return tag === "INPUT" && !NON_TEXT_INPUTS.has(String(element.type ?? "text").toLowerCase());
}

/**
 * Turn text selection off on an element, and hand back how to turn it on again.
 *
 * Inline, and restored to exactly what was there, for the reason
 * `internal/disclosure.js` gives about its measuring pass: a stylesheet must
 * never be left fighting an inline declaration this wrote.
 */
function withoutTextSelection(element: HTMLElement | null): () => void {
  if (element == null) {
    return () => {};
  }
  const style = element.style;
  const before = style.getPropertyValue("user-select");
  const beforeWebkit = style.getPropertyValue("-webkit-user-select");
  style.setProperty("user-select", "none");
  style.setProperty("-webkit-user-select", "none");
  return () => {
    restoreProperty(style, "user-select", before);
    restoreProperty(style, "-webkit-user-select", beforeWebkit);
  };
}

/** Put one inline declaration back to what it was, including to nothing. */
function restoreProperty(style: CSSStyleDeclaration, name: string, value: string): void {
  if (value === "") {
    style.removeProperty(name);
  } else {
    style.setProperty(name, value);
  }
}

/**
 * Focus what a pointer pressed, the way a browser does, without scrolling.
 *
 * Only an element a pointer would focus: a `<div>` with no `tabindex` is not
 * focusable, and asking it to be does nothing anyway.
 */
function focusWithoutScrolling(element: HTMLElement): void {
  if (element.ownerDocument.activeElement === element) {
    return;
  }
  if (!element.hasAttribute("tabindex") && !element.matches(FOCUSABLE_BY_POINTER)) {
    return;
  }
  element.focus({ preventScroll: true });
}

// ---------------------------------------------------------------------------
// Which element answered an event
// ---------------------------------------------------------------------------

/**
 * The element that answered each event, for a press inside a press.
 *
 * Keyed on the DOM's event rather than React's wrapper, because a document
 * listener and a React handler see different wrappers of the same event. A
 * `WeakMap`, so an event nobody holds any more takes its entry with it.
 */
const answeredBy: WeakMap<Event, HTMLElement> = new WeakMap();

/** Whether a different element — one inside this one — already answered the event. */
function answeredElsewhere(event: InteractionEvent, element: HTMLElement): boolean {
  const by = answeredBy.get(nativeOf(event));
  return by != null && by !== element;
}

/**
 * Record that `element` answered the event, unless something inside it did first.
 *
 * Two hooks on the *same* element are not nested and both see the event: a
 * `usePress` for the click and a `useLongPress` for the hold are one control.
 */
function markAnswered(event: InteractionEvent, element: HTMLElement): void {
  const native = nativeOf(event);
  if (!answeredBy.has(native)) {
    answeredBy.set(native, element);
  }
}

// ---------------------------------------------------------------------------
// usePress
// ---------------------------------------------------------------------------

/** A moment in a press. */
export type PressEvent = {|
  readonly type: "pressstart" | "pressend" | "pressup" | "press",
  /** The input that made it. */
  readonly pointerType: PointerType,
  /** The element the press belongs to. */
  readonly target: HTMLElement,
  readonly altKey: boolean,
  readonly ctrlKey: boolean,
  readonly metaKey: boolean,
  readonly shiftKey: boolean,
  /** Where the pointer was, from the element's left edge; nought for a key or a screen reader. */
  readonly x: number,
  /** Where the pointer was, from the element's top edge; nought for a key or a screen reader. */
  readonly y: number,
  /**
   * Let a pressable around this one receive the same press.
   *
   * By default a press belongs to the innermost pressable element; see the
   * module header for why that is recorded rather than stopped.
   */
  readonly continuePropagation: () => void,
|};

/** What `usePress` is told. */
export type PressOptions = {|
  /** Leave text selection alone while a pointer is down. */
  readonly allowTextSelectionOnPress?: boolean,
  /**
   * No press, no hover state, and no activation of the element either: a click
   * on a disabled link or submit button is prevented.
   */
  readonly isDisabled?: boolean,
  /** The press completed, over the element. */
  readonly onPress?: (event: PressEvent) => mixed,
  /** `isPressed` changed. */
  readonly onPressChange?: (isPressed: boolean) => mixed,
  /** The press ended, pressed or not: released, left, cancelled or taken away. */
  readonly onPressEnd?: (event: PressEvent) => mixed,
  /** A press began, or a pointer still down came back over the element. */
  readonly onPressStart?: (event: PressEvent) => mixed,
  /** A pointer or a key was released over the element, whether or not the press began there. */
  readonly onPressUp?: (event: PressEvent) => mixed,
  /** Keep focus where it is when a pointer presses the element. */
  readonly preventFocusOnPress?: boolean,
  /** A pointer that leaves takes the press back for good, rather than until it returns. */
  readonly shouldCancelOnPointerExit?: boolean,
|};

/** The handlers `usePress` needs on the element. */
export type PressProps = {|
  readonly onClick: (event: InteractionEvent) => void,
  readonly onDragStart: (event: InteractionEvent) => void,
  readonly onKeyDown: (event: InteractionEvent) => void,
  readonly onMouseDown: (event: InteractionEvent) => void,
  readonly onPointerDown: (event: InteractionEvent) => void,
  readonly onPointerEnter: (event: InteractionEvent) => void,
  readonly onPointerLeave: (event: InteractionEvent) => void,
  readonly onPointerUp: (event: InteractionEvent) => void,
|};

/** What `usePress` hands back. */
export type PressResult = {|
  /** Whether a press is under way and over the element. */
  readonly isPressed: boolean,
  /** Spread onto the element, or merged with other hooks' props by `mergeProps`. */
  readonly pressProps: PressProps,
|};

/** A pointer that is down on the element. */
type PointerPress = {|
  cancelled: boolean,
  over: boolean,
  readonly pointerId: number,
  readonly pointerType: PhysicalPointer,
  readonly stop: () => void,
  readonly target: HTMLElement,
|};

/** `Space` held down on the element. */
type KeyPress = {|
  readonly native: boolean,
  readonly stop: () => void,
  readonly target: HTMLElement,
|};

/** A pointer press that ended over the element and is owed the click that follows. */
type OwedPress = {|
  ...Modifiers,
  readonly point: Point,
  readonly pointerType: PhysicalPointer,
|};

/** Everything a press in progress is made of. Written by handlers, never read by a render. */
type PressState = {|
  key: KeyPress | null,
  keyClick: HTMLElement | null,
  owed: OwedPress | null,
  pointer: PointerPress | null,
  pressed: boolean,
  refuse: boolean,
|};

/** Which keys press an element, and whether the browser clicks it for them. */
type KeyRule = {| readonly enter: boolean, readonly native: boolean, readonly space: boolean |};

/**
 * The keys that press `element`, or nothing when its keys are text.
 *
 * `native` names the elements whose own activation — following a link,
 * submitting or resetting a form, checking a box, opening a `<summary>` — comes
 * from the click the browser sends for a key. The press waits for that click on
 * them, and claims the key everywhere else; the module header says why.
 *
 * The tag is asked before the role because the browser asks the tag: a
 * `<button type="button" role="checkbox">` takes `Enter` like any button, so a
 * part that wants `Enter` for something else — `Checkbox` submits the form with
 * it — prevents the key first, and a press never sees it.
 */
function keyRuleFor(element: HTMLElement): KeyRule | null {
  if (element.isContentEditable) {
    return null;
  }
  const tag = element.tagName.toUpperCase();
  if (tag === "TEXTAREA" || tag === "SELECT") {
    return null;
  }
  if (tag === "INPUT") {
    const type = String((element as $FlowFixMe).type ?? "").toLowerCase();
    if (type === "checkbox" || type === "radio") {
      return { enter: false, native: true, space: true };
    }
    if (type === "submit" || type === "reset" || type === "image") {
      return { enter: true, native: true, space: true };
    }
    if (type === "button") {
      return { enter: true, native: false, space: true };
    }
    return null;
  }
  if (tag === "BUTTON") {
    // `.type` rather than the attribute: a `<button>` with none is a submit
    // button, and submitting is the activation a press cannot do instead.
    const type = String((element as $FlowFixMe).type ?? "submit").toLowerCase();
    return { enter: true, native: type === "submit" || type === "reset", space: true };
  }
  if (tag === "SUMMARY") {
    return { enter: true, native: true, space: true };
  }
  if ((tag === "A" || tag === "AREA") && element.hasAttribute("href")) {
    return { enter: true, native: true, space: false };
  }
  const role = element.getAttribute("role");
  if (role === "link") {
    return { enter: true, native: false, space: false };
  }
  if (role === "checkbox" || role === "radio") {
    return { enter: false, native: false, space: true };
  }
  return { enter: true, native: false, space: true };
}

/**
 * A press, from a pointer, a key or assistive technology, with one set of events.
 *
 *     const { isPressed, pressProps } = usePress({ onPress: () => save() });
 *     return <div {...pressProps} data-pressed={isPressed} role="button" tabIndex={0}>Save</div>;
 *
 * The module header states every rule and what each one prevents. The events
 * arrive in the order `pressstart`, `pressup`, `pressend`, `press`, and
 * `onPressChange` reports every change to `isPressed` between them.
 */
export hook usePress(options?: PressOptions): PressResult {
  const [isPressed, setPressed] = useState(false);
  const state = useRef<PressState>({
    key: null,
    keyClick: null,
    owed: null,
    pointer: null,
    pressed: false,
    refuse: false,
  });

  const emit = useStableCallback(
    (
      type: "pressstart" | "pressend" | "pressup" | "press",
      pointerType: PointerType,
      target: HTMLElement,
      source: mixed,
      point: Point | null,
    ): boolean => {
      let continued = false;
      const event: PressEvent = {
        ...modifiersOf(source),
        continuePropagation: () => {
          continued = true;
        },
        pointerType,
        target,
        type,
        x: point?.x ?? 0,
        y: point?.y ?? 0,
      };
      const handler =
        type === "pressstart"
          ? options?.onPressStart
          : type === "pressup"
            ? options?.onPressUp
            : type === "pressend"
              ? options?.onPressEnd
              : options?.onPress;
      handler?.(event);
      return continued;
    },
  );

  const change = useStableCallback((next: boolean) => {
    const current = state.current;
    if (current.pressed === next) {
      return;
    }
    current.pressed = next;
    setPressed(next);
    options?.onPressChange?.(next);
  });

  /** End a pointer press without pressing: cancelled, dragged away, or disabled. */
  const cancelPointer = useStableCallback((source: mixed) => {
    const current = state.current;
    const press = current.pointer;
    if (press == null) {
      return;
    }
    current.pointer = null;
    current.owed = null;
    press.stop();
    if (press.over && !press.cancelled) {
      change(false);
      emit("pressend", press.pointerType, press.target, source, null);
    }
  });

  const onDocumentPointerUp = useStableCallback((native: $FlowFixMe) => {
    const current = state.current;
    const press = current.pointer;
    if (press == null || (native.pointerId ?? 0) !== press.pointerId) {
      return;
    }
    current.pointer = null;
    press.stop();
    const released = contains(press.target, native.target);
    if (press.cancelled) {
      // Taken back when the pointer left. Coming back and letting go over the
      // element still makes the browser click it, and that click is not a press.
      current.refuse = released;
      return;
    }
    if (!press.over) {
      return;
    }
    change(false);
    const point = pointOf(native, press.target);
    emit("pressend", press.pointerType, press.target, native, point);
    if (released) {
      current.owed = { ...modifiersOf(native), point, pointerType: press.pointerType };
    }
  });

  const onDocumentPointerCancel = useStableCallback((native: $FlowFixMe) => {
    const press = state.current.pointer;
    if (press != null && (native.pointerId ?? 0) === press.pointerId) {
      cancelPointer(native);
    }
  });

  const onPointerDown = useStableCallback((event: InteractionEvent) => {
    const element = elementOf(event.currentTarget);
    if (answeredElsewhere(event, element)) {
      return;
    }
    const current = state.current;
    // A new press settles whatever the last one left owing.
    current.owed = null;
    current.refuse = false;
    current.keyClick = null;
    if (options?.isDisabled === true) {
      markAnswered(event, element);
      return;
    }
    // A virtual pointer is left to the click it is followed by, which is where
    // a screen reader's press is recognised.
    if (event.button !== 0 || current.pointer != null || isVirtualPointer(event)) {
      return;
    }
    const pointerId = event.pointerId ?? 0;
    const pointerType = physicalPointerOf(event.pointerType);
    // A touch or a pen is captured to the element it went down on, so without
    // this the element would never hear the finger leave it.
    const captured: $FlowFixMe = element;
    if (
      typeof captured.hasPointerCapture === "function" &&
      captured.hasPointerCapture(pointerId) === true
    ) {
      captured.releasePointerCapture(pointerId);
    }
    if (options?.preventFocusOnPress !== true) {
      focusWithoutScrolling(element);
    }
    const document = element.ownerDocument;
    const restoreSelection =
      options?.allowTextSelectionOnPress === true ? null : withoutTextSelection(element);
    document.addEventListener("pointerup", onDocumentPointerUp, false);
    document.addEventListener("pointercancel", onDocumentPointerCancel, false);
    current.pointer = {
      cancelled: false,
      over: true,
      pointerId,
      pointerType,
      stop: () => {
        document.removeEventListener("pointerup", onDocumentPointerUp, false);
        document.removeEventListener("pointercancel", onDocumentPointerCancel, false);
        restoreSelection?.();
      },
      target: element,
    };
    const continued = emit("pressstart", pointerType, element, event, pointOf(event, element));
    change(true);
    if (!continued) {
      markAnswered(event, element);
    }
  });

  const onPointerUp = useStableCallback((event: InteractionEvent) => {
    const element = elementOf(event.currentTarget);
    if (
      answeredElsewhere(event, element) ||
      options?.isDisabled === true ||
      event.button !== 0 ||
      isVirtualPointer(event)
    ) {
      return;
    }
    const press = state.current.pointer;
    if (
      press != null &&
      ((event.pointerId ?? 0) !== press.pointerId || press.cancelled || !press.over)
    ) {
      return;
    }
    const pointerType = press?.pointerType ?? physicalPointerOf(event.pointerType);
    if (!emit("pressup", pointerType, element, event, pointOf(event, element))) {
      markAnswered(event, element);
    }
  });

  const onPointerLeave = useStableCallback((event: InteractionEvent) => {
    const press = state.current.pointer;
    if (press == null || (event.pointerId ?? 0) !== press.pointerId || !press.over) {
      return;
    }
    press.over = false;
    change(false);
    emit("pressend", press.pointerType, press.target, event, pointOf(event, press.target));
    if (options?.shouldCancelOnPointerExit === true) {
      press.cancelled = true;
    }
  });

  const onPointerEnter = useStableCallback((event: InteractionEvent) => {
    const press = state.current.pointer;
    if (
      press == null ||
      (event.pointerId ?? 0) !== press.pointerId ||
      press.over ||
      press.cancelled
    ) {
      return;
    }
    press.over = true;
    emit("pressstart", press.pointerType, press.target, event, pointOf(event, press.target));
    change(true);
  });

  // Safari starts a native drag without sending `pointercancel`, and a press
  // that turned into a drag is not a press.
  const onDragStart = useStableCallback((event: InteractionEvent) => {
    cancelPointer(event);
  });

  // The default action of `mousedown` is what moves focus, for a mouse and for
  // the emulated mouse a touch is followed by.
  const onMouseDown = useStableCallback((event: InteractionEvent) => {
    if (event.button === 0 && options?.preventFocusOnPress === true) {
      event.preventDefault();
    }
  });

  const onDocumentKeyUp = useStableCallback((native: $FlowFixMe) => {
    const current = state.current;
    const press = current.key;
    if (press == null || (native.key !== " " && native.key !== "Spacebar")) {
      return;
    }
    current.key = null;
    press.stop();
    // Released where it went down, which is where focus still is. A reader who
    // moved focus while holding the key has taken the press elsewhere.
    const released = contains(press.target, native.target);
    if (released) {
      emit("pressup", "keyboard", press.target, native, null);
    }
    change(false);
    emit("pressend", "keyboard", press.target, native, null);
    if (!released) {
      return;
    }
    current.keyClick = press.target;
    if (press.native) {
      // The browser clicks it after this listener returns, and that click is
      // the press; see the module header.
      return;
    }
    // Claimed, so Firefox does not click a button on key up as well, and the
    // click a button would have made is dispatched instead; see `onKeyDown`.
    native.preventDefault();
    press.target.click();
  });

  const onKeyPressBlur = useStableCallback((native: $FlowFixMe) => {
    const current = state.current;
    const press = current.key;
    if (press == null) {
      return;
    }
    current.key = null;
    press.stop();
    change(false);
    emit("pressend", "keyboard", press.target, native, null);
  });

  const onKeyDown = useStableCallback((event: InteractionEvent) => {
    const element = elementOf(event.currentTarget);
    const key = event.key;
    const space = key === " " || key === "Spacebar";
    if (
      (!space && key !== "Enter") ||
      event.target !== element ||
      event.defaultPrevented ||
      options?.isDisabled === true
    ) {
      return;
    }
    const rule = keyRuleFor(element);
    if (rule == null || (space ? !rule.space : !rule.enter)) {
      return;
    }
    const current = state.current;
    if (event.repeat === true) {
      // One press for a held key. The repeat is claimed so that neither the
      // page, which would scroll, nor the browser, which would click again,
      // takes it instead.
      event.preventDefault();
      return;
    }
    if (current.key != null || current.pointer != null) {
      return;
    }
    current.owed = null;
    current.refuse = false;
    current.keyClick = null;
    if (!rule.native) {
      event.preventDefault();
    }
    emit("pressstart", "keyboard", element, event, null);
    change(true);
    if (space) {
      // Completed on key up, the way a button is. On the document, so a key
      // released after focus moved is still heard, and ended by a blur.
      const document = element.ownerDocument;
      document.addEventListener("keyup", onDocumentKeyUp, true);
      element.addEventListener("blur", onKeyPressBlur, false);
      current.key = {
        native: rule.native,
        stop: () => {
          document.removeEventListener("keyup", onDocumentKeyUp, true);
          element.removeEventListener("blur", onKeyPressBlur, false);
        },
        target: element,
      };
      return;
    }
    emit("pressup", "keyboard", element, event, null);
    change(false);
    emit("pressend", "keyboard", element, event, null);
    // The press completes on a click either way: the browser's own, for an
    // element whose click does something, or the one a button would have made,
    // dispatched here — so every route to activation ends in exactly one click,
    // and a click handler hears the keyboard as well as the pointer.
    current.keyClick = element;
    if (!rule.native) {
      element.click();
    }
  });

  const onClick = useStableCallback((event: InteractionEvent) => {
    const element = elementOf(event.currentTarget);
    if (answeredElsewhere(event, element)) {
      return;
    }
    const current = state.current;
    if (options?.isDisabled === true) {
      // A disabled control does not act, and neither does the link or the
      // submit button it was rendered as.
      event.preventDefault();
      markAnswered(event, element);
      return;
    }
    if (current.refuse) {
      current.refuse = false;
      event.preventDefault();
      markAnswered(event, element);
      return;
    }
    const owed = current.owed;
    if (owed != null) {
      current.owed = null;
      if (!emit("press", owed.pointerType, element, owed, owed.point)) {
        markAnswered(event, element);
      }
      return;
    }
    if (current.pointer != null) {
      return;
    }
    if (current.keyClick === element) {
      current.keyClick = null;
      if (!emit("press", "keyboard", element, event, null)) {
        markAnswered(event, element);
      }
      return;
    }
    // A pointer's click with no press before it went down somewhere else, or
    // before anything here was listening, and is not a press of this element.
    if (!isVirtualClick(event)) {
      return;
    }
    let continued = emit("pressstart", "virtual", element, event, null);
    change(true);
    continued = emit("pressup", "virtual", element, event, null) || continued;
    change(false);
    continued = emit("pressend", "virtual", element, event, null) || continued;
    continued = emit("press", "virtual", element, event, null) || continued;
    if (!continued) {
      markAnswered(event, element);
    }
  });

  // Disabled in the middle of a press: it ends there, unpressed.
  const isDisabled = options?.isDisabled === true;
  useEffect(() => {
    if (!isDisabled) {
      return;
    }
    cancelPointer(null);
    const current = state.current;
    const press = current.key;
    if (press != null) {
      current.key = null;
      press.stop();
      change(false);
      emit("pressend", "keyboard", press.target, null, null);
    }
  }, [isDisabled, cancelPointer, change, emit]);

  // Taken away in the middle of a press: its listeners go with it.
  useEffect(
    () => () => {
      const current = state.current;
      current.pointer?.stop();
      current.key?.stop();
      current.pointer = null;
      current.key = null;
    },
    [],
  );

  const pressProps = useMemo(
    () => ({
      onClick,
      onDragStart,
      onKeyDown,
      onMouseDown,
      onPointerDown,
      onPointerEnter,
      onPointerLeave,
      onPointerUp,
    }),
    [
      onClick,
      onDragStart,
      onKeyDown,
      onMouseDown,
      onPointerDown,
      onPointerEnter,
      onPointerLeave,
      onPointerUp,
    ],
  );

  return { isPressed, pressProps };
}

// ---------------------------------------------------------------------------
// useInteractOutside
// ---------------------------------------------------------------------------

/** A ref to an element a press may land in without being "outside". */
export type InteractOutsideRef = { readonly current: HTMLElement | null, ... };

/** What `useInteractOutside` is told. */
export type InteractOutsideOptions = {|
  /** Hear nothing: what an overlay that is closed asks for. */
  readonly isDisabled?: boolean,
  /** A whole gesture began and ended outside every ref. */
  readonly onInteractOutside: (event: Event) => mixed,
  /**
   * The elements that are not "outside".
   *
   * The overlay, and whatever opens it: a trigger sits outside the overlay's
   * own box and is not "outside" for this purpose, because dismissing there
   * and then letting the trigger's own click reopen makes a press on it a
   * no-op that flickers.
   *
   * Read when an event arrives rather than when the listener is attached, so a
   * ref that is still null on the commit that opened the overlay is not a
   * listener that quietly never worked.
   */
  readonly refs: $ReadOnlyArray<InteractOutsideRef>,
|};

/** A pointer that went down outside and has not ended anywhere yet. */
type OutsideGesture = {| readonly pointerId: number |};

/**
 * Whether `node` is still in the document.
 *
 * A press whose target left the page between going down and coming up did not
 * end outside anything: it ended on something that is no longer there, and the
 * reader was most likely pressing what replaced it.
 */
function isStillInDocument(document: mixed, node: mixed): boolean {
  const root: $FlowFixMe = document;
  const element: $FlowFixMe = root?.documentElement;
  return element != null && contains(element, node);
}

/**
 * A press that began *and* ended outside an element: what dismisses an overlay.
 *
 *     useInteractOutside({
 *       isDisabled: !open,
 *       onInteractOutside: close,
 *       refs: [bodyRef, triggerRef],
 *     });
 *
 * The module header says what a bare `pointerdown` gets wrong and why this
 * waits for the end of the gesture. One hook rather than a copy in each
 * overlay, because a copy is how the answers drift apart: the case that a
 * scroll must not dismiss is one rule, not five.
 */
export hook useInteractOutside(options: InteractOutsideOptions): void {
  const gesture = useRef<OutsideGesture | null>(null);
  const isDisabled = options.isDisabled === true;

  const isOutside = useStableCallback((target: mixed): boolean => {
    for (const ref of options.refs) {
      const element: $FlowFixMe = ref.current;
      if (element != null && contains(element, target)) {
        return false;
      }
    }
    return true;
  });

  const dismiss = useStableCallback((event: Event) => {
    options.onInteractOutside(event);
  });

  useEffect(() => {
    if (isDisabled) {
      gesture.current = null;
      return;
    }
    const host: $FlowFixMe = globalThis;
    const document = host.document;
    if (document == null) {
      return;
    }

    // Only the primary button begins one. A right click is asking for a
    // context menu beside the overlay, not for the overlay to go away.
    const onPointerDown = (event: $FlowFixMe) => {
      gesture.current =
        event.button === 0 && isOutside(event.target) ? { pointerId: event.pointerId ?? 0 } : null;
    };

    // The release that ends the gesture this began. A click is accepted as
    // that end as well, for a host where the release never arrives — and it
    // cannot dismiss twice, because the first end takes the gesture with it.
    const end = (event: $FlowFixMe, samePointer: boolean) => {
      const began = gesture.current;
      if (began == null || (samePointer && (event.pointerId ?? 0) !== began.pointerId)) {
        return;
      }
      gesture.current = null;
      if (
        event.button !== 0 ||
        !isOutside(event.target) ||
        !isStillInDocument(document, event.target)
      ) {
        return;
      }
      dismiss(event);
    };

    const onPointerUp = (event: $FlowFixMe) => end(event, true);
    const onClick = (event: $FlowFixMe) => end(event, false);

    // A gesture the browser took back — the scroll this hook exists for — ends
    // nowhere, and dismisses nothing.
    const onPointerCancel = (event: $FlowFixMe) => {
      const began = gesture.current;
      if (began != null && (event.pointerId ?? 0) === began.pointerId) {
        gesture.current = null;
      }
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("pointerup", onPointerUp, true);
    document.addEventListener("pointercancel", onPointerCancel, true);
    document.addEventListener("click", onClick, true);
    return () => {
      gesture.current = null;
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("pointerup", onPointerUp, true);
      document.removeEventListener("pointercancel", onPointerCancel, true);
      document.removeEventListener("click", onClick, true);
    };
  }, [isDisabled, isOutside, dismiss]);
}

// ---------------------------------------------------------------------------
// Which input came last
// ---------------------------------------------------------------------------

/** The input used most recently, while anything is listening; `null` before any. */
let lastModality: Modality | null = null;

/** What the last key or pointer went down on, so its own click is not mistaken for a screen reader's. */
let interactionTarget: mixed = null;

/** Everything subscribed to `lastModality`. */
const modalitySubscribers: Set<() => void> = new Set();

/** Removes the document listeners, while there are any. */
let stopTrackingModality: (() => void) | null = null;

/** Record the input used most recently, and tell whoever is listening. */
function announceModality(next: Modality): void {
  if (lastModality === next) {
    return;
  }
  lastModality = next;
  for (const subscriber of modalitySubscribers) {
    subscriber();
  }
}

function onModalityKey(event: $FlowFixMe): void {
  const key = event.key;
  // A modifier on its own, or a shortcut, is not the keyboard taking over.
  if (key === "Alt" || key === "Control" || key === "Meta" || key === "Shift") {
    return;
  }
  if (event.metaKey === true || event.ctrlKey === true) {
    return;
  }
  if (event.altKey === true && !isApplePlatform()) {
    return;
  }
  // Typing is not either — except the two keys that leave a field.
  if (isTextEntry(event.target) && key !== "Tab" && key !== "Escape") {
    return;
  }
  interactionTarget = event.target;
  announceModality("keyboard");
}

function onModalityPointer(event: $FlowFixMe): void {
  interactionTarget = event.target;
  announceModality(isVirtualPointer(event) ? "virtual" : "pointer");
}

function onModalityClick(event: $FlowFixMe): void {
  // The click a key or a pointer went on to make is theirs: the next click,
  // on what they went down on or on an ancestor both ends of the press were
  // in. Only the next one, so a screen reader's click on the page that follows
  // a mouse press inside it is not mistaken for that press's.
  const origin = interactionTarget;
  interactionTarget = null;
  if (origin != null && (origin === event.target || contains(elementOf(event.target), origin))) {
    return;
  }
  if (isVirtualClick(event)) {
    announceModality("virtual");
  }
}

/**
 * Listen for which input is used, for as long as anybody is asking.
 *
 * Capture, on the document, so an event a component stops is still counted —
 * the reader still used that input.
 */
function subscribeModality(subscriber: () => void): () => void {
  modalitySubscribers.add(subscriber);
  const host: $FlowFixMe = globalThis;
  const document = host.document;
  if (stopTrackingModality == null && document != null) {
    document.addEventListener("keydown", onModalityKey, true);
    document.addEventListener("keyup", onModalityKey, true);
    document.addEventListener("pointerdown", onModalityPointer, true);
    document.addEventListener("mousedown", onModalityPointer, true);
    document.addEventListener("click", onModalityClick, true);
    stopTrackingModality = () => {
      document.removeEventListener("keydown", onModalityKey, true);
      document.removeEventListener("keyup", onModalityKey, true);
      document.removeEventListener("pointerdown", onModalityPointer, true);
      document.removeEventListener("mousedown", onModalityPointer, true);
      document.removeEventListener("click", onModalityClick, true);
    };
  }
  return () => {
    modalitySubscribers.delete(subscriber);
    if (modalitySubscribers.size > 0 || stopTrackingModality == null) {
      return;
    }
    stopTrackingModality();
    stopTrackingModality = null;
    // Nothing was watching in between, so whatever was last seen may no longer
    // be true; "nobody has done anything yet" is the answer that draws rings.
    lastModality = null;
    interactionTarget = null;
  };
}

function readModality(): Modality | null {
  return lastModality;
}

function readServerModality(): Modality | null {
  return null;
}

/**
 * The input used most recently, or `null` when nothing is listening for it.
 *
 * For an event handler deciding something now — whether focus it is about to
 * move should draw a ring. A render that depends on the answer reads
 * `useInteractionModality` instead, which also keeps the listening on.
 */
export function getInteractionModality(): Modality | null {
  return stopTrackingModality == null ? null : lastModality;
}

/** The input used most recently, re-rendering when it changes; `null` before any. */
export hook useInteractionModality(): Modality | null {
  return useSyncExternalStore(subscribeModality, readModality, readServerModality);
}

/** What `useFocusVisible` hands back. */
export type FocusVisibleResult = {|
  /** Whether focus, wherever it is, should be drawn: anything but a pointer came last. */
  readonly isFocusVisible: boolean,
|};

/**
 * Whether a focus ring should be drawn, for the page as a whole.
 *
 * `useFocusRing` is the one a control wants — it adds whether the control has
 * focus. This is for something that draws focus elsewhere, or an overlay that
 * decides whether to draw one on what it focused.
 */
export hook useFocusVisible(): FocusVisibleResult {
  const modality = useInteractionModality();
  return { isFocusVisible: modality !== "pointer" };
}

/** What `useFocusRing` is told. */
export type FocusRingOptions = {|
  /** Count focus anywhere inside the element, not only on the element itself. */
  readonly within?: boolean,
|};

/** The handlers `useFocusRing` needs on the element. */
export type FocusRingProps = {|
  readonly onBlur: (event: InteractionEvent) => void,
  readonly onFocus: (event: InteractionEvent) => void,
|};

/** What `useFocusRing` hands back. */
export type FocusRingResult = {|
  readonly focusProps: FocusRingProps,
  /** Whether the element — or, with `within`, something inside it — has focus. */
  readonly isFocused: boolean,
  /** Whether it has focus and the input that came last was not a pointer. */
  readonly isFocusVisible: boolean,
|};

/**
 * Whether an element has focus, and whether that focus should be drawn.
 *
 *     const { focusProps, isFocusVisible } = useFocusRing();
 *     return <button {...focusProps} data-focus-visible={isFocusVisible || undefined}>Save</button>;
 *
 * React Aria's version takes `isTextInput` and `autoFocus` as well. Neither is
 * needed here: typing into a text field is ignored for the whole document, and
 * focus that arrives before any input is drawn already, because nothing has
 * made it a pointer's.
 */
export hook useFocusRing(options?: FocusRingOptions): FocusRingResult {
  const within = options?.within === true;
  const [isFocused, setFocused] = useState(false);
  const { isFocusVisible } = useFocusVisible();
  const watching = useRef<(() => void) | null>(null);

  const stopWatching = useStableCallback(() => {
    watching.current?.();
    watching.current = null;
  });

  const onFocus = useStableCallback((event: InteractionEvent) => {
    const element = elementOf(event.currentTarget);
    if (!within && event.target !== element) {
      return;
    }
    setFocused(true);
    if (watching.current != null) {
      return;
    }
    // A focused element taken out of the document takes its blur with it, so
    // the next focus anywhere else is how that is noticed.
    const document = element.ownerDocument;
    const onFocusElsewhere = (native: $FlowFixMe) => {
      if (!contains(element, native.target)) {
        stopWatching();
        setFocused(false);
      }
    };
    document.addEventListener("focusin", onFocusElsewhere, true);
    watching.current = () => document.removeEventListener("focusin", onFocusElsewhere, true);
  });

  const onBlur = useStableCallback((event: InteractionEvent) => {
    const element = elementOf(event.currentTarget);
    if (within ? contains(element, event.relatedTarget) : event.target !== element) {
      return;
    }
    stopWatching();
    setFocused(false);
  });

  useEffect(() => stopWatching, [stopWatching]);

  const focusProps = useMemo(() => ({ onBlur, onFocus }), [onBlur, onFocus]);
  return { focusProps, isFocused, isFocusVisible: isFocused && isFocusVisible };
}

// ---------------------------------------------------------------------------
// useHover
// ---------------------------------------------------------------------------

/** Whether a `"mouse"` pointer is to be disbelieved because a touch just happened. */
let ignoreEmulatedMouse = false;

/** The clock that ends `ignoreEmulatedMouse`. */
let emulatedMouseTimer: TimeoutID | null = null;

/** How many hovers are listening for touches, and how to stop. */
let touchWatchers = 0;
let stopWatchingTouches: (() => void) | null = null;

function onTouchPointer(event: $FlowFixMe): void {
  if (event.pointerType !== "touch") {
    return;
  }
  ignoreEmulatedMouse = true;
  if (emulatedMouseTimer != null) {
    clearTimeout(emulatedMouseTimer);
  }
  emulatedMouseTimer = setTimeout(() => {
    ignoreEmulatedMouse = false;
    emulatedMouseTimer = null;
  }, EMULATED_MOUSE_WINDOW);
}

/** Watch for touches, for as long as any hover is mounted. */
function watchTouches(): () => void {
  touchWatchers += 1;
  const host: $FlowFixMe = globalThis;
  const document = host.document;
  if (stopWatchingTouches == null && document != null) {
    document.addEventListener("pointerdown", onTouchPointer, true);
    document.addEventListener("pointerup", onTouchPointer, true);
    stopWatchingTouches = () => {
      document.removeEventListener("pointerdown", onTouchPointer, true);
      document.removeEventListener("pointerup", onTouchPointer, true);
      if (emulatedMouseTimer != null) {
        clearTimeout(emulatedMouseTimer);
        emulatedMouseTimer = null;
      }
      ignoreEmulatedMouse = false;
    };
  }
  return () => {
    touchWatchers -= 1;
    if (touchWatchers === 0 && stopWatchingTouches != null) {
      stopWatchingTouches();
      stopWatchingTouches = null;
    }
  };
}

/** A hover beginning or ending. */
export type HoverEvent = {|
  readonly type: "hoverstart" | "hoverend",
  /** A mouse or a pen: a finger has no hover. */
  readonly pointerType: "mouse" | "pen",
  readonly target: HTMLElement,
|};

/** What `useHover` is told. */
export type HoverOptions = {|
  /** No hover; a hover in progress ends. */
  readonly isDisabled?: boolean,
  readonly onHoverChange?: (isHovering: boolean) => mixed,
  readonly onHoverEnd?: (event: HoverEvent) => mixed,
  readonly onHoverStart?: (event: HoverEvent) => mixed,
|};

/** The handlers `useHover` needs on the element. */
export type HoverProps = {|
  readonly onPointerEnter: (event: InteractionEvent) => void,
  readonly onPointerLeave: (event: InteractionEvent) => void,
|};

/** What `useHover` hands back. */
export type HoverResult = {|
  readonly hoverProps: HoverProps,
  readonly isHovered: boolean,
|};

/** A hover in progress. */
type Hovering = {|
  readonly pointerType: "mouse" | "pen",
  readonly stop: () => void,
  readonly target: HTMLElement,
|};

/**
 * Whether a mouse or a pen is over an element — and never a finger.
 *
 *     const { hoverProps, isHovered } = useHover({ onHoverStart: preview });
 *
 * A touch pointer is ignored, and so is a mouse pointer within half a second of
 * a touch; see the module header for the iOS behaviour that makes the second
 * rule necessary.
 */
export hook useHover(options?: HoverOptions): HoverResult {
  const [isHovered, setHovered] = useState(false);
  const hovering = useRef<Hovering | null>(null);

  useEffect(() => watchTouches(), []);

  const end = useStableCallback(() => {
    const current = hovering.current;
    if (current == null) {
      return;
    }
    hovering.current = null;
    current.stop();
    setHovered(false);
    options?.onHoverEnd?.({
      pointerType: current.pointerType,
      target: current.target,
      type: "hoverend",
    });
    options?.onHoverChange?.(false);
  });

  const onPointerEnter = useStableCallback((event: InteractionEvent) => {
    if (options?.isDisabled === true || hovering.current != null) {
      return;
    }
    const pointer = physicalPointerOf(event.pointerType);
    if (pointer === "touch" || (pointer === "mouse" && ignoreEmulatedMouse)) {
      return;
    }
    const target = elementOf(event.currentTarget);
    const document = target.ownerDocument;
    // A pointer that turns up somewhere else without this element hearing it
    // leave — the element moved, or was covered — has left.
    const onPointerElsewhere = (native: $FlowFixMe) => {
      if (!contains(target, native.target)) {
        end();
      }
    };
    document.addEventListener("pointerover", onPointerElsewhere, true);
    hovering.current = {
      pointerType: pointer,
      stop: () => document.removeEventListener("pointerover", onPointerElsewhere, true),
      target,
    };
    setHovered(true);
    options?.onHoverStart?.({ pointerType: pointer, target, type: "hoverstart" });
    options?.onHoverChange?.(true);
  });

  const onPointerLeave = useStableCallback((_event: InteractionEvent) => {
    end();
  });

  const isDisabled = options?.isDisabled === true;
  useEffect(() => {
    if (isDisabled) {
      end();
    }
  }, [isDisabled, end]);

  useEffect(
    () => () => {
      hovering.current?.stop();
      hovering.current = null;
    },
    [],
  );

  const hoverProps = useMemo(
    () => ({ onPointerEnter, onPointerLeave }),
    [onPointerEnter, onPointerLeave],
  );
  return { hoverProps, isHovered };
}

// ---------------------------------------------------------------------------
// useLongPress
// ---------------------------------------------------------------------------

/** A moment in a long press. */
export type LongPressEvent = {|
  readonly type: "longpressstart" | "longpressend" | "longpress",
  readonly pointerType: PhysicalPointer,
  readonly target: HTMLElement,
  readonly altKey: boolean,
  readonly ctrlKey: boolean,
  readonly metaKey: boolean,
  readonly shiftKey: boolean,
  readonly x: number,
  readonly y: number,
|};

/** What `useLongPress` is told. */
export type LongPressOptions = {|
  /**
   * What a reader is told a long press does, as the element's description.
   *
   * A long press is invisible. A component that offers one owes a keyboard way
   * to do the same thing, and this is where it says what that is — "Long press
   * or press Shift+F10 for more actions".
   */
  readonly accessibilityDescription?: string,
  readonly isDisabled?: boolean,
  /** The press lasted long enough. The press underneath is cancelled, and its click refused. */
  readonly onLongPress?: (event: LongPressEvent) => mixed,
  /** The press that might have been a long one ended, whichever it turned out to be. */
  readonly onLongPressEnd?: (event: LongPressEvent) => mixed,
  /** A press began that could become a long press. */
  readonly onLongPressStart?: (event: LongPressEvent) => mixed,
  /**
   * Which pointers a long press may come from; every one of them by default.
   *
   * A context menu's long press is a touch's, because a mouse has a right
   * button for it — and a mouse held down on a row is starting a text
   * selection or a drag, not asking for a menu.
   */
  readonly pointerTypes?: $ReadOnlyArray<PhysicalPointer>,
  /** How long, in milliseconds. 500 by default. */
  readonly threshold?: number,
|};

/** The handlers and the description `useLongPress` needs on the element. */
export type LongPressProps = {|
  ...PressProps,
  readonly "aria-describedby"?: string,
|};

/** What `useLongPress` hands back. */
export type LongPressResult = {|
  readonly longPressProps: LongPressProps,
|};

/** A description element shared by every long press that says the same thing. */
type SharedDescription = {| readonly node: HTMLElement, users: number |};

const descriptions: Map<string, SharedDescription> = new Map();
let descriptionCount = 0;

/**
 * The id of an element holding `text`, once there is one in the document.
 *
 * `hidden`, because a description is read by reference and never shown, and an
 * element referenced by `aria-describedby` is described from even when hidden.
 * The id is only handed out after the element exists, which is the rule this
 * package keeps everywhere: a reference to an id nothing has is announced as
 * nothing at all.
 */
hook useDescription(text: string | void): string | void {
  const [id, setId] = useState<string | void>(undefined);

  useEffect(() => {
    const host: $FlowFixMe = globalThis;
    const body: HTMLElement | null = host.document?.body ?? null;
    if (text == null || text === "" || body == null) {
      // The id is state because it must render only after the shared DOM node exists.
      // uf-lint-disable-next-line react-compiler/set-state-in-effect
      setId(undefined);
      return;
    }
    let shared: SharedDescription | void = descriptions.get(text);
    if (shared == null) {
      descriptionCount += 1;
      const node = body.ownerDocument.createElement("div");
      node.id = `uf-long-press-description-${String(descriptionCount)}`;
      node.hidden = true;
      node.textContent = text;
      body.appendChild(node);
      // Annotated, or the literal's `0` is the type of `users` from here on
      // and counting the next user is a type error.
      const created: SharedDescription = { node, users: 0 };
      descriptions.set(text, created);
      shared = created;
    }
    const entry: SharedDescription = shared;
    entry.users += 1;
    // The id is state because it must render only after the shared DOM node exists.
    // uf-lint-disable-next-line react-compiler/set-state-in-effect
    setId(entry.node.id);
    return () => {
      entry.users -= 1;
      if (entry.users === 0) {
        entry.node.remove();
        descriptions.delete(text);
      }
    };
  }, [text]);

  return id;
}

/**
 * End every press and every drag on `target`: a long press is the whole gesture.
 *
 * A `pointercancel` rather than a call into the other hooks, because the hooks
 * on an element are the caller's to combine and none of them knows about the
 * others; every one of them already ends on this event.
 */
function cancelGesturesOn(
  target: HTMLElement,
  pointerId: number,
  pointerType: PhysicalPointer,
): void {
  const view: $FlowFixMe = target.ownerDocument.defaultView;
  if (view == null) {
    return;
  }
  const cancel =
    typeof view.PointerEvent === "function"
      ? new view.PointerEvent("pointercancel", { bubbles: true, pointerId, pointerType })
      : new view.Event("pointercancel", { bubbles: true });
  if (cancel.pointerId !== pointerId) {
    Object.defineProperty(cancel, "pointerId", { value: pointerId });
  }
  target.dispatchEvent(cancel);
}

/**
 * Refuse the click that ends a long press, so a long press on a link does not follow it.
 *
 * On the document and in the capture phase, so the click reaches nothing inside
 * the element — not a `usePress` beside this hook, not a link's navigation. The
 * next press is where the refusal gives up, for the release that never clicks.
 */
function refuseNextClick(target: HTMLElement): void {
  const document = target.ownerDocument;
  const refuse = (event: $FlowFixMe) => {
    stop();
    if (contains(target, event.target)) {
      event.preventDefault();
      event.stopPropagation();
    }
  };
  const giveUp = () => stop();
  const stop = () => {
    document.removeEventListener("click", refuse, true);
    document.removeEventListener("pointerdown", giveUp, true);
  };
  document.addEventListener("click", refuse, true);
  document.addEventListener("pointerdown", giveUp, true);
}

/**
 * A press held long enough to mean something else.
 *
 *     const { longPressProps } = useLongPress({
 *       accessibilityDescription: "Long press or press Shift+F10 for more actions",
 *       onLongPress: openMenu,
 *       pointerTypes: ["touch", "pen"],
 *     });
 *
 * Merged with a `usePress` on the same element, a long press cancels the press
 * — `onPress` does not follow `onLongPress`. See the module header for why it
 * has no keyboard of its own.
 */
export hook useLongPress(options?: LongPressOptions): LongPressResult {
  const threshold = options?.threshold ?? LONG_PRESS_THRESHOLD;
  const timer = useRef<TimeoutID | null>(null);
  const pointerId = useRef(0);
  const holding = useRef<PressEvent | null>(null);
  const stopRefusingMenu = useRef<(() => void) | null>(null);
  const describedBy = useDescription(
    options?.isDisabled === true || options?.onLongPress == null
      ? undefined
      : options?.accessibilityDescription,
  );

  const settle = useStableCallback(() => {
    if (timer.current != null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    stopRefusingMenu.current?.();
    stopRefusingMenu.current = null;
  });

  const describe = (
    type: "longpressstart" | "longpressend" | "longpress",
    pointerType: PhysicalPointer,
    event: PressEvent,
  ): LongPressEvent => ({
    altKey: event.altKey,
    ctrlKey: event.ctrlKey,
    metaKey: event.metaKey,
    pointerType,
    shiftKey: event.shiftKey,
    target: event.target,
    type,
    x: event.x,
    y: event.y,
  });

  const onPressStart = useStableCallback((event: PressEvent) => {
    const pointerType = event.pointerType;
    if (pointerType !== "mouse" && pointerType !== "pen" && pointerType !== "touch") {
      return;
    }
    if (!(options?.pointerTypes ?? EVERY_POINTER).includes(pointerType)) {
      return;
    }
    settle();
    holding.current = event;
    options?.onLongPressStart?.(describe("longpressstart", pointerType, event));
    const target = event.target;
    if (pointerType === "touch") {
      // A finger held down asks the platform for its own menu at about the
      // same moment; the long press is the answer instead.
      const refuseMenu = (native: $FlowFixMe) => native.preventDefault();
      target.addEventListener("contextmenu", refuseMenu, false);
      stopRefusingMenu.current = () => target.removeEventListener("contextmenu", refuseMenu, false);
    }
    const id = pointerId.current;
    timer.current = setTimeout(() => {
      timer.current = null;
      options?.onLongPress?.(describe("longpress", pointerType, event));
      refuseNextClick(target);
      cancelGesturesOn(target, id, pointerType);
    }, threshold);
  });

  const onPressEnd = useStableCallback((event: PressEvent) => {
    const held = holding.current;
    if (held == null) {
      return;
    }
    holding.current = null;
    settle();
    const pointerType = held.pointerType;
    if (pointerType === "mouse" || pointerType === "pen" || pointerType === "touch") {
      options?.onLongPressEnd?.(describe("longpressend", pointerType, event));
    }
  });

  const { pressProps } = usePress({
    isDisabled: options?.isDisabled,
    onPressEnd,
    onPressStart,
  });

  const onPointerDown = useStableCallback((event: InteractionEvent) => {
    pointerId.current = event.pointerId ?? 0;
    pressProps.onPointerDown(event);
  });

  useEffect(() => settle, [settle]);

  const longPressProps = useMemo(
    () => ({ ...pressProps, "aria-describedby": describedBy, onPointerDown }),
    [pressProps, describedBy, onPointerDown],
  );
  return { longPressProps };
}

// ---------------------------------------------------------------------------
// useMove
// ---------------------------------------------------------------------------

/** The input a move came from. */
export type MovePointerType = PhysicalPointer | "keyboard";

/** A move beginning: the first movement after a pointer went down, or an arrow key. */
export type MoveStartEvent = {|
  readonly type: "movestart",
  readonly pointerType: MovePointerType,
  ...Modifiers,
|};

/** A movement, in pixels for a pointer and in steps of one for a key. */
export type MoveMoveEvent = {|
  readonly type: "move",
  readonly pointerType: MovePointerType,
  /** How far right since the last event; negative is left. */
  readonly deltaX: number,
  /** How far down since the last event; negative is up. */
  readonly deltaY: number,
  ...Modifiers,
|};

/** A move ending. */
export type MoveEndEvent = {|
  readonly type: "moveend",
  readonly pointerType: MovePointerType,
  ...Modifiers,
|};

/** What `useMove` is told. */
export type MoveOptions = {|
  readonly onMove?: (event: MoveMoveEvent) => mixed,
  readonly onMoveEnd?: (event: MoveEndEvent) => mixed,
  readonly onMoveStart?: (event: MoveStartEvent) => mixed,
|};

/** The handlers `useMove` needs on the element. */
export type MoveProps = {|
  readonly onKeyDown: (event: InteractionEvent) => void,
  readonly onPointerDown: (event: InteractionEvent) => void,
|};

/** What `useMove` hands back. */
export type MoveResult = {|
  readonly moveProps: MoveProps,
|};

/** A drag in progress. */
type Dragging = {|
  lastX: number,
  lastY: number,
  moved: boolean,
  readonly pointerId: number,
  readonly pointerType: PhysicalPointer,
  restoreSelection: (() => void) | null,
  readonly stop: () => void,
|};

/**
 * How far a pointer or an arrow key moved something, one event at a time.
 *
 *     const { moveProps } = useMove({ onMove: ({ deltaX }) => resizeBy(deltaX) });
 *
 * A move begins with the first movement rather than with the press, so a click
 * that does not move is not a drag. Physical directions, not reading ones: a
 * caller whose axis runs the other way in a right-to-left page — a slider —
 * turns `deltaX` round itself, because only it knows that its axis does.
 */
export hook useMove(options?: MoveOptions): MoveResult {
  const dragging = useRef<Dragging | null>(null);

  const emitStart = useStableCallback((pointerType: MovePointerType, source: mixed) => {
    options?.onMoveStart?.({ ...modifiersOf(source), pointerType, type: "movestart" });
  });
  const emitMove = useStableCallback(
    (pointerType: MovePointerType, deltaX: number, deltaY: number, source: mixed) => {
      options?.onMove?.({ ...modifiersOf(source), deltaX, deltaY, pointerType, type: "move" });
    },
  );
  const emitEnd = useStableCallback((pointerType: MovePointerType, source: mixed) => {
    options?.onMoveEnd?.({ ...modifiersOf(source), pointerType, type: "moveend" });
  });

  const finish = useStableCallback((native: $FlowFixMe) => {
    const current = dragging.current;
    if (current == null || (native.pointerId ?? 0) !== current.pointerId) {
      return;
    }
    dragging.current = null;
    current.stop();
    if (current.moved) {
      emitEnd(current.pointerType, native);
    }
  });

  const onDocumentPointerMove = useStableCallback((native: $FlowFixMe) => {
    const current = dragging.current;
    if (current == null || (native.pointerId ?? 0) !== current.pointerId) {
      return;
    }
    // A mouse moving with no button held has already let go, somewhere this
    // never heard about: the menu a right click opens swallows the release.
    if (
      current.pointerType === "mouse" &&
      typeof native.buttons === "number" &&
      (native.buttons & 1) === 0
    ) {
      finish(native);
      return;
    }
    const x = typeof native.clientX === "number" ? native.clientX : current.lastX;
    const y = typeof native.clientY === "number" ? native.clientY : current.lastY;
    const deltaX = x - current.lastX;
    const deltaY = y - current.lastY;
    if (deltaX === 0 && deltaY === 0) {
      return;
    }
    current.lastX = x;
    current.lastY = y;
    if (!current.moved) {
      current.moved = true;
      // Only once it is a drag: a click that does not move may still select.
      current.restoreSelection = withoutTextSelection(
        native.target?.ownerDocument?.documentElement ?? null,
      );
      emitStart(current.pointerType, native);
    }
    emitMove(current.pointerType, deltaX, deltaY, native);
  });

  const onPointerDown = useStableCallback((event: InteractionEvent) => {
    const element = elementOf(event.currentTarget);
    if (event.button !== 0 || dragging.current != null || answeredElsewhere(event, element)) {
      return;
    }
    markAnswered(event, element);
    const document = element.ownerDocument;
    document.addEventListener("pointermove", onDocumentPointerMove, false);
    document.addEventListener("pointerup", finish, false);
    document.addEventListener("pointercancel", finish, false);
    const drag: Dragging = {
      lastX: event.clientX ?? 0,
      lastY: event.clientY ?? 0,
      moved: false,
      pointerId: event.pointerId ?? 0,
      pointerType: physicalPointerOf(event.pointerType),
      restoreSelection: null,
      stop: () => {
        document.removeEventListener("pointermove", onDocumentPointerMove, false);
        document.removeEventListener("pointerup", finish, false);
        document.removeEventListener("pointercancel", finish, false);
        drag.restoreSelection?.();
        drag.restoreSelection = null;
      },
    };
    dragging.current = drag;
  });

  const onKeyDown = useStableCallback((event: InteractionEvent) => {
    if (event.defaultPrevented) {
      return;
    }
    const key = event.key;
    const deltaX =
      key === "ArrowLeft" || key === "Left" ? -1 : key === "ArrowRight" || key === "Right" ? 1 : 0;
    const deltaY =
      key === "ArrowUp" || key === "Up" ? -1 : key === "ArrowDown" || key === "Down" ? 1 : 0;
    if (deltaX === 0 && deltaY === 0) {
      return;
    }
    // The arrow moved something, so it neither scrolls the page nor moves a
    // roving focus around this element.
    event.preventDefault();
    event.stopPropagation();
    emitStart("keyboard", event);
    emitMove("keyboard", deltaX, deltaY, event);
    emitEnd("keyboard", event);
  });

  useEffect(
    () => () => {
      dragging.current?.stop();
      dragging.current = null;
    },
    [],
  );

  const moveProps = useMemo(() => ({ onKeyDown, onPointerDown }), [onKeyDown, onPointerDown]);
  return { moveProps };
}

// ---------------------------------------------------------------------------
// useKeyboard
// ---------------------------------------------------------------------------

/** A key, as `useKeyboard` hands it to a handler. */
export type KeyboardInteraction = {|
  readonly type: "keydown" | "keyup",
  readonly key: string,
  readonly code: string,
  readonly repeat: boolean,
  readonly altKey: boolean,
  readonly ctrlKey: boolean,
  readonly metaKey: boolean,
  readonly shiftKey: boolean,
  /** The element the key went to, which may be inside the one listening. */
  readonly target: mixed,
  /** The element listening. */
  readonly currentTarget: HTMLElement,
  readonly isDefaultPrevented: () => boolean,
  readonly preventDefault: () => void,
  /**
   * Let the key reach the elements around this one.
   *
   * Stopping is the default, and there is no `stopPropagation` to call: see
   * the module header.
   */
  readonly continuePropagation: () => void,
|};

/** What `useKeyboard` is told. */
export type KeyboardOptions = {|
  /** Hear nothing and stop nothing. */
  readonly isDisabled?: boolean,
  readonly onKeyDown?: (event: KeyboardInteraction) => mixed,
  readonly onKeyUp?: (event: KeyboardInteraction) => mixed,
|};

/** The handlers `useKeyboard` needs on the element — only the ones it was given. */
export type KeyboardProps = {|
  readonly onKeyDown?: (event: InteractionEvent) => void,
  readonly onKeyUp?: (event: InteractionEvent) => void,
|};

/** What `useKeyboard` hands back. */
export type KeyboardResult = {|
  readonly keyboardProps: KeyboardProps,
|};

/**
 * Keys on an element, stopped there unless a handler passes them on.
 *
 *     const { keyboardProps } = useKeyboard({
 *       onKeyDown: (event) => {
 *         if (event.key === "Delete") remove();
 *         else event.continuePropagation();
 *       },
 *     });
 *
 * A handler that is not given is not attached, so a `useKeyboard` with only
 * `onKeyDown` stops no `keyup`.
 */
export hook useKeyboard(options?: KeyboardOptions): KeyboardResult {
  const route = useStableCallback((event: InteractionEvent, up: boolean) => {
    const handler = up ? options?.onKeyUp : options?.onKeyDown;
    if (handler == null) {
      return;
    }
    let continued = false;
    handler({
      ...modifiersOf(event),
      code: event.code ?? "",
      continuePropagation: () => {
        continued = true;
      },
      currentTarget: elementOf(event.currentTarget),
      isDefaultPrevented: () => event.defaultPrevented,
      key: event.key ?? "",
      preventDefault: () => {
        event.preventDefault();
      },
      repeat: event.repeat === true,
      target: event.target,
      type: up ? "keyup" : "keydown",
    });
    if (!continued) {
      event.stopPropagation();
    }
  });

  const onKeyDown = useStableCallback((event: InteractionEvent) => route(event, false));
  const onKeyUp = useStableCallback((event: InteractionEvent) => route(event, true));

  const disabled = options?.isDisabled === true;
  const hearsDown = options?.onKeyDown != null;
  const hearsUp = options?.onKeyUp != null;
  const keyboardProps = useMemo(
    () =>
      disabled
        ? {}
        : {
            onKeyDown: hearsDown ? onKeyDown : undefined,
            onKeyUp: hearsUp ? onKeyUp : undefined,
          },
    [disabled, hearsDown, hearsUp, onKeyDown, onKeyUp],
  );
  return { keyboardProps };
}

// ---------------------------------------------------------------------------
// mergeProps
// ---------------------------------------------------------------------------

/**
 * Several hooks' props, for one element.
 *
 *     <button {...mergeProps(pressProps, hoverProps, focusProps)}>Save</button>
 *
 * An event handler — a name that is `on` and a capital letter — present in more
 * than one is called in the order given, each of them; a `className` present in
 * more than one is joined; anything else is the last one given, and an
 * `undefined` does not replace what came before it. See the module header for
 * why this is not `internal/merge-props.js`.
 */
export function mergeProps(
  ...sources: $ReadOnlyArray<?{ readonly [string]: mixed }>
): InteractionProps {
  const merged: { key?: empty, [string]: mixed } = {};
  for (const source of sources) {
    if (source == null) {
      continue;
    }
    for (const name of Object.keys(source)) {
      const value = source[name];
      // `key` is React's, and never arrives in props; see `InteractionProps`.
      if (value === undefined || name === "key") {
        continue;
      }
      const before = merged[name];
      if (typeof before === "function" && typeof value === "function" && /^on[A-Z]/.test(name)) {
        merged[name] = chain(before, value);
      } else if (name === "className" && typeof before === "string" && typeof value === "string") {
        merged[name] = `${before} ${value}`;
      } else {
        merged[name] = value;
      }
    }
  }
  return merged;
}

/** Two handlers as one, called in order with the same arguments. */
function chain(first: mixed, second: mixed): (...args: $ReadOnlyArray<mixed>) => void {
  return (...args: $ReadOnlyArray<mixed>) => {
    (first as $FlowFixMe)(...args);
    (second as $FlowFixMe)(...args);
  };
}
