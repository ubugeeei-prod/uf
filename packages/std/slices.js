// @flow
//
// `@uniflowed/std/slices`: Go's `sort.Search` and `slices.BinarySearch`.
//
// JavaScript has a stable full sort and no binary search primitive. That gap is
// small enough that people rewrite it at the call site, and large enough that
// the rewritten version is often off by one at the insertion point, misses the
// first duplicate, or silently overflows through `>> 1` on a large array.
//
// This module keeps the three useful facts together:
//
// * [`search`] is the generic monotone-predicate primitive from Go's
//   `sort.Search`: first true index, or `length` when none is true.
// * [`binarySearchBy`] is the lower-bound form: the comparator says how the
//   item at `index` relates to the target value the caller has in mind.
// * [`binarySearch`] is the ordinary "sorted array plus target" wrapper.
//
// All three return insertion points rather than `-1`. That is the useful
// answer for sorted data: when a value is absent, the returned index is where it
// belongs if the caller wants to insert it and keep the array sorted.

/**
 * Where a value is, or where it belongs.
 *
 * `found` is true when `index` names an existing item equal to the target. When
 * it is false, `index` is the insertion point in the half-open range
 * `[0, items.length]`.
 */
export type SearchResult = {
  readonly index: number,
  readonly found: boolean,
};

/**
 * Compare two values for sorted order.
 *
 * Same contract as `Array.prototype.sort`: negative means `left` comes before
 * `right`, positive means after, zero means equal.
 */
export type Compare<T> = (left: T, right: T) => number;

/**
 * Return the first index in `[0, length)` whose predicate is true.
 *
 * If the predicate is false everywhere, the answer is `length`. The predicate
 * must be monotone — false for some prefix and true for the rest — which is the
 * same precondition Go documents and the same precondition every binary search
 * has. A non-monotone predicate still terminates, but the answer only describes
 * the partition the predicate claimed to have.
 */
export function search(length: number, predicate: (index: number) => boolean): number {
  assertSearchLength(length);

  let low = 0;
  let high = length;
  while (low < high) {
    // `Math.floor` rather than `>> 1`: bitwise operators coerce through signed
    // 32-bit integers, which is exactly the overflow this helper exists to keep
    // out of call sites.
    const middle = low + Math.floor((high - low) / 2);
    if (predicate(middle)) {
      high = middle;
    } else {
      low = middle + 1;
    }
  }
  return low;
}

/**
 * Binary-search a sorted array with a comparator against one captured target.
 *
 * The comparator receives an item and returns how it relates to the target:
 * negative when the item is before it, positive when after it, zero when equal.
 * The first equal item is returned, so duplicates behave like a lower bound.
 */
export function binarySearchBy<T>(
  items: $ReadOnlyArray<T>,
  compareItemToTarget: (item: T) => number,
): SearchResult {
  const index = search(items.length, (at) => compareItemToTarget(items[at]) >= 0);
  const found = index < items.length && compareItemToTarget(items[index]) === 0;
  return { index, found };
}

/**
 * Binary-search a sorted array for `target`.
 *
 * `compare` is the same comparator the array was sorted with. If `target` is
 * absent, the returned index is where it can be inserted without disturbing the
 * order.
 */
export function binarySearch<T>(
  items: $ReadOnlyArray<T>,
  target: T,
  compare: Compare<T>,
): SearchResult {
  return binarySearchBy(items, (item) => compare(item, target));
}

function assertSearchLength(length: number): void {
  if (!Number.isSafeInteger(length) || length < 0) {
    throw new RangeError(
      `@uniflowed/std/slices: search length must be a non-negative safe integer, got ${String(
        length,
      )}`,
    );
  }
}
