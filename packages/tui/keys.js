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
// # One stream, two kinds of event
//
// A terminal with mouse reporting on writes its reports into the same stream,
// as escape sequences that are not keys. So the decoder's output is a union:
// {@link InputEvent} is a key or a mouse report, told apart by `kind`, and
// `mouse.js` holds the half this module does not. Splitting them at the byte
// level rather than after the fact is not a preference — a mouse report and
// `Alt+[` begin with the same two bytes, and a decoder that guessed later
// would have had to un-decode a key it had already emitted.
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

import type { MouseEvent } from "./mouse.js";
import { decodeMouse, legacyReportLength } from "./mouse.js";

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
  /**
   * Which of the two things a terminal's byte stream carries.
   *
   * A stream holds keys and, when mouse reporting is on, mouse reports. This
   * is what tells them apart, and it is on the event rather than inferred from
   * the presence of a field so that a `switch` over it is exhaustive.
   */
  readonly kind: "key",
  /**
   * The canonical name: `"a"`, `"space"`, `"return"`, `"escape"`, `"up"`.
   *
   * `"paste"` is the one name that is not a key. A terminal in bracketed
   * paste mode wraps pasted text in `ESC[200~` and `ESC[201~` so that an
   * application can tell it from typing, and the whole point of knowing is to
   * treat it as *text* — so it arrives as one event carrying all of it rather
   * than as the burst of key presses it would otherwise look like.
   */
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

/**
 * One thing that arrived from a terminal.
 *
 * Everything a driver reads is one of these two, and `kind` is how a caller
 * tells them apart without a type test on a field that might one day exist on
 * both.
 */
export type InputEvent = KeyEvent | MouseEvent;

const ESC = "\u001b";

/** What a terminal in bracketed paste mode puts around pasted text. */
const PASTE_START = "\u001b[200~";
const PASTE_END = "\u001b[201~";

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
    kind: "key",
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
 * One pasted block, as the event a handler sees.
 *
 * The payload is whatever was on the clipboard, given back verbatim and
 * therefore **untrusted**: it can hold newlines, control bytes and escape
 * sequences of its own. Handing it over as text rather than as keys is the
 * entire purpose of bracketed paste — a terminal without it delivers a pasted
 * `\r` as Enter, which is how pasting a two-line command into a prompt runs
 * the first line. What a component does with the text is its own decision;
 * `Input` takes the first line and drops the control characters, because it is
 * one line of text and cannot hold either.
 */
function pasteEvent(text: string, raw: string): KeyEvent {
  return event({ name: "paste", sequence: text, raw, source: "escape" });
}

/**
 * A decoder that survives a paste arriving in pieces.
 *
 * {@link decodeInput} is a pure function of one chunk, which is right for
 * every key: a terminal delivers an escape sequence in a single read. A paste
 * is the exception — it is as long as the clipboard, and the operating system
 * splits a large one across reads wherever it likes, including in the middle
 * of a word and including between the text and its terminator. So a driver
 * reading a real stream holds one of these across chunks, and the text that
 * arrives is the text that was pasted rather than the first sixty-four
 * kilobytes of it followed by a burst of keys.
 */
export type InputDecoder = {
  /** Decode one chunk, holding back a paste that has not ended yet. */
  push(chunk: string): Array<InputEvent>,
  /** Give up on an unterminated paste and emit what arrived. */
  flush(): Array<InputEvent>,
};

/**
 * Whether `input` from `start` could still become a paste introducer.
 *
 * Two bytes at least, so a lone `ESC` is still the Escape key: holding that
 * back would mean Escape never fires until the next keystroke, which is worse
 * than the ambiguity it would solve. From `ESC[` on there is nothing else it
 * could be that this decoder would get right anyway — an unfinished CSI at the
 * end of a chunk decodes today as Alt and a bracket.
 */
function beginsPaste(input: string, start: number): boolean {
  const rest = input.length - start;
  return rest >= 2 && rest < PASTE_START.length && PASTE_START.startsWith(input.slice(start));
}

