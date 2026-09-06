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
// Five rules make it up, and each one has a way of being got wrong that no
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
//   * **Disabled is skipped, not landed on** — in a set with a roving tab stop,
//     which is not all of them; `moveTo` says which and why. And the direction
//     to keep searching in cannot be inferred from the target index: `End` aims
//     at the last item and, if that one is disabled, has to walk *backwards*.
//     Guessing "forwards, because the target is ahead of us" wrapped `End`
//     around to the first item.
//   * **Typeahead.** Pressing `r` in a menu goes to Refresh. Without it a menu
//     of thirty items is thirty arrow presses, and every native menu on every
//     platform has had this since before the web.
//   * **The horizontal arrows point at the reader's "next", not at the west.**
//     In a right-to-left page the first item of a row is the rightmost one, so
//     `ArrowLeft` is *next* and `ArrowRight` is *previous*. Hard-coding the
//     left-to-right answer renders identically and walks an Arabic, Hebrew,
//     Persian or Urdu reader backwards through every set in this package.
//
// # Why this is `internal/` and not a subpath
//
// It is a description of DOM structure this package owns — that a tab lives
// under `[role="tablist"]`, that a menu item's owner is `[role="menu"]` — and
// those relationships are only guaranteed because the components in this
// package build them. Handed to a consumer it would be a set of selectors that
// happen to work today, which is a different and much weaker promise than the
// one the components make.

import { useCallback, useEffect, useRef, useState } from "@uniflowed/react";

/** Which way a key asks the focus to move within a set. */
export type Movement = "previous" | "next" | "first" | "last";

/** The axis a set's arrow keys run along. */
export type Orientation = "horizontal" | "vertical";

/** Which way the inline axis runs where a set sits: the reader's direction. */
export type Direction = "ltr" | "rtl";

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
 * Which way the page reads where `element` sits.
 *
 * Every caller asks from inside a `keydown` handler, which is a moment where
 * reading the document is legitimate — so no direction prop, no context and no
 * provider: the answer is already in the DOM, and asking it there means a
 * component nested under someone else's `dir="rtl"` is right without anybody
 * having had to thread a value down to it.
 *
 * # Two questions, because one of them is not answered everywhere
 *
 * `getComputedStyle(element).direction` is the whole answer in a browser: the
 * HTML user-agent stylesheet carries `[dir="rtl" i] { direction: rtl }`, so the
 * computed value accounts for a `dir` attribute on any ancestor *and* for a CSS
 * `direction` a caller wrote, which an attribute walk alone would miss. It is
 * what this was written as first, and it is wrong on its own here: `happy-dom`,
 * the DOM this package's own tests run in, ships no user-agent stylesheet for
 * `[dir]`, so a button inside `<div dir="rtl">` computes `ltr` and every RTL
 * test passed for the wrong reason. `element.matches(":dir(rtl)")` — the
 * pseudo-class HTML defines directionality against — answers `false` there too,
 * silently, which makes it the worse of the two to rely on.
 *
 * So the `dir` attribute is asked first and the computed style second, and the
 * order is the useful one rather than a workaround: `dir` is HTML's own
 * statement about directionality and the thing an RTL page actually sets, while
 * `closest` stops at the *nearest* ancestor that carries one, so a `dir="ltr"`
 * island inside a `dir="rtl"` page reads as `ltr`. `dir="auto"` is deliberately
 * not an answer — it means "work it out from the content", which only the
 * layout engine can do — so it falls through to the computed style, where a
 * browser has already worked it out.
 *
 * The cost is one `closest` per arrow key press, plus one `getComputedStyle` on
 * a page that declares no `dir` at all — which is most left-to-right pages.
 * Both are paid at the rate a person presses arrow keys, so neither was worth
 * caching behind a context that could then be stale.
 */
export function directionOf(element: HTMLElement): Direction {
  const declared = element.closest("[dir]")?.getAttribute("dir")?.toLowerCase();
  if (declared === "rtl" || declared === "ltr") {
    return declared;
  }
  const style: $FlowFixMe = element.ownerDocument?.defaultView?.getComputedStyle?.(element);
  return style?.direction === "rtl" ? "rtl" : "ltr";
}

/**
 * The movement a key asks for along `orientation`, or nothing if it is not ours.
 *
 * The unhandled keys matter as much as the handled ones. `ArrowDown` inside a
 * *horizontal* tab list belongs to the page — it scrolls — and a component that
 * swallows it has taken a key away from every reader who uses it to read.
 *
 * `direction` mirrors the horizontal pair and nothing else. `ArrowUp` and
 * `ArrowDown` are unaffected because a right-to-left page still runs top to
 * bottom, and `Home` and `End` are unaffected because they name the first and
 * last item in *reading* order, which is what `moveTo` already walks: in an RTL
 * row the first item is the rightmost one, and `Home` should go to it.
 */
