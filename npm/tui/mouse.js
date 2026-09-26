// @flow
//
// The mouse: what a terminal reports, and what a handler is given.
//
// A terminal says nothing about the mouse until it is asked to. The ask is a
// set of private modes (`terminal.js` writes them), and what comes back is an
// escape sequence in the middle of the same byte stream the keys arrive on —
// so this module is the grammar of that sequence, and `keys.js` is where the
// stream is split between the two.
//
// # SGR-1006, and only SGR-1006
//
// The original encoding (`ESC [ M` and three bytes) adds 32 to each coordinate
// so that they are printable, which stops working at column 223 — on a wide
// terminal it reports a position that is not where the pointer is. The 1006
// extension writes the numbers as decimal parameters and has no limit, and it
// also distinguishes a release from a press, which the original does not: a
// terminal in the old mode reports button 3 for *every* release, so an
// application cannot tell which button was let go.
//
// Both matter here, so this decoder accepts the extension only. A terminal
// that ignored `?1006h` reports the old form, and those bytes are recognised
// and dropped rather than decoded — see {@link legacyReportLength}. Dropping
// them is deliberate: decoding a position that is wrong past column 223 is
// worse than reporting no mouse at all, and letting the three raw bytes
// through would deliver them as keys, which is how a click on such a terminal
// types random characters into an `Input`.
//
// # Names are OpenTUI's, and so is the shape of the event
//
// `"down"`, `"up"`, `"move"`, `"drag"`, `"drag-end"`, `"drop"`, `"over"`,
// `"out"`, `"scroll"` — OpenTUI's nine `event.type` values, delivered to
// `onMouseDown`, `onMouseUp` and the rest of its handler names, bubbling from
// the node under the pointer up through its parents. A component written
// against OpenTUI's interaction page behaves the same way here.
//
// `preventDefault()` is here now, and it is worth saying what it prevents,
// because a method that suppressed nothing would be a promise rather than a
// method — which is why this module did not have one until there was a default
// to suppress. There is exactly one: a left press clears the selection and, if
// it landed on selectable text, starts a new one. A handler that calls
// `preventDefault()` on that `down` keeps the selection the reader already
// had and starts none, which is what a box that does its own thing with a
// drag — a slider, a canvas, a splitter — needs in order not to leave a
// highlight behind it. Nothing else this renderer does to a mouse event can be
// prevented, and no other event type has anything to prevent.
//
// It is not a severity of `stopPropagation()`, the same way it is not one for
// a key: one of them decides whether anybody else sees the event, the other
// decides whether the renderer acts on it.

/** OpenTUI's nine mouse event types. */
export type MouseEventType =
  | "down"
  | "up"
  | "move"
  | "drag"
  | "drag-end"
  | "drop"
  | "over"
  | "out"
  | "scroll";

/** Which way a wheel turned. */
export type ScrollDirection = "up" | "down" | "left" | "right";

/** One turn of the wheel: which way, and how far. */
export type Scroll = {
  readonly direction: ScrollDirection,
  /** Notches. A terminal reports one report per notch, so this is always 1. */
  readonly delta: number,
};

/**
 * The three buttons a terminal can report, by the numbers it reports them as.
 *
 * A terminal has no notion of a fourth button or of a chord: the two low bits
 * of its report hold one of these three and a fourth value meaning "none",
 * which is why {@link MouseEvent.button} is nullable rather than being a
 * fourth constant here.
 */
export const MouseButton: {
  readonly LEFT: number,
  readonly MIDDLE: number,
  readonly RIGHT: number,
} = Object.freeze({ LEFT: 0, MIDDLE: 1, RIGHT: 2 });

/**
 * One mouse report, as the handler on a node sees it.
 *
 * `x` and `y` are cells of the frame, counted from zero at the top left — the
 * same coordinates layout writes onto a node, so a handler can compare them
 * with a node's geometry without converting. The terminal counts from one and
 * that is converted here, once.
 *
 * `target`, `currentTarget` and `source` are the `id` prop of a box rather
 * than the box itself. This package has no public node type — the tree is
 * `internal/tree.js` precisely so that nothing outside can hold a node across
 * a commit and read stale geometry off it — so what an event can carry is the
 * name the application gave the box. A box with no `id` reports `null`, which
 * is the right answer for the common case: a handler already knows which node
 * it is on, because it is its own closure.
 */
