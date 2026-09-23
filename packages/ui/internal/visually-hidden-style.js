// @flow
//
// The one inline style for "hidden from sight, present for a screen reader".
//
// Its own module, with no `"use client"`, so a part that renders on the server
// can import it without importing `visually-hidden.js` — a client module whose
// every export is a client reference to a Server Component. Internal for the
// reason `packages/ui/index.js` gives about every module here: the public
// answer is `VisuallyHidden`.

/**
 * The style that hides an element from sight and from nothing else.
 *
 * Shared by `VisuallyHidden` and by the parts that keep a live region of their
 * own beside what they render — the table's sort announcement, the
 * collections', the calendar's and the range calendar's, and the segmented
 * fields'. Frozen, because every one of them holds the same object.
 */
export const visuallyHiddenStyle: {|
  readonly border: number,
  readonly clip: string,
  readonly clipPath: string,
  readonly height: number,
  readonly margin: number,
  readonly overflow: string,
  readonly padding: number,
  readonly position: string,
  readonly whiteSpace: string,
  readonly width: number,
|} = Object.freeze({
  border: 0,
  clip: "rect(0, 0, 0, 0)",
  clipPath: "inset(50%)",
  height: 1,
  margin: -1,
  overflow: "hidden",
  padding: 0,
  position: "absolute",
  whiteSpace: "nowrap",
  width: 1,
});