export function movementFor(
  key: string,
  orientation: Orientation,
  direction: Direction,
): Movement | null {
  const rtl = direction === "rtl";
  return match (key) {
    "Home" => "first",
    "End" => "last",
    "ArrowUp" => orientation === "vertical" ? "previous" : null,
    "ArrowDown" => orientation === "vertical" ? "next" : null,
    "ArrowLeft" => orientation === "horizontal" ? (rtl ? "next" : "previous") : null,
    "ArrowRight" => orientation === "horizontal" ? (rtl ? "previous" : "next") : null,
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
 * `skipDisabled` is true for every set with a roving tab stop, where an
 * unavailable item is announced and stepped over. It is false for an accordion,
 * and that is not a preference: an accordion's headers are ordinary buttons in
 * the page's tab order, so `Tab` reaches every one of them, and arrow keys that
 * stepped over one would disagree with `Tab` about which headers exist. The
 * item they would step over is the open section's own header, which
 * `aria-disabled` marks as "pressing this closes nothing" rather than "there is
 * nothing here".
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
  skipDisabled?: boolean = true,
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
    if (!skipDisabled || isEnabled(candidate)) {
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
 * Which items a container owns and how the keyboard runs across them.
 *
 * The two selectors are the pair `itemsOf` needs — what an item is, and what
 * owns one — kept together because giving a set only the first of them is how a
 * nested set steals its parent's items.
 */
export type RovingSet = {|
  readonly item: string,
  readonly owner: string,
  readonly orientation: Orientation,
  /** Whether running off the end cycles or stops. */
  readonly wrap: boolean,
  /** Whether an `aria-disabled` item is stepped over; see `moveTo`. */
  readonly skipDisabled: boolean,
|};

/** The part of a key event a set reads, and the right to claim the key. */
type KeyPress = {
  readonly key: string,
  readonly preventDefault: () => mixed,
  ...
};

/**
 * Move focus within `container` for one key press, and say where it went.
 *
 * Returns the item focus moved to, or null when the key was not one of the
 * set's — `ArrowDown` in a horizontal set, a letter, `Tab` — or when there was
 * nowhere for it to go. A null answer is a key the caller has not claimed, so
 * the page still gets it.
 *
 * This is the whole of the container half of a roving tab stop, written once
 * because the order of the last three lines is not obvious and getting it wrong
 * is invisible: the key has to be claimed *before* focus moves, or the browser
 * scrolls the page under the item that has just taken focus, and the reader
 * ends up looking somewhere else entirely. Every set in this package that is
 * only arrows — a tab list, a radio group, a toggle group — is this function
 * plus what it does with the answer. `Menu.Body` is deliberately not: its keys
 * interleave with `Escape`, `Tab` and typeahead, and it has to stop events
 * propagating between nested menus, which is a different job.
 */
export function moveOnKey(
  event: KeyPress,
  container: HTMLElement,
  set: RovingSet,
): HTMLElement | null {
  const movement = movementFor(event.key, set.orientation, directionOf(container));
  if (movement == null) {
    return null;
  }
  const items = itemsOf(container, set.item, set.owner);
  const next = moveTo(
    items,
    indexOfActive(items, container.ownerDocument?.activeElement),
    movement,
    set.wrap,
    set.skipDisabled,
  );
  if (next == null) {
    return null;
  }
  event.preventDefault();
  next.focus();
  return next;
}

/**
 * The id of the first item the keyboard may land on, or null for none.
 *
 * This answers the one question a roving set cannot answer during a render:
 * which item holds the tab stop before anything has claimed it. A tab list
 * never has that state, because a selection is required — but a radio group
 * with nothing chosen does, and a toggle group nobody has focused does, and
 * getting it wrong is not a cosmetic loss: with no item at `tabindex="0"` the
 * whole set is unreachable by `Tab`, which is the failure worth the machinery.
 *
 * It is a fact about the document, so it is read from the document in an effect
 * and put in state because a render depends on the answer — the rule
 * `index.js` states for the package. `wanted` turns it off: the moment
 * something is chosen or focused, that item holds the tab stop and this is work
 * with no reader.
 *
 * The effect has no dependency array on purpose. What comes first changes when
 * the caller renders a different set of items or disables one, and neither of
 * those is anything this hook is handed — a dependency list here would be a
 * claim about when the document changes that only the caller could keep, and it
 * would be wrong exactly when a caller made their first item conditional. The
 * cost is one `querySelectorAll` over a set that is small by construction, only
 * while nothing is chosen; `setState` with an unchanged id renders nothing.
 */
export hook useFirstItem(
  container: { current: HTMLElement | null },
  set: RovingSet,
  wanted: boolean,
): string | null {
  const [first, setFirst] = useState<string | null>(null);

  useEffect(() => {
    const root = container.current;
    if (!wanted || root == null) {
      return;
    }
    // `moveTo` rather than `items[0]`, so a disabled first item is stepped over
    // here exactly as the arrow keys step over it: a group whose first choice
    // is unavailable must still be reachable.
    const landing = moveTo(
      itemsOf(root, set.item, set.owner),
      -1,
      "first",
      false,
      set.skipDisabled,
    );
    setFirst(landing?.id ?? null);
  });

  return wanted ? first : null;
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
