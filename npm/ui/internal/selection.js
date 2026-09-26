// @flow
//
// What a collection's selection becomes after one gesture, as pure functions.
//
// `ListBox`, `GridList`, `Tree` and `TagGroup` all ask the same four questions
// — toggle this key, make it the only one, extend to it, take everything — and
// the answers depend on three settings rather than on which collection is
// asking. Keeping them here, apart from React, is what lets every rule be
// tested without rendering and lets all four collections agree by construction.
//
// The rules are React Aria's selection manager (`@react-stately/selection`),
// which is itself the WAI-ARIA APG's listbox and grid selection models made
// precise:
//
//   * `mode` is what may be chosen: nothing, one key, or any number.
//   * `behavior` is what a plain gesture does. `"toggle"` flips the key — the
//     checkbox model, right for touch and for lists whose rows carry a
//     checkbox. `"replace"` makes the key the whole selection — the file
//     manager model, where Ctrl/Cmd adds one and Shift adds a range.
//   * `disallowEmpty` refuses the gesture that would leave nothing selected,
//     rather than accepting it and then reselecting.
//
// Every function returns the *same* set it was given when nothing changes, so a
// caller can compare by identity and skip `onSelectionChange` for a gesture
// that did nothing — a controlled parent is not told about a change that is
// not one.
//
// # Why this is `internal/`
//
// A public selection manager would be a second way to build a collection, and
// the point of this package is that there is one. The collections are the
// public surface; `npm/ui/index.js` says why the internals stay internal.

export type SelectionMode = "none" | "single" | "multiple";
export type SelectionBehavior = "toggle" | "replace";

/** The three settings every answer below depends on. */
export type SelectionPolicy = {|
  readonly mode: SelectionMode,
  readonly behavior: SelectionBehavior,
  readonly disallowEmpty: boolean,
|};

/**
 * The keys a range covers, in collection order, inclusive at both ends.
 *
 * `order` is the selectable keys only — disabled rows are not in it — so a range
 * dragged across a disabled row does not pick it up, which is what React Aria
 * and every native list do. An end that is not in `order` (a key filtered out
 * since it was chosen) contributes nothing rather than stretching the range to
 * the start of the list.
 */
export function keyRange(
  order: $ReadOnlyArray<string>,
  from: string,
  to: string,
): $ReadOnlyArray<string> {
  const start = order.indexOf(from);
  const end = order.indexOf(to);
  if (start < 0 || end < 0) return end < 0 ? [] : [to];
  return order.slice(Math.min(start, end), Math.max(start, end) + 1);
}

/** Flip one key, unless that would empty a selection that must not be empty. */
export function toggleKey(
  policy: SelectionPolicy,
  selected: $ReadOnlySet<string>,
  key: string,
): $ReadOnlySet<string> {
  if (policy.mode === "none") return selected;
  if (selected.has(key)) {
    if (policy.disallowEmpty && selected.size === 1) return selected;
    if (policy.mode === "single") return new Set();
    const next = new Set(selected);
    next.delete(key);
    return next;
  }
  if (policy.mode === "single") return new Set([key]);
  const next = new Set(selected);
  next.add(key);
  return next;
}

/** Make one key the whole selection. */
export function replaceWith(
  policy: SelectionPolicy,
  selected: $ReadOnlySet<string>,
  key: string,
): $ReadOnlySet<string> {
  if (policy.mode === "none") return selected;
  if (selected.size === 1 && selected.has(key)) return selected;
  return new Set([key]);
}

/**
 * Extend from `anchor` to `key`, replacing the range the previous extension made.
 *
 * `lead` is where the last extension ended. Shift+Down, Shift+Down, Shift+Up
 * must end with two rows selected rather than three, so the old range
 * (`anchor`…`lead`) comes out before the new one (`anchor`…`key`) goes in —
 * while anything selected outside both ranges, by an earlier Ctrl/Cmd+click,
 * stays. With no anchor the extension starts at `key`.
 */
export function extendTo(
  policy: SelectionPolicy,
  selected: $ReadOnlySet<string>,
  order: $ReadOnlyArray<string>,
  anchor: string | null,
  lead: string | null,
  key: string,
): $ReadOnlySet<string> {
  if (policy.mode === "none") return selected;
  if (policy.mode === "single") return replaceWith(policy, selected, key);
  const from = anchor ?? key;
  const next = new Set(selected);
  if (lead != null) for (const each of keyRange(order, from, lead)) next.delete(each);
  for (const each of keyRange(order, from, key)) next.add(each);
  if (next.size === 0 && policy.disallowEmpty) return selected;
  return sameKeys(next, selected) ? selected : next;
}

/** Every selectable key, when more than one may be chosen. */
export function selectAll(
  policy: SelectionPolicy,
  selected: $ReadOnlySet<string>,
  order: $ReadOnlyArray<string>,
): $ReadOnlySet<string> {
  if (policy.mode !== "multiple" || order.length === 0) return selected;
  const next = new Set(selected);
  for (const key of order) next.add(key);
  return sameKeys(next, selected) ? selected : next;
}

/** Nothing, unless nothing is not allowed. */
export function clearAll(
  policy: SelectionPolicy,
  selected: $ReadOnlySet<string>,
): $ReadOnlySet<string> {
  if (policy.mode === "none" || policy.disallowEmpty || selected.size === 0) return selected;
  return new Set();
}

/**
 * A selection as the array `onSelectionChange` reports: collection order first,
 * then any key the collection is not showing, in the order it was chosen.
 *
 * Keys the collection is not showing are real — a child of a collapsed tree
 * row, a row a filter hid — and dropping them because they are out of sight
 * would deselect something the reader chose and cannot see go.
 */
export function orderedKeys(
  selected: $ReadOnlySet<string>,
  order: $ReadOnlyArray<string>,
): $ReadOnlyArray<string> {
  const result = [];
  const placed = new Set<string>();
  for (const key of order) {
    if (selected.has(key)) {
      result.push(key);
      placed.add(key);
    }
  }
  for (const key of selected) if (!placed.has(key)) result.push(key);
  return result;
}

function sameKeys(a: $ReadOnlySet<string>, b: $ReadOnlySet<string>): boolean {
  if (a.size !== b.size) return false;
  for (const key of a) if (!b.has(key)) return false;
  return true;
}