export type MouseEvent = {
  /** Which of the two things a terminal's byte stream carries. */
  readonly kind: "mouse",
  readonly type: MouseEventType,
  /**
   * The button, as {@link MouseButton} names them, or `null`.
   *
   * `null` for a wheel report and for motion with nothing held down: both are
   * encoded by the terminal as "no button", and reporting a left button for
   * them would make `event.button === MouseButton.LEFT` true for a plain
   * hover.
   */
  readonly button: number | null,
  /** The cell under the pointer, counted from zero. */
  readonly x: number,
  readonly y: number,
  readonly ctrl: boolean,
  readonly shift: boolean,
  readonly meta: boolean,
  /** The bytes this arrived as. */
  readonly raw: string,
  /** The wheel, on a `"scroll"` event, and `null` on every other. */
  readonly scroll: Scroll | null,
  /** The `id` of the topmost box under the pointer. */
  target: string | null,
  /** The `id` of the box whose handler is running; changes as it bubbles. */
  currentTarget: string | null,
  /**
   * The `id` of the box a drag started on.
   *
   * Set on `"drag"`, `"drag-end"` and `"drop"`, and `null` otherwise. It is
   * what makes a drop useful: the box that was dropped *on* receives the
   * event, and this says what was dropped.
   */
  source: string | null,
  /** Stop the event reaching this node's ancestors. */
  stopPropagation(): void,
  /**
   * Keep the renderer from doing its own thing with this event.
   *
   * On a left `"down"` that is the selection: the one the reader had is kept,
   * and no new one begins under this press. Every other event type has no
   * default, so calling this on one is harmless and does nothing.
   */
  preventDefault(): void,
  /** Whether `stopPropagation()` was called. */
  propagationStopped: boolean,
  /** Whether `preventDefault()` was called. */
  defaultPrevented: boolean,
};

/** The fields a decoded report carries before routing fills the rest in. */
type MouseFields = {
  type: MouseEventType,
  button: number | null,
  x: number,
  y: number,
  ctrl: boolean,
  shift: boolean,
  meta: boolean,
  raw: string,
  scroll?: Scroll | null,
};

/** Build a mouse event with its propagation flag wired up. */
export function mouseEvent(fields: MouseFields): MouseEvent {
  const event: MouseEvent = {
    kind: "mouse",
    type: fields.type,
    button: fields.button,
    x: fields.x,
    y: fields.y,
    ctrl: fields.ctrl,
    shift: fields.shift,
    meta: fields.meta,
    raw: fields.raw,
    scroll: fields.scroll ?? null,
    target: null,
    currentTarget: null,
    source: null,
    propagationStopped: false,
    defaultPrevented: false,
    stopPropagation() {
      event.propagationStopped = true;
    },
    preventDefault() {
      event.defaultPrevented = true;
    },
  };
  return event;
}

/**
 * The same report again, as another type.
 *
 * `"over"` and `"out"` are not reported by a terminal; they are what the
 * renderer says when the topmost node under the pointer changed, and they
 * carry the position and modifiers of the report that moved it. Deriving them
 * rather than synthesising a bare event is what keeps `event.ctrl` true for
 * the `"over"` that a Ctrl-drag caused.
 */
export function derive(from: MouseEvent, type: MouseEventType): MouseEvent {
  return mouseEvent({
    type,
    button: from.button,
    x: from.x,
    y: from.y,
    ctrl: from.ctrl,
    shift: from.shift,
    meta: from.meta,
    raw: from.raw,
    scroll: from.scroll,
  });
}

/** Which bit of a terminal's button byte means what. */
const SHIFT = 4;
const META = 8;
const CTRL = 16;
const MOTION = 32;
const WHEEL = 64;
/** The two low bits when no button is down. */
const NO_BUTTON = 3;

