// @flow
//
// The keyboard pattern shared by every list of things in this package.
//
// A tab list, a menu and a listbox look nothing alike and behave identically at
// the keyboard, because WAI-ARIA says they must: the *set* takes one stop in the
// page's tab order, and the arrow keys move within it. That is what makes a
// twelve-item menu something a keyboard user passes in one Tab press instead of
// twelve, and it is the part hand-written components leave out.
//
// Four rules make it up, and each one has a way of being got wrong that no
// screenshot shows:
//
//   * **Document order, read from the document.** Items are found by querying
//     the container at the moment a key is pressed, not from a registry the
//     items push themselves into as they mount. Mount order is not document
//     order the moment a list is filtered, reordered, or has a conditional item
//     in the middle of it — and a registry that disagrees with the page sends
//     the arrow keys somewhere the reader is not.
//   * **Nesting.** A submenu's items are inside its parent menu's element, so
//     "the items of this menu" cannot be `querySelectorAll` alone. An item
//     belongs to the nearest container of its own kind.
//   * **Disabled is skipped, not landed on.** And the direction to keep
//     searching in cannot be inferred from the target index: `End` aims at the
//     last item and, if that one is disabled, has to walk *backwards*. Guessing
//     "forwards, because the target is ahead of us" wrapped `End` around to the
//     first item.
//   * **Typeahead.** Pressing `r` in a menu goes to Refresh. Without it a menu
//     of thirty items is thirty arrow presses, and every native menu on every
//     platform has had this since before the web.
//
// # Why this is `internal/` and not a subpath
//
// It is a description of DOM structure this package owns — that a tab lives
// under `[role="tablist"]`, that a menu item's owner is `[role="menu"]` — and
// those relationships are only guaranteed because the components in this
// package build them. Handed to a consumer it would be a set of selectors that
// happen to work today, which is a different and much weaker promise than the
// one the components make.

import { useCallback, useRef } from "@uniflowed/react";

/** Which way a key asks the focus to move within a set. */
export type Movement = "previous" | "next" | "first" | "last";

/** The axis a set's arrow keys run along. */
export type Orientation = "horizontal" | "vertical";

/** How long a typeahead buffer survives without another key, in milliseconds. */
const TYPEAHEAD_WINDOW = 500;

/**
 * The items directly belonging to `container`, in document order.
 *
 * `owner` names the container's own kind — `[role="menu"]` for a menu — so an
 * item inside a *nested* container of that kind is left to the nested one. A
 * plain `querySelectorAll` returns a submenu's items as if they were the parent
 * menu's, which makes `ArrowDown` in the parent step into a menu the reader
 * cannot see.
 */
export function itemsOf(container: HTMLElement, item: string, owner: string): Array<HTMLElement> {
  return Array.from(container.querySelectorAll(item)).filter(
    (element: $FlowFixMe) => element.closest(owner) === container,
  );
}

/**
 * Whether the keyboard may land on this item.
 *
 * Both spellings, because the two mean different things and this package uses
 * both: a native `disabled` takes an element out of the accessibility tree's
 * reach, while `aria-disabled` leaves it announced — which is what a menu item
 * or a tab wants, so a reader can tell the option exists and is unavailable
 * rather than finding a gap where it used to be.
 */
export function isEnabled(element: HTMLElement): boolean {
  return (
    (element as $FlowFixMe).disabled !== true && element.getAttribute("aria-disabled") !== "true"
  );
}

/**
 * The movement a key asks for along `orientation`, or nothing if it is not ours.
 *
 * The unhandled keys matter as much as the handled ones. `ArrowDown` inside a
 * *horizontal* tab list belongs to the page — it scrolls — and a component that
 * swallows it has taken a key away from every reader who uses it to read.
 */
export function movementFor(key: string, orientation: Orientation): Movement | null {
  return match (key) {
    "Home" => "first",
    "End" => "last",
    "ArrowUp" => orientation === "vertical" ? "previous" : null,
    "ArrowDown" => orientation === "vertical" ? "next" : null,
    "ArrowLeft" => orientation === "horizontal" ? "previous" : null,
    "ArrowRight" => orientation === "horizontal" ? "next" : null,
    _ => null,
  };
}

/**
 * The item `movement` reaches from `from`, skipping disabled ones.
 *
 * `from` may be `-1` for "nothing is focused yet", which is what makes
 * `ArrowDown` on a freshly opened menu land on the first item. `wrap` is false
 * for a set where running off the end should stop rather than cycle.
 *
 * Returns null when every item is disabled, or when the ends are closed and
 * there is nothing further in that direction — in both cases the caller should
 * leave focus where it is rather than move it somewhere arbitrary.
 */
