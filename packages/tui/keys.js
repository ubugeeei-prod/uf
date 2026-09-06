// @flow
//
// Terminal bytes in, key events out.
//
// A terminal does not deliver keys. It delivers a byte stream in which most
// keys are one byte, some are two, and the interesting ones are escape
// sequences of four to nine bytes whose grammar predates every convention a
// reader would guess from. `Escape` and `Alt+A` begin with the same byte.
// `Enter` is `\r` under raw mode and `\n` when it is not. The up arrow is
// `ESC [ A` on one terminal and `ESC O A` on another, depending on a mode the
// application itself may have set.
//
// This module is the whole of that grammar, and it is a pure function from a
// string to events so that a test can press a key without a terminal. Every
// input test in `tui.test.js` goes through here, which is what makes them
// tests of the real path rather than tests of a fake one.
//
// # The names are OpenTUI's
//
// `"return"`, not `"enter"`. `"escape"`, not `"esc"`. Those are the canonical
// names OpenTUI's `KeyEvent.name` uses, and a component written against its
// documentation compares against them — so a handler that reads
// `key.name === "return"` behaves identically under either library. The
// aliases people actually type (`"enter"`, `"esc"`) are accepted where uf
// takes a key *binding* from a caller, and normalised there rather than here,
// so there is exactly one spelling in an event.
//
// # Propagation is OpenTUI's too, including the part that looks wrong
//
// `stopPropagation()` stops later global listeners *and* prevents the focused
// node from seeing the event. `preventDefault()` does the opposite half: later
// global listeners still run, but the focused node is skipped. They are not a
// pair of ordered severities, they are two independent answers to two
// questions — "does anyone else get to see this" and "does the thing that has
// focus act on it" — and conflating them is what makes a global `Ctrl+C`
// handler either unreachable or unable to stop a text input from inserting a
// character.

/** Which of the two parsers produced an event. */
export type KeySource = "raw" | "escape";

/**
 * One key press.
 *
 * `sequence` is the text the key stands for and `raw` is the bytes it arrived
 * as; they differ for every key that is not a printable character, and a
 * handler that inserts `sequence` into a buffer rather than `raw` is the
 * difference between typing `a` and typing `^[[A`.
 */
export type KeyEvent = {
  /** The canonical name: `"a"`, `"space"`, `"return"`, `"escape"`, `"up"`. */
  readonly name: string,
  /** The text this key stands for, empty for keys that stand for none. */
  readonly sequence: string,
  /** The bytes as they arrived. */
  readonly raw: string,
  /** Which parser produced it. */
  readonly source: KeySource,
  readonly ctrl: boolean,
  readonly shift: boolean,
  readonly meta: boolean,
  /** Always `"press"`. Release reporting needs the Kitty protocol; see #314. */
  readonly eventType: "press",
  /** Skip the focused node's handler, without silencing later global ones. */
  preventDefault(): void,
  /** Silence later global handlers, and the focused node's. */
  stopPropagation(): void,
  /** Whether `preventDefault()` was called. */
  defaultPrevented: boolean,
  /** Whether `stopPropagation()` was called. */
  propagationStopped: boolean,
};

const ESC = "\u001b";

/** The `CSI …` final bytes that name a key on their own. */
const CSI_FINAL: { [string]: string } = {
  A: "up",
  B: "down",
  C: "right",
  D: "left",
  H: "home",
  F: "end",
  E: "clear",
  P: "f1",
  Q: "f2",
  R: "f3",
  S: "f4",
  Z: "tab",
};

/** The `CSI n ~` numbers, which is the other half of the same vocabulary. */
const CSI_TILDE: { [string]: string } = {
  "1": "home",
  "2": "insert",
  "3": "delete",
  "4": "end",
  "5": "pageup",
  "6": "pagedown",
  "11": "f1",
  "12": "f2",
  "13": "f3",
  "14": "f4",
  "15": "f5",
  "17": "f6",
  "18": "f7",
  "19": "f8",
  "20": "f9",
  "21": "f10",
  "23": "f11",
  "24": "f12",
};

/** Build an event with its two propagation flags wired up. */
function event(fields: {
  name: string,
  sequence: string,
  raw: string,
  source: KeySource,
  ctrl?: boolean,
  shift?: boolean,
  meta?: boolean,
}): KeyEvent {
  const key: KeyEvent = {
    name: fields.name,
    sequence: fields.sequence,
    raw: fields.raw,
    source: fields.source,
    ctrl: fields.ctrl === true,
    shift: fields.shift === true,
    meta: fields.meta === true,
    eventType: "press",
    defaultPrevented: false,
    propagationStopped: false,
    preventDefault() {
      key.defaultPrevented = true;
    },
    stopPropagation() {
      key.propagationStopped = true;
    },
  };
  return key;
}

/**
 * Decode the `1 + shift + 2·alt + 4·ctrl` parameter terminals encode
 * modifiers in.
 *
 * The offset of one is not decoration: a parameter of zero means "absent" in
 * the CSI grammar, so the unmodified case has to be one and every modifier
 * combination is that plus a bit mask.
 */