/** The wheel directions, in the order the two low bits name them. */
const WHEEL_DIRECTIONS: $ReadOnlyArray<ScrollDirection> = ["up", "down", "left", "right"];

/**
 * Decode one SGR-1006 report: `ESC [ < button ; column ; row M` or `… m`.
 *
 * `start` must be the `ESC`, and the caller must already have established that
 * `[` and `<` follow. Returns `null` for anything that is not a complete
 * report, which includes one cut in half by the end of a chunk — the caller
 * then treats the bytes as it treats any other unfinished sequence.
 *
 * The final byte is the whole of the press/release distinction: `M` is a
 * press or a motion, `m` is a release, and the button bits say which button
 * in both cases. That is the half of this encoding the original does not
 * have.
 */
export function decodeMouse(
  input: string,
  start: number,
): { event: MouseEvent, length: number } | null {
  let cursor = start + 3;
  let parameters = "";
  while (cursor < input.length && /[0-9;]/.test(input[cursor])) {
    parameters += input[cursor];
    cursor += 1;
  }
  const final = input[cursor];
  if (final !== "M" && final !== "m") {
    return null;
  }
  const [rawCode, rawColumn, rawRow] = parameters.split(";");
  const code = Number.parseInt(rawCode, 10);
  const column = Number.parseInt(rawColumn, 10);
  const row = Number.parseInt(rawRow, 10);
  if (!Number.isFinite(code) || !Number.isFinite(column) || !Number.isFinite(row)) {
    return null;
  }

  const raw = input.slice(start, cursor + 1);
  const modifiers = {
    shift: (code & SHIFT) !== 0,
    meta: (code & META) !== 0,
    ctrl: (code & CTRL) !== 0,
  };
  // A terminal counts from one, this renderer counts from zero, and the
  // conversion happens exactly here so that no handler ever has to know the
  // terminal had a different opinion.
  const x = Math.max(0, column - 1);
  const y = Math.max(0, row - 1);
  const held = code & NO_BUTTON;

  if ((code & WHEEL) !== 0) {
    return {
      event: mouseEvent({
        type: "scroll",
        button: null,
        x,
        y,
        ...modifiers,
        raw,
        scroll: { direction: WHEEL_DIRECTIONS[held], delta: 1 },
      }),
      length: raw.length,
    };
  }

  const button = held === NO_BUTTON ? null : held;
  const type: MouseEventType =
    final === "m" ? "up" : (code & MOTION) !== 0 ? (button == null ? "move" : "drag") : "down";

  return {
    event: mouseEvent({ type, button, x, y, ...modifiers, raw }),
    length: raw.length,
  };
}

/**
 * How long an old-style `ESC [ M` report is: three introducer bytes and one
 * each for the button, the column and the row.
 *
 * Named rather than written twice, because `keys.js` needs the same number to
 * know when one of these has not all arrived yet.
 */
export const LEGACY_REPORT_LENGTH: number = 6;

/**
 * How many bytes an old-style `ESC [ M` report occupies, or zero.
 *
 * {@link LEGACY_REPORT_LENGTH} of them. The caller consumes them and emits
 * nothing, which is the documented behaviour of this package on a terminal
 * that does not implement SGR mouse reporting — see this module's header for
 * why that is better than decoding them.
 *
 * Returns zero when fewer than that have arrived, so that a report split
 * across two reads is not half-consumed. A decoder reading a stream holds
 * those bytes until the rest of them arrive (`keys.js`, `incomplete`); the
 * pure `decodeInput` has no later chunk to wait for and delivers three payload
 * bytes as keys, which is the one case where this still leaks and needs a
 * terminal without SGR *and* a caller that is not buffering.
 */
export function legacyReportLength(input: string, start: number): number {
  if (input[start + 2] !== "M") {
    return 0;
  }
  return input.length - start >= LEGACY_REPORT_LENGTH ? LEGACY_REPORT_LENGTH : 0;
}
