// @flow
//
// `@uniflowed/hooks/keyboard`: a chord is not an event.
//
// This is the third subject in the package that is neither one element nor the
// ambient environment, and it earns a file because none of the difficulty is
// in the listening. `document.addEventListener("keydown", …)` is one line; the
// four things after it are what people get wrong, and they get them wrong the
// same way in every project:
//
// **A combination is not a key.** `⌘K` is a `keydown` whose `key` is `"k"` and
// whose `metaKey` is true, and a handler that checks only the first fires on a
// plain `k` — so a search box opens while somebody is typing a name. Checking
// the modifiers that *are* named is half of it; refusing the ones that are not
// is the other half, and it is the half that gets left out.
//
// **`mod` is not `ctrl`.** The shortcut is ⌘K on a Mac and Ctrl+K everywhere
// else. A library that makes the caller write two bindings makes every caller
// write the same platform test.
//
// **A shortcut inside a text field is a keystroke.** `?` should open help,
// except while somebody is typing a question mark into a comment. The default
// here is that a chord does not fire while the reader is typing, and a caller
// who means it to says so.
//
// **A held key repeats.** Holding a key sends `keydown` twenty times a second.
// A shortcut that opens a dialog should open it once.
//
// # What belongs in this module
//
// A hook whose subject is what is being pressed: a chord, and whether a key is
// down. Not the key events of one element — `useEventListener(ref, "keydown",
// …)` in `dom.js` is that, and it is the right tool when the subject really is
// the element. These listen on the document by default, because a shortcut
// belongs to the page rather than to whatever happens to have focus.
//
// # Before hydration
//
// Nothing. Both hooks do their work in effects, `useKeyHeld` reports `false`
// on a server and in the first client render, and the platform test that
// decides what `mod` means is made inside the effect rather than during a
// render — so nothing here can differ between the two passes React compares.

import { useEffect, useState } from "@uniflowed/react";

import { browserWindow } from "./browser.js";
import type { Ref } from "./dom.js";
import { useStableCallback } from "./lifecycle.js";

/** A combination, once its spelling has been resolved. */
type Chord = {|
  readonly key: string,
  readonly ctrl: boolean,
  readonly meta: boolean,
  readonly alt: boolean,
  readonly shift: boolean,
  /** Ctrl, or Command on Apple platforms. */
  readonly mod: boolean,
|};

/**
 * The spellings people actually write, mapped to the one `KeyboardEvent.key`
 * uses.
 *
 * `key` reports the character produced, so the arrows are `"ArrowUp"` and the
 * space bar is a literal space — neither of which anybody writes in a
 * shortcut.
 */
const KEY_ALIASES: { readonly [string]: string } = {
  esc: "escape",
  space: " ",
  spacebar: " ",
  ret: "enter",
  return: "enter",
  up: "arrowup",
  down: "arrowdown",
  left: "arrowleft",
  right: "arrowright",
  del: "delete",
  plus: "+",
};

/**
 * Read `"mod+shift+k"` into a chord.
 *
 * Unknown modifier names are treated as the key rather than rejected: a typo
 * produces a shortcut that never fires, which the author notices, and a throw
 * during a render would take the page down instead.
 */
function parseChord(combo: string): Chord {
  let ctrl = false;
  let meta = false;
  let alt = false;
  let shift = false;
  let mod = false;
  let key = "";

  for (const raw of combo.split("+")) {
    const part = raw.trim().toLowerCase();
    if (part === "") {
      // `"shift++"` is shift and the plus key, and splitting leaves a hole.
      key = "+";
    } else if (part === "ctrl" || part === "control") {
      ctrl = true;
    } else if (part === "cmd" || part === "command" || part === "meta" || part === "super") {
      meta = true;
    } else if (part === "alt" || part === "opt" || part === "option") {
      alt = true;
    } else if (part === "shift") {
      shift = true;
    } else if (part === "mod") {
      mod = true;
    } else {
      key = KEY_ALIASES[part] ?? part;
    }
  }

  return { key, ctrl, meta, alt, shift, mod };
}

/**
 * Whether `mod` means Command here.
 *
 * Read from the user agent because that is the only thing every browser
 * agrees on: `navigator.platform` is deprecated and frozen, and
 * `userAgentData` exists in Chromium alone. Called from inside an effect, so a
 * server render never asks.
 */
function onApple(): boolean {
  const agent = browserWindow()?.navigator.userAgent ?? "";
  return /mac|iphone|ipad|ipod/i.test(agent);
}

