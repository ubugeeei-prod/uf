// @flow
//
// A value in a range: the arithmetic, and which way the arrow keys move it.
//
// Three components in this package report a number between two others —
// `slider.js`, `resizable.js` and `progress.js` — and all three make the same
// four promises through `aria-valuemin`, `aria-valuemax`, `aria-valuenow` and
// `aria-valuetext`. A reader is told a number, and the number has to be true:
// a thumb announced as 73 that the next `ArrowRight` moves to 75 has told them
// the step is 2 when it is 1, and a value announced outside its own bounds has
// told them the control is broken.
//
// So the arithmetic lives once. It is four small functions, and each of them
// is a rule that was got wrong somewhere before it was written down:
//
//   * **Snapping is measured from the minimum, not from zero.** A slider from
//     5 to 100 in steps of 10 has values 5, 15, 25 — not 10, 20, 30. Rounding
//     `value / step` produces the second list, and the reader who presses
//     `Home` then `ArrowRight` lands on 10 from a minimum of 5, which is a
//     first step of 5 on a slider that says its step is 10.
//   * **Clamping happens after snapping.** Snapping a value near the top can
//     push it past the maximum, and a slider whose `aria-valuenow` is greater
//     than its `aria-valuemax` is a contradiction a screen reader reads out
//     loud.
//   * **Floating point has to be cleaned up.** `0.1 + 0.2` is not `0.3`, and a
//     slider stepping by `0.1` announces `0.30000000000000004` — which is not
//     a rounding error to the person hearing it, it is the control being
//     absurd. The number is rounded to the precision the step implies.
//   * **A fraction of an empty range is not a division.** `min === max` is a
//     legal range with one value in it, and dividing by its width is `NaN`,
//     which reaches the page as `left: NaN%` and lays the whole control out at
//     the origin.
//
// # Which way is forward
//
// `ArrowRight` adds a step in a left-to-right page and subtracts one in a
// right-to-left page, because the reader is asking for "further along" and
// further along is the other way. `isReversed` answers that from the element
// the key arrived on, which is the answer `packages/ui/index.js` prescribes:
// the direction is something the DOM knows, so it is read in an event handler
// rather than provided through a context a caller has to remember to render.
//
// Both a `dir` attribute and the computed `direction` are consulted, in that
// order, and neither alone is enough. An attribute walk misses a page that
// sets `direction` only in CSS. The computed value misses a page whose host
// does not compute inherited `direction` at all, which includes the DOM these
// tests run on — so a control that trusted it alone would pass every test and
// walk the wrong way in the one place it mattered. The one arrangement not
// answered is a `dir="rtl"` an author then contradicts with a CSS
// `direction: ltr`, which is a page disagreeing with itself.
//
// The same question is open for `movementFor` in `internal/roving-focus.js`,
// where a horizontal `Tabs.List` still walks the wrong way for an RTL reader —
// ubugeeei-prod/uf#253, which now has this to call rather than a second copy
// of it to write.
//
// # Why this is `internal/` and not a subpath
//
// `clamp` and a stepping function are the two most generic names in
// programming, and exporting them from a UI package would be publishing a
// numeric utility library under the wrong name. What is here is narrower than
// it looks: it is the arithmetic that keeps this package's four ARIA value
// attributes true about each other, and it is worth nothing to anyone who is
// not writing one of those components.

import type { Orientation } from "./roving-focus.js";

/** `value`, held inside `[lower, upper]`. */
export function clamp(value: number, lower: number, upper: number): number {
  if (value < lower) {
    return lower;
  }
  return value > upper ? upper : value;
}

/**
 * `value`, moved onto the nearest step and then held inside the range.
 *
 * The steps start at `lower`, not at zero. A `step` of zero or less means the
 * value is continuous, which is what a slider bound to a pixel measurement
 * wants, and dividing by it would be the other kind of infinity.
 */
export function snap(value: number, lower: number, upper: number, step: number): number {
  if (step <= 0) {
    return clamp(value, lower, upper);
  }
  const stepped = lower + Math.round((value - lower) / step) * step;
  // Snapping can overshoot the top when the range is not a whole number of
  // steps — 0 to 95 by 10 rounds 95 up to 100 — and an `aria-valuenow` above
  // `aria-valuemax` is a contradiction a screen reader reads out loud.
  return clamp(round(stepped, step), lower, upper);
}

/**
 * `value` as a fraction of the range, for a caller to draw with.
 *
 * `0` for an empty range rather than the `NaN` the division gives, because
 * `min === max` is a legal range with exactly one value in it and `NaN`
 * reaches the page as `left: NaN%`.
 */
export function fraction(value: number, lower: number, upper: number): number {
  const width = upper - lower;
  return width <= 0 ? 0 : clamp((value - lower) / width, 0, 1);
}

/**
 * Whether a positive step moves *backwards* along `orientation` on this page.
 *
 * Only ever true for a horizontal control in a right-to-left page: vertical
 * axes are not mirrored by writing direction, and `Home` and `End` are the
 * first and last value in both directions rather than the left and right one.
 */
export function isReversed(element: HTMLElement | null, orientation: Orientation): boolean {
  if (element == null || orientation !== "horizontal") {
    return false;
  }
  // The attribute first, because it is the answer every host agrees on.
  // Inherited `direction` is something a DOM implementation may decline to
  // compute — uf's own test DOM reports `ltr` for an element inside
  // `dir="rtl"` — and a control that reads only the computed value walks the
  // wrong way there while looking correct everywhere it was tried by hand.
  const declared = element.closest("[dir]")?.getAttribute("dir")?.toLowerCase();
  if (declared === "rtl" || declared === "ltr") {
    return declared === "rtl";
  }
  // No `dir` anywhere above it, so the page either is left-to-right or said so
  // in CSS, and only the computed value can tell the two apart.
  const view: $FlowFixMe = element.ownerDocument?.defaultView;
  return view?.getComputedStyle?.(element)?.direction === "rtl";
}

/**
 * `value` with the digits `step` cannot reach removed.
 *
 * Stepping by `0.1` from `0` reaches `0.30000000000000004`, which a screen
 * reader says in full. The number of decimals `step` implies is how many the
 * value is allowed to have.
 */
function round(value: number, step: number): number {
  const text = String(step);
  const point = text.indexOf(".");
  if (point < 0) {
    return value;
  }
  const decimals = text.length - point - 1;
  return Number(value.toFixed(decimals));
}