function modifiers(parameter: string | void): { ctrl: boolean, shift: boolean, meta: boolean } {
  const value = Number.parseInt(parameter ?? "1", 10);
  const bits = Number.isFinite(value) && value > 0 ? value - 1 : 0;
  return { shift: (bits & 1) !== 0, meta: (bits & 2) !== 0, ctrl: (bits & 4) !== 0 };
}

/**
 * Every key event in a chunk of terminal input.
 *
 * A chunk is not a key. Holding a key down, pasting, or simply typing fast
 * delivers several at once, and a decoder that returns the first and drops the
 * rest loses characters under exactly the conditions — fast typing — where
 * losing them is most obvious.
 */
export function decodeKeys(input: string): Array<KeyEvent> {
  const events: Array<KeyEvent> = [];
  let index = 0;
  while (index < input.length) {
    const consumed = decodeOne(input, index, events);
    index += consumed;
  }
  return events;
}

function decodeOne(input: string, start: number, events: Array<KeyEvent>): number {
  const character = input[start];

  if (character !== ESC) {
    events.push(decodePlain(character));
    return 1;
  }

  // `ESC` with nothing after it is the Escape key. This is the ambiguity at
  // the centre of terminal input: the same byte begins `Alt+A` and every
  // arrow key, and the only thing that distinguishes them is what follows in
  // the *same read*. A terminal delivers a real escape sequence as one chunk,
  // so "nothing follows in this chunk" is the signal — imperfect, and the
  // reason `Escape` is felt as slightly laggy in every terminal program ever
  // written.
  const next = input[start + 1];
  if (next === undefined) {
    events.push(event({ name: "escape", sequence: "", raw: ESC, source: "raw" }));
    return 1;
  }

  if (next === "[" || next === "O") {
    const parsed = decodeSequence(input, start);
    if (parsed != null) {
      events.push(parsed.key);
      return parsed.length;
    }
  }

  // `ESC` followed by anything else is that key with Alt held.
  const inner = decodePlain(next);
  events.push(
    event({
      name: inner.name,
      sequence: inner.sequence,
      raw: ESC + next,
      source: "escape",
      ctrl: inner.ctrl,
      shift: inner.shift,
      meta: true,
    }),
  );
  return 2;
}

function decodeSequence(input: string, start: number): { key: KeyEvent, length: number } | null {
  const introducer = input[start + 1];
  // `ESC O x` — the "application cursor keys" form. Same keys, different
  // spelling, chosen by a mode the terminal may be in for reasons that have
  // nothing to do with this program.
  if (introducer === "O") {
    const final = input[start + 2];
    const name = final != null ? CSI_FINAL[final] : undefined;
    if (name == null) {
      return null;
    }
    const raw = input.slice(start, start + 3);
    return { key: event({ name, sequence: "", raw, source: "escape" }), length: 3 };
  }

  let cursor = start + 2;
  let parameters = "";
  while (cursor < input.length && /[0-9;]/.test(input[cursor])) {
    parameters += input[cursor];
    cursor += 1;
  }
  const final = input[cursor];
  if (final === undefined) {
    return null;
  }
  const raw = input.slice(start, cursor + 1);
  const [first, second] = parameters.split(";");

  if (final === "~") {
    const name = CSI_TILDE[first];
    if (name == null) {
      return null;
    }
    const mods = modifiers(second);
    return {
      key: event({ name, sequence: "", raw, source: "escape", ...mods }),
      length: raw.length,
    };
  }

  const name = CSI_FINAL[final];
  if (name == null) {
    return null;
  }
  // `CSI Z` is Shift+Tab, and it carries no modifier parameter to say so.
  const mods = final === "Z" ? { ctrl: false, shift: true, meta: false } : modifiers(second);
  return { key: event({ name, sequence: "", raw, source: "escape", ...mods }), length: raw.length };
}

/** One byte that is not part of an escape sequence. */
function decodePlain(character: string): KeyEvent {
  const code = character.charCodeAt(0);

  if (character === "\r" || character === "\n") {
    return event({ name: "return", sequence: "\r", raw: character, source: "raw" });
  }
  if (character === "\t") {
    return event({ name: "tab", sequence: "\t", raw: character, source: "raw" });
  }
  if (character === " ") {
    return event({ name: "space", sequence: " ", raw: character, source: "raw" });
  }
  // Both spellings of Backspace. Which one arrives depends on the terminal's
  // `erase` setting, and a program that handles only `\x7f` is a program whose
  // backspace key does nothing on somebody else's machine.
  if (code === 0x7f || code === 0x08) {
    return event({ name: "backspace", sequence: "", raw: character, source: "raw" });
  }
  if (code === 0) {
    return event({ name: "space", sequence: "", raw: character, source: "raw", ctrl: true });
  }
  if (code < 0x20) {
    // A C0 control is Ctrl plus the letter at that position in the alphabet.
    const letter = String.fromCharCode(code + 0x60);
    return event({ name: letter, sequence: "", raw: character, source: "raw", ctrl: true });
  }
  // A printable character. `shift` is reported for an uppercase letter because
  // that is the only evidence a terminal gives: there is no separate shift
  // report outside the Kitty protocol, and `A` is what Shift+A means.
  const lower = character.toLowerCase();
  return event({
    name: lower,
    sequence: character,
    raw: character,
    source: "raw",
    shift: character !== lower,
  });
}