/** A decoder with somewhere to keep a half-arrived paste. */
export function createInputDecoder(): InputDecoder {
  let pending: string | null = null;
  /** A chunk that ended part-way through `ESC[200~`. */
  let introducer = "";

  const decoder: InputDecoder = {
    push(chunk: string): Array<InputEvent> {
      const events: Array<InputEvent> = [];
      let input = introducer + chunk;
      introducer = "";
      if (pending != null) {
        const end = input.indexOf(PASTE_END);
        if (end < 0) {
          pending += input;
          return events;
        }
        const text = pending + input.slice(0, end);
        pending = null;
        events.push(pasteEvent(text, PASTE_START + text + PASTE_END));
        input = input.slice(end + PASTE_END.length);
      }

      let index = 0;
      while (index < input.length) {
        if (beginsPaste(input, index)) {
          introducer = input.slice(index);
          return events;
        }
        if (input.startsWith(PASTE_START, index)) {
          const from = index + PASTE_START.length;
          const end = input.indexOf(PASTE_END, from);
          if (end < 0) {
            pending = input.slice(from);
            return events;
          }
          const text = input.slice(from, end);
          events.push(pasteEvent(text, PASTE_START + text + PASTE_END));
          index = end + PASTE_END.length;
          continue;
        }
        index += decodeOne(input, index, events);
      }
      return events;
    },
    flush(): Array<InputEvent> {
      if (introducer !== "") {
        // Not a paste after all: no more input is coming, so the bytes are
        // whatever they decode to on their own.
        const held = introducer;
        introducer = "";
        const events: Array<InputEvent> = [];
        let index = 0;
        while (index < held.length) {
          index += decodeOne(held, index, events);
        }
        return events;
      }
      if (pending == null) {
        return [];
      }
      const text = pending;
      pending = null;
      return [pasteEvent(text, PASTE_START + text)];
    },
  };
  return decoder;
}

/**
 * Everything in a chunk of terminal input: keys, and mouse reports.
 *
 * A chunk is not a key. Holding a key down, pasting, or simply typing fast
 * delivers several at once, and a decoder that returns the first and drops the
 * rest loses characters under exactly the conditions — fast typing — where
 * losing them is most obvious.
 *
 * One chunk, decoded completely: a paste this chunk begins and does not end is
 * emitted anyway, because there is no later chunk for a pure function to wait
 * for. A driver reading a stream wants {@link createInputDecoder} instead.
 */
export function decodeInput(input: string): Array<InputEvent> {
  const decoder = createInputDecoder();
  return [...decoder.push(input), ...decoder.flush()];
}

/**
 * The key events in a chunk, with any mouse reports left out.
 *
 * The narrow view, for a caller that has not turned mouse reporting on and
 * therefore cannot receive one — which is every caller of this function until
 * an application asks `render` for the mouse. A caller that has wants
 * {@link decodeInput}, because dropping half of what a terminal said is a
 * poor way to find out it was said.
 */
export function decodeKeys(input: string): Array<KeyEvent> {
  const keys: Array<KeyEvent> = [];
  for (const event of decodeInput(input)) {
    if (event.kind === "key") {
      keys.push(event);
    }
  }
  return keys;
}

function decodeOne(input: string, start: number, events: Array<InputEvent>): number {
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

  // A mouse report, before the key grammar gets a look at it. `ESC[<` is not
  // reachable as a key — the CSI parameter bytes are digits and semicolons —
  // so this branch takes nothing away from the one below it.
  if (next === "[" && input[start + 2] === "<") {
    const report = decodeMouse(input, start);
    if (report != null) {
      events.push(report.event);
      return report.length;
    }
  }

  // The report a terminal sends when it did not understand `?1006h`. Consumed
  // and dropped: `mouse.js` says why decoding it would be worse, and why
  // letting its three payload bytes through as keys would be worse still.
  if (next === "[") {
    const legacy = legacyReportLength(input, start);
    if (legacy > 0) {
      return legacy;
    }
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