/** Whether the event landed in something the reader is typing into. */
function typing(target: EventTarget): boolean {
  if (typeof HTMLElement === "undefined" || !(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable) {
    return true;
  }
  const tag = target.tagName.toLowerCase();
  return tag === "input" || tag === "textarea" || tag === "select";
}

/**
 * Whether a key is one that shift is needed to type.
 *
 * A single character that is neither a letter nor a digit: `"?"`, `"+"`,
 * `"!"`. `KeyboardEvent.key` reports the character *produced*, so those arrive
 * with `shiftKey` true on a US layout and false on layouts where they have
 * their own key.
 */
function shiftedSymbol(key: string): boolean {
  return key.length === 1 && !/[a-z0-9]/.test(key);
}

/**
 * Whether `event` is this chord.
 *
 * The modifiers named must be down and the ones not named must be up. That
 * second half is what makes `"mod+k"` refuse Ctrl+Shift+K, which is a
 * different shortcut somebody else has probably bound.
 *
 * Shift is the one exception, and only for a key that shift is needed to
 * type: `"?"` is the most common single-key shortcut on the web and arrives
 * with `shiftKey` true on a US layout and false on a German one, so requiring
 * either would make it unbindable on half the keyboards in the world. For a
 * letter, a digit or a named key it is checked like the rest.
 */
function isChord(chord: Chord, event: KeyboardEvent, apple: boolean): boolean {
  if (event.key.toLowerCase() !== chord.key) {
    return false;
  }
  const meta = chord.meta || (chord.mod && apple);
  const ctrl = chord.ctrl || (chord.mod && !apple);
  if (event.metaKey !== meta || event.ctrlKey !== ctrl || event.altKey !== chord.alt) {
    return false;
  }
  if (chord.shift) {
    return event.shiftKey;
  }
  return shiftedSymbol(chord.key) || !event.shiftKey;
}

/** `event` as a keyboard event, or `null` for anything else. */
function asKeyboardEvent(event: Event): KeyboardEvent | null {
  if (typeof KeyboardEvent === "undefined" || !(event instanceof KeyboardEvent)) {
    return null;
  }
  return event;
}

/** How a chord is listened for. */
export type KeyComboOptions = {|
  /** Listen on this element instead of the document. */
  readonly target?: Ref<HTMLElement> | null,
  /** Fire even while the reader is typing into a field. Off by default. */
  readonly whileTyping?: boolean,
  /** Fire again while the key is held down. Off by default. */
  readonly repeat?: boolean,
  /** Call `preventDefault` when it fires. On by default, since ⌘K is the browser's too. */
  readonly preventDefault?: boolean,
  /** Turn the binding off without changing where the hook is called. */
  readonly enabled?: boolean,
|};

/**
 * Call `handler` when a key combination is pressed.
 *
 * ```js
 * useKeyCombo("mod+k", () => setSearchOpen(true));
 * useKeyCombo("escape", close, { whileTyping: true });
 * ```
 *
 * `preventDefault` defaults to on because the combinations worth binding are
 * the ones the browser also wants — ⌘K is the address bar in Chrome, ⌘S is
 * Save Page — and a shortcut that fires *and* opens a browser dialog is worse
 * than either alone. A chord that is only the application's, like `escape`,
 * loses nothing by it.
 */
export hook useKeyCombo(
  combo: string,
  handler: (event: KeyboardEvent) => mixed,
  options?: KeyComboOptions,
): void {
  const stable = useStableCallback(handler);
  const target = options?.target ?? null;
  const whileTyping = options?.whileTyping ?? false;
  const repeat = options?.repeat ?? false;
  const preventDefault = options?.preventDefault ?? true;
  const enabled = options?.enabled ?? true;

  useEffect(() => {
    const win = browserWindow();
    if (!enabled || win == null) {
      return;
    }
    // A ref that is given and empty means the element is not there yet, which
    // is not the same as "no element was asked for": falling back to the
    // document would bind the shortcut to the whole page by accident.
    const node = target == null ? win.document : target.current;
    if (node == null) {
      return;
    }

    const chord = parseChord(combo);
    const apple = onApple();

    const listener = (event: Event) => {
      const key = asKeyboardEvent(event);
      if (key == null || (!repeat && key.repeat)) {
        return;
      }
      if (!whileTyping && typing(key.target)) {
        return;
      }
      if (!isChord(chord, key, apple)) {
        return;
      }
      if (preventDefault) {
        key.preventDefault();
      }
      stable(key);
    };

    node.addEventListener("keydown", listener);
    return () => node.removeEventListener("keydown", listener);
  }, [combo, enabled, target, whileTyping, repeat, preventDefault, stable]);
}

/**
 * Whether a key is being held down.
 *
 * For the case a chord cannot express: a modifier that changes what a drag
 * does while it is held, a space bar that pans a canvas. `key` is matched
 * against `KeyboardEvent.key`, case-insensitively, so `"shift"`, `"escape"`
 * and `" "` all work.
 *
 * The `blur` reset is the reason this is a hook rather than two listeners.
 * Holding a key and switching windows sends the `keyup` to the other window,
 * so a hand-written version leaves the key held forever — the canvas stays
 * panning after the reader comes back. Losing focus releases everything.
 */
export hook useKeyHeld(
  key: string,
  options?: {| readonly target?: Ref<HTMLElement> | null |},
): boolean {
  const [held, setHeld] = useState(false);
  const target = options?.target ?? null;
  const wanted = key.toLowerCase();

  useEffect(() => {
    const win = browserWindow();
    if (win == null) {
      return;
    }
    const node = target == null ? win.document : target.current;
    if (node == null) {
      return;
    }

    const down = (event: Event) => {
      const pressed = asKeyboardEvent(event);
      if (pressed != null && pressed.key.toLowerCase() === wanted) {
        setHeld(true);
      }
    };
    const up = (event: Event) => {
      const released = asKeyboardEvent(event);
      if (released != null && released.key.toLowerCase() === wanted) {
        setHeld(false);
      }
    };
    const release = () => setHeld(false);

    node.addEventListener("keydown", down);
    node.addEventListener("keyup", up);
    win.addEventListener("blur", release);
    return () => {
      node.removeEventListener("keydown", down);
      node.removeEventListener("keyup", up);
      win.removeEventListener("blur", release);
    };
  }, [wanted, target]);

  return held;
}
