// @flow
//
// The workload, written once as data and twice as components.
//
// `@uniflowed/tui` and React Ink are two different libraries, so the only way
// to compare them is to give each the same picture to draw and the same four
// changes to make to it. That picture is here, as plain values, so that
// neither implementation can quietly draw something cheaper than the other.
//
// # What the four steps are, and why these four
//
// * **first** — the whole frame, from nothing. This is the number that says
//   what a mount costs, and it is the one where the two libraries are most
//   alike: nobody can send less than a screen when the screen is empty.
// * **status** — one character of one line changes. The smallest possible
//   edit, and the one that separates a renderer that sends cells from one that
//   sends lines: the status line is at the *top*, so a renderer that reprints
//   from the first changed line to the bottom of the frame reprints all of it.
// * **scroll** — the list advances by one row. Twenty-one lines change and
//   two do not. Nothing can be cheap here; the question is how much more than
//   the change each library sends.
// * **full** — every line changes. The upper bound, and a sanity check: two
//   renderers that disagree by much on *this* are not drawing the same thing.
//
// # The frame
//
// Eighty by twenty-four, the size of the terminal everything else in this
// repository is measured at. One status line, twenty-one rows of list, a blank
// line, and a footer. No border and no colour: both libraries draw borders,
// and they draw them with different characters and different padding rules, so
// a border would be measuring the difference between two pictures rather than
// between two renderers.

/** How wide and how tall the terminal is for every measurement here. */
export const WIDTH: number = 80;
export const HEIGHT: number = 24;

/** How many list rows sit between the status line and the footer. */
export const LIST_ROWS: number = 21;

/** The state the two applications render, and nothing else. */
export type Frame = {
  /** The counter on the status line. */
  readonly tick: number,
  /** The first list row's number. */
  readonly offset: number,
  /** A suffix appended to every list row, to make "everything changed" cheap
   * to express. */
  readonly mark: string,
};

/** Where the four steps start. */
export const START: Frame = { tick: 1000, offset: 0, mark: "" };

/**
 * The four measured steps, in order, as the state each one moves to.
 *
 * `first` is the mount, so it has no state of its own — it is `START`.
 */
export const STEPS: $ReadOnlyArray<{ readonly name: string, readonly to: Frame }> = [
  { name: "status", to: { tick: 1001, offset: 0, mark: "" } },
  { name: "scroll", to: { tick: 1001, offset: 1, mark: "" } },
  { name: "full", to: { tick: 1002, offset: 2, mark: "*" } },
];

/** The status line, which is what the `status` step changes one character of. */
export function statusLine(frame: Frame): string {
  return `uf bench · frame ${String(frame.tick)} · ${String(LIST_ROWS)} rows`;
}

/** One list row. Padded to a fixed width so no step changes a line's length. */
export function listRow(frame: Frame, index: number): string {
  const number = String(frame.offset + index).padStart(5, "0");
  return `${number}  packages/example/module-${number}.js  compiled${frame.mark}`;
}

/** The last line, which never changes and is therefore worth having. */
export const FOOTER: string = "press q to quit";

/** Every line of the frame, top to bottom, as the two renderers must draw it. */
export function lines(frame: Frame): Array<string> {
  const out = [statusLine(frame)];
  for (let index = 0; index < LIST_ROWS; index += 1) {
    out.push(listRow(frame, index));
  }
  out.push("");
  out.push(FOOTER);
  return out;
}