export function moveTo(
  items: $ReadOnlyArray<HTMLElement>,
  from: number,
  movement: Movement,
  wrap: boolean,
): HTMLElement | null {
  const count = items.length;
  if (count === 0) {
    return null;
  }
  // Two things this expression is careful about, each of which was a bug.
  //
  // The direction is part of the answer rather than derived from it: `last`
  // aims at the end and searches *backwards* from there, and deriving
  // "forwards" from the target being ahead of `from` sent `End` past the end
  // and around to the first item whenever the last one was disabled.
  //
  // And `from` is -1 when nothing is focused yet, which the two directions read
  // differently: "next" from nowhere is the first item, and "previous" from
  // nowhere is the *last* one. Letting -1 fall through the arithmetic aimed
  // `previous` at -2, which wraps to `count - 2` — so `ArrowUp` on a freshly
  // opened list landed one short of the end, and on a two-item list landed on
  // the first item.
  const aim = match (movement) {
    "previous" => [from < 0 ? count - 1 : from - 1, -1],
    "next" => [from + 1, 1],
    "first" => [0, 1],
    "last" => [count - 1, -1],
  };
  const [target, direction] = aim;

  for (let tried = 0; tried < count; tried += 1) {
    const at = target + tried * direction;
    if (!wrap && (at < 0 || at >= count)) {
      return null;
    }
    const candidate = items[((at % count) + count) % count];
    if (isEnabled(candidate)) {
      return candidate;
    }
  }
  return null;
}

/** The index of the focused item, or `-1` when focus is elsewhere. */
export function indexOfActive(items: $ReadOnlyArray<HTMLElement>, active: mixed): number {
  return items.findIndex((item) => item === active);
}

/**
 * Match items by the characters a reader types, the way every native menu does.
 *
 * The returned function is stable, so a component may pass it straight to a key
 * handler without re-subscribing anything. The buffer lives in a ref and is only
 * ever touched from an event handler — never during a render, where a value that
 * depends on how many times React chose to render is a bug waiting for
 * Strict Mode to find it.
 *
 * Two behaviours people notice when they are missing:
 *
 *   * Typing `s`, `a`, `v` within half a second looks for "sav", not for three
 *     separate items starting with `s`, `a` and `v`.
 *   * Pressing the *same* letter repeatedly cycles through the items starting
 *     with it, which is how a reader reaches the second "Save as…".
 */
export hook useTypeahead(): (
  items: $ReadOnlyArray<HTMLElement>,
  from: number,
  key: string,
) => HTMLElement | null {
  const buffer = useRef<{| text: string, at: number |}>({ text: "", at: 0 });

  return useCallback(
    (items: $ReadOnlyArray<HTMLElement>, from: number, key: string): HTMLElement | null => {
      const now = Date.now();
      const text = now - buffer.current.at > TYPEAHEAD_WINDOW ? key : buffer.current.text + key;
      buffer.current = { text, at: now };

      const repeated = text.length > 1 && text.split("").every((each) => each === text[0]);
      const needle = (repeated ? text[0] : text).toLowerCase();
      // A single character — or the same one again — moves on from where we
      // are. A longer buffer starts *at* the current item, so typing "sa" after
      // "s" can keep the item "s" already found.
      const start = repeated || text.length === 1 ? from + 1 : Math.max(from, 0);

      for (let tried = 0; tried < items.length; tried += 1) {
        const candidate = items[(((start + tried) % items.length) + items.length) % items.length];
        if (isEnabled(candidate) && labelOf(candidate).startsWith(needle)) {
          return candidate;
        }
      }
      return null;
    },
    [],
  );
}

/**
 * Whether a key press is a character a reader meant to type.
 *
 * Modifier combinations are excluded because `Ctrl+P` is the browser's, and a
 * component that treats it as "the letter p" both steals the shortcut and jumps
 * the selection somewhere the reader did not ask for.
 */
export function isTypeaheadKey(event: {
  readonly key: string,
  readonly altKey?: boolean,
  readonly ctrlKey?: boolean,
  readonly metaKey?: boolean,
  ...
}): boolean {
  return (
    event.key.length === 1 &&
    event.key !== " " &&
    event.altKey !== true &&
    event.ctrlKey !== true &&
    event.metaKey !== true
  );
}

/** What a reader hears for this item, lower-cased for matching. */
function labelOf(element: HTMLElement): string {
  const spoken = element.getAttribute("aria-label") ?? element.textContent ?? "";
  return spoken.replace(/\s+/g, " ").trim().toLowerCase();
}
