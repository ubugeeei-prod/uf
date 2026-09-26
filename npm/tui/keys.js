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
import { LEGACY_REPORT_LENGTH, decodeMouse, legacyReportLength } from "./mouse.js";

/** Which of the two parsers produced an event. */
export type KeySource = "raw" | "escape";

/** What the terminal says happened to the key. */
export type KeyEventType = "press" | "repeat" | "release";

/**
 * One key event.
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
  /** Whether this was a press, terminal repeat, or release event. */
  readonly eventType: KeyEventType,
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

/** What an SGR-1006 mouse report begins with; `mouse.js` has the rest. */
const MOUSE_SGR = "\u001b[<";

/** What a terminal that ignored `?1006h` begins one with instead. */
const MOUSE_LEGACY = "\u001b[M";

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

/** Non-Unicode Kitty `CSI u` key codes that do not already have legacy names. */
const CSI_U_FUNCTION: { [string]: string } = {
  "9": "tab",
  "13": "return",
  "27": "escape",
  "127": "backspace",
  "57358": "caps-lock",
  "57359": "scroll-lock",
  "57360": "num-lock",
  "57361": "print-screen",
  "57362": "pause",
  "57363": "menu",
  "57376": "f13",
  "57377": "f14",
  "57378": "f15",
  "57379": "f16",
  "57380": "f17",
  "57381": "f18",
  "57382": "f19",
  "57383": "f20",
  "57384": "f21",
  "57385": "f22",
  "57386": "f23",
  "57387": "f24",
  "57388": "f25",
  "57389": "f26",
  "57390": "f27",
  "57391": "f28",
  "57392": "f29",
  "57393": "f30",
  "57394": "f31",
  "57395": "f32",
  "57396": "f33",
  "57397": "f34",
  "57398": "f35",
  "57399": "kp-0",
  "57400": "kp-1",
  "57401": "kp-2",
  "57402": "kp-3",
  "57403": "kp-4",
  "57404": "kp-5",
  "57405": "kp-6",
  "57406": "kp-7",
  "57407": "kp-8",
  "57408": "kp-9",
  "57409": "kp-decimal",
  "57410": "kp-divide",
  "57411": "kp-multiply",
  "57412": "kp-subtract",
  "57413": "kp-add",
  "57414": "kp-enter",
  "57415": "kp-equal",
  "57416": "kp-separator",
  "57417": "kp-left",
  "57418": "kp-right",
  "57419": "kp-up",
  "57420": "kp-down",
  "57421": "kp-pageup",
  "57422": "kp-pagedown",
  "57423": "kp-home",
  "57424": "kp-end",
  "57425": "kp-insert",
  "57426": "kp-delete",
  "57427": "kp-begin",
  "57428": "media-play",
  "57429": "media-pause",
  "57430": "media-play-pause",
  "57431": "media-reverse",
  "57432": "media-stop",
  "57433": "media-fast-forward",
  "57434": "media-rewind",
  "57435": "media-track-next",
  "57436": "media-track-previous",
  "57437": "media-record",
  "57438": "volume-down",
  "57439": "volume-up",
  "57440": "volume-mute",
  "57441": "left-shift",
  "57442": "left-control",
  "57443": "left-alt",
  "57444": "left-super",
  "57445": "left-hyper",
  "57446": "left-meta",
  "57447": "right-shift",
  "57448": "right-control",
  "57449": "right-alt",
  "57450": "right-super",
  "57451": "right-hyper",
  "57452": "right-meta",
  "57453": "iso-level3-shift",
  "57454": "iso-level5-shift",
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
  eventType?: KeyEventType,
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
    eventType: fields.eventType ?? "press",
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

/** The Kitty keyboard protocol encodes press/repeat/release as modifier sub-fields. */
function keyEventType(parameter: string | void): KeyEventType | null {
  if (parameter == null || parameter === "" || parameter === "1") {
    return "press";
  }
  if (parameter === "2") {
    return "repeat";
  }
  if (parameter === "3") {
    return "release";
  }
  return null;
}

function codePoint(parameter: string | void): number | null {
  if (parameter == null || parameter === "") {
    return null;
  }
  const value = Number.parseInt(parameter, 10);
  if (!Number.isFinite(value) || value < 0 || value > 0x10ffff) {
    return null;
  }
  if (value >= 0xd800 && value <= 0xdfff) {
    return null;
  }
  return value;
}

function codePointText(parameter: string | void): string {
  if (parameter == null || parameter === "") {
    return "";
  }
  const points: Array<number> = [];
  for (const field of parameter.split(":")) {
    const point = codePoint(field);
    if (point == null || point < 0x20 || (point >= 0x7f && point <= 0x9f)) {
      return "";
    }
    points.push(point);
  }
  return String.fromCodePoint(...points);
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
 * A decoder that survives a sequence arriving in pieces.
 *
 * {@link decodeInput} is a pure function of one chunk, which is right for
 * every key: a terminal delivers a key's escape sequence in a single read, and
 * `ESC` at the end of a chunk is the Escape key. Two things a terminal sends
 * are not keys and do not keep that promise — a paste, which is as long as the
 * clipboard, and a mouse report, which a terminal in any-motion mode sends one
 * of per cell the pointer crosses. The operating system splits either wherever
 * it likes. So a driver reading a real stream holds one of these across
 * chunks.
 *
 * # The one rule, and what bounds it
 *
 * `push` holds back a trailing run of bytes that **cannot be anything but the
 * beginning of a sequence this decoder must see whole** — see `incomplete`,
 * which is the whole of that judgement and the only place it is made. Nothing
 * else is buffered: a chunk that ends anywhere else is decoded completely,
 * because every other sequence a terminal sends either fits in a read or is
 * ambiguous with a key that must fire now.
 *
 * A held run is released by exactly two things. The next chunk completes it,
 * or {@link InputDecoder.flush} says no next chunk is coming and the bytes are
 * decoded as they stand. And a run that grows past {@link HOLD_LIMIT} without
 * completing is not one of these sequences however it began, so it is decoded
 * rather than held — which is what keeps a terminal emitting nonsense from
 * wedging the decoder even where nobody calls `flush`.
 */
export type InputDecoder = {
  /** Decode one chunk, holding back a sequence that has not ended yet. */
  push(chunk: string): Array<InputEvent>,
  /** Give up on an unfinished sequence and emit what arrived. */
  flush(): Array<InputEvent>,
};

/**
 * The longest run of bytes this decoder will hold waiting for the rest of it.
 *
 * Every sequence `incomplete` waits for is shorter: the paste introducer is
 * six bytes, an old-style mouse report is six, and an SGR report is `ESC [ <`
 * plus three decimal parameters, two semicolons and a final byte. Kitty's
 * `CSI u` key reports can be longer because their associated text is encoded
 * as code points. Past this, the bytes are not the sequence they looked like
 * and are decoded as what they are.
 *
 * The bound is what makes the hold safe without a timer. `flush` is the
 * ordinary way out and a driver calls it when its stream ends; this is for the
 * case where nothing ever calls it and the terminal has sent `ESC [ <` and
 * then a thousand digits.
 */
const HOLD_LIMIT = 96;

/**
 * Whether the bytes from `start` begin a sequence whose rest has not arrived.
 *
 * The one buffering rule. It answers for the three sequences a read boundary
 * can fall inside and this decoder would otherwise mis-decode:
 *
 *   * `ESC [ 2 0 0 ~`, the paste introducer, where a split turns the marker
 *     into Alt-and-a-bracket followed by three digits and the paste's first
 *     line then runs as typing;
 *   * `ESC [ <` …, an SGR mouse report, where a split turns a click into the
 *     characters of its coordinates — ubugeeei-prod/uf#612, and the traffic
 *     `?1003h` produces is a report per cell crossed, which is the traffic
 *     most likely to be split;
 *   * `ESC [ M` …, the old-style report, whose three payload bytes are
 *     arbitrary and become arbitrary keys;
 *   * Kitty `CSI u` key reports, where splitting before the final `u` would
 *     turn `CSI 97 ; 1 : 3 u` from "release A" into ordinary characters.
 *
 * Two bytes at least, so a lone `ESC` is still the Escape key: holding that
 * back would mean Escape never fires until the next keystroke, which is worse
 * than the ambiguity it would solve. From `ESC[` on there is nothing else the
 * bytes could be that this decoder would get right anyway — an unfinished CSI
 * at the end of a chunk decodes as Alt and a bracket.
 *
 * It is deliberately *not* asked about a complete sequence with more input
 * after it. A run is held only when it reaches the end of the chunk, which is
 * what "the rest has not arrived" means; a report followed by a keystroke has
 * a final byte and is decoded where it stands.
 */
function incomplete(input: string, start: number): boolean {
  const rest = input.slice(start);
  if (rest.length < 2 || rest.length > HOLD_LIMIT || !rest.startsWith(ESC)) {
    return false;
  }
  if (rest.length < PASTE_START.length && PASTE_START.startsWith(rest)) {
    return true;
  }
  if (rest.startsWith(MOUSE_SGR)) {
    // Complete when the final byte has arrived, and no longer a report at all
    // once something that is not a parameter byte has: `decodeMouse` answers
    // `null` for that and the bytes fall through to the key grammar, which is
    // where they should fall through rather than being waited on for ever.
    const parameters = rest.slice(MOUSE_SGR.length);
    return /^[0-9;]*$/.test(parameters);
  }
  if (rest.startsWith(MOUSE_LEGACY)) {
    return rest.length < LEGACY_REPORT_LENGTH;
  }
  if (rest.startsWith(ESC + "[")) {
    return /^[0-9:;]*$/.test(rest.slice(2));
  }
  return false;
}

/** A decoder with somewhere to keep a half-arrived sequence. */
export function createInputDecoder(): InputDecoder {
  let pending: string | null = null;
  /** A chunk that ended part-way through a sequence; see `incomplete`. */
  let waiting = "";

  const decoder: InputDecoder = {
    push(chunk: string): Array<InputEvent> {
      const events: Array<InputEvent> = [];
      let input = waiting + chunk;
      waiting = "";
      const pasting = pending;
      if (pasting != null) {
        const end = input.indexOf(PASTE_END);
        if (end < 0) {
          pending = pasting + input;
          return events;
        }
        const text = pasting + input.slice(0, end);
        pending = null;
        events.push(pasteEvent(text, PASTE_START + text + PASTE_END));
        input = input.slice(end + PASTE_END.length);
      }

      let index = 0;
      while (index < input.length) {
        if (incomplete(input, index)) {
          waiting = input.slice(index);
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
      if (waiting !== "") {
        // Not the sequence it looked like after all: no more input is coming,
        // so the bytes are whatever they decode to on their own.
        const held = waiting;
        waiting = "";
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
 * One chunk, decoded completely. A sequence this chunk begins and does not end
 * is not held, because there is no later chunk for a pure function to wait for:
 * an unfinished paste is emitted as the paste it was becoming, and an
 * unfinished mouse report is decoded as the bytes it is. A driver reading a
 * stream wants {@link createInputDecoder} instead, which holds them.
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
  while (cursor < input.length && /[0-9:;]/.test(input[cursor])) {
    parameters += input[cursor];
    cursor += 1;
  }
  const final = input[cursor];
  if (final === undefined) {
    return null;
  }
  const raw = input.slice(start, cursor + 1);
  const [first, second] = parameters.split(";");

  if (final === "u") {
    const key = decodeKittyKey(parameters, raw);
    if (key == null) {
      return null;
    }
    return { key, length: raw.length };
  }

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

function decodeKittyKey(parameters: string, raw: string): KeyEvent | null {
  const [keyParameter, modifierParameter, textParameter] = parameters.split(";");
  const key = codePoint(keyParameter.split(":")[0]);
  if (key == null) {
    return null;
  }
  const modifierFields = modifierParameter?.split(":") ?? [];
  const mods = modifiers(modifierFields[0]);
  const eventType = keyEventType(modifierFields[1]);
  if (eventType == null) {
    return null;
  }
  const associatedText = codePointText(textParameter);
  const functionName = CSI_U_FUNCTION[String(key)];
  if (functionName != null) {
    return event({
      name: functionName,
      sequence: "",
      raw,
      source: "escape",
      ...mods,
      eventType,
    });
  }
  if (key === 0) {
    return event({
      name: "text",
      sequence: eventType === "release" ? "" : associatedText,
      raw,
      source: "escape",
      ...mods,
      eventType,
    });
  }
  const character = String.fromCodePoint(key);
  const name = character === " " ? "space" : character.toLowerCase();
  const sequence =
    associatedText !== ""
      ? associatedText
      : eventType === "release" || mods.ctrl || mods.meta
        ? ""
        : character;
  return event({ name, sequence, raw, source: "escape", ...mods, eventType });
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
