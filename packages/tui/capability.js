// @flow
//
// What the attached terminal can actually render.
//
// A TUI that assumes 24-bit colour, a mouse and box-drawing characters works
// on the machine it was written on and produces unreadable noise on a build
// agent, in a `TERM=dumb` shell, and in every terminal a screen reader is
// pointed at. This module is where that assumption is refused, once, before
// the first frame.
//
// # It is the CLI's answer, not a second one
//
// `crates/uf_term/src/capability.rs` already answers this question for `uf`
// itself, and it answers it after a long argument with reality — the
// precedence order below is its order, variable for variable, including the
// parts that look arbitrary. `CLICOLOR_FORCE` beating `TERM=dumb` is not an
// oversight; `FORCE_COLOR=` with an empty value meaning "on" is a convention
// three other tools already established. Reproducing it exactly is the point:
// a project whose CLI and whose TUI library disagree about whether this
// terminal takes colour is a project that will be sent a screenshot of one of
// them being wrong.
//
// # Detected once
//
// Capability is resolved at start-up from three inputs — an explicit choice,
// the environment, and whether the stream is a terminal — and then carried as
// a plain value. No write path re-reads `process.env`, which is both faster
// and the only way the result can be tested without mutating the environment
// of the process running the test.

/** How much colour a stream can carry. */
export type ColorLevel = "none" | "ansi16" | "ansi256" | "truecolor";

/** Which glyph vocabulary is safe to print. */
export type GlyphSet = "unicode" | "ascii";

/** Whether a human is looking at the stream. */
export type Tty = "interactive" | "piped";

/** What a caller asked for, before the environment gets a say. */
export type ColorChoice = "auto" | "always" | "never";

/** The environment variables that influence terminal rendering. */
export type TerminalEnv = {
  readonly NO_COLOR?: string,
  readonly FORCE_COLOR?: string,
  readonly CLICOLOR?: string,
  readonly CLICOLOR_FORCE?: string,
  readonly TERM?: string,
  readonly COLORTERM?: string,
  readonly LC_ALL?: string,
  readonly LC_CTYPE?: string,
  readonly LANG?: string,
};

/** The resolved rendering capability of one stream. */
export type Capabilities = {
  readonly color: ColorLevel,
  readonly glyphs: GlyphSet,
  readonly tty: Tty,
};

const LEVELS: $ReadOnlyArray<ColorLevel> = ["none", "ansi16", "ansi256", "truecolor"];

/** Order colour levels so `max` and `min` mean what they say. */
const rank = (level: ColorLevel): number => LEVELS.indexOf(level);

const nonEmpty = (value: string | void): string | null =>
  value != null && value !== "" ? value : null;

const has = (haystack: string, needle: string): boolean => haystack.toLowerCase().includes(needle);

/** The level `COLORTERM` and `TERM` advertise, before any switch is applied. */
function declaredLevel(env: TerminalEnv): ColorLevel {
  const colorterm = nonEmpty(env.COLORTERM);
  if (colorterm != null && (has(colorterm, "truecolor") || has(colorterm, "24bit"))) {
    return "truecolor";
  }
  const term = nonEmpty(env.TERM);
  if (term == null) {
    return "ansi16";
  }
  if (has(term, "direct")) {
    return "truecolor";
  }
  return has(term, "256") ? "ansi256" : "ansi16";
}

/** `FORCE_COLOR`, which both disables colour (`0`) and picks a level (`1`–`3`). */
function forceColorLevel(env: TerminalEnv): ColorLevel | null {
  const raw = env.FORCE_COLOR;
  if (raw == null) {
    return null;
  }
  const value = raw.trim();
  if (value === "0" || value === "false") {
    return "none";
  }
  if (value === "2") {
    return "ansi256";
  }
  if (value === "3") {
    return "truecolor";
  }
  const declared = declaredLevel(env);
  return rank(declared) > rank("ansi16") ? declared : "ansi16";
}

/**
 * Whether the locale says this terminal understands UTF-8.
 *
 * An *unset* locale counts as yes. It is the common case on macOS and inside
 * container images that render UTF-8 perfectly well, and treating it as a
 * downgrade signal would give a plain-ASCII interface to most of the people
 * who would rather have the box-drawing characters.
 */
function utf8Locale(env: TerminalEnv): boolean {
  const locale = nonEmpty(env.LC_ALL) ?? nonEmpty(env.LC_CTYPE) ?? nonEmpty(env.LANG);
  if (locale == null) {
    return true;
  }
  return has(locale, "utf-8") || has(locale, "utf8");
}

/**
 * Resolve capability from a choice, a stream classification, and an
 * environment.
 *
 * Precedence, highest first — this list is
 * `crates/uf_term/src/capability.rs`'s, and changing it here alone is how the
 * CLI and the library start disagreeing:
 *
 * 1. an explicit `"never"` or `"always"`
 * 2. `NO_COLOR` (any non-empty value)
 * 3. `FORCE_COLOR`
 * 4. `CLICOLOR_FORCE`
 * 5. `TERM=dumb`
 * 6. `CLICOLOR=0`
 * 7. whether the stream is a terminal
 * 8. `COLORTERM` / `TERM`
 */
export function detectCapabilities(choice: ColorChoice, tty: Tty, env: TerminalEnv): Capabilities {
  const dumb = env.TERM === "dumb";
  return {
    color: detectColor(choice, tty, env, dumb),
    // Glyphs do not follow colour. A terminal can be perfectly capable of
    // UTF-8 while its output is being piped into a file, and replacing a box
    // border with `+---+` in that file helps nobody.
    glyphs: dumb || !utf8Locale(env) ? "ascii" : "unicode",
    tty,
  };
}

function detectColor(choice: ColorChoice, tty: Tty, env: TerminalEnv, dumb: boolean): ColorLevel {
  if (choice === "never") {
    return "none";
  }
  if (choice === "always") {
    const declared = declaredLevel(env);
    return rank(declared) > rank("ansi16") ? declared : "ansi16";
  }
  if (nonEmpty(env.NO_COLOR) != null) {
    return "none";
  }
  const forced = forceColorLevel(env);
  if (forced != null) {
    return forced;
  }
  const clicolorForce = nonEmpty(env.CLICOLOR_FORCE);
  if (clicolorForce != null && clicolorForce !== "0") {
    const declared = declaredLevel(env);
    return rank(declared) > rank("ansi16") ? declared : "ansi16";
  }
  if (dumb) {
    return "none";
  }
  if (nonEmpty(env.CLICOLOR) === "0") {
    return "none";
  }
  if (tty === "piped") {
    return "none";
  }
  return declaredLevel(env);
}

/**
 * The most conservative capability there is.
 *
 * What a redirected stream and a snapshot test both use: no escape sequences
 * at all, ASCII glyphs, nobody watching.
 */
export function plainCapabilities(): Capabilities {
  return { color: "none", glyphs: "ascii", tty: "piped" };
}

/**
 * The border characters this terminal can print.
 *
 * OpenTUI's four border styles, plus the ASCII fallback that is the whole
 * reason this is a lookup rather than a constant. The ASCII set is not a
 * different design, it is the same design drawn with the characters a
 * `TERM=dumb` terminal will not replace with a question mark: every glyph
 * below is one column wide in both vocabularies, so a box's geometry does not
 * change when its characters do.
 */
export type BorderStyle = "single" | "double" | "rounded" | "heavy";

/** Eight characters: the four corners, then top, right, bottom, left. */
export type BorderGlyphs = {
  readonly topLeft: string,
  readonly topRight: string,
  readonly bottomLeft: string,
  readonly bottomRight: string,
  readonly top: string,
  readonly right: string,
  readonly bottom: string,
  readonly left: string,
};

const ASCII_BORDER: BorderGlyphs = {
  topLeft: "+",
  topRight: "+",
  bottomLeft: "+",
  bottomRight: "+",
  top: "-",
  right: "|",
  bottom: "-",
  left: "|",
};

const UNICODE_BORDERS: { [BorderStyle]: BorderGlyphs } = {
  single: {
    topLeft: "┌",
    topRight: "┐",
    bottomLeft: "└",
    bottomRight: "┘",
    top: "─",
    right: "│",
    bottom: "─",
    left: "│",
  },
  double: {
    topLeft: "╔",
    topRight: "╗",
    bottomLeft: "╚",
    bottomRight: "╝",
    top: "═",
    right: "║",
    bottom: "═",
    left: "║",
  },
  rounded: {
    topLeft: "╭",
    topRight: "╮",
    bottomLeft: "╰",
    bottomRight: "╯",
    top: "─",
    right: "│",
    bottom: "─",
    left: "│",
  },
  heavy: {
    topLeft: "┏",
    topRight: "┓",
    bottomLeft: "┗",
    bottomRight: "┛",
    top: "━",
    right: "┃",
    bottom: "━",
    left: "┃",
  },
};

/** The border glyphs for a style, downgraded to ASCII when they cannot be printed. */
export function borderGlyphs(style: BorderStyle, glyphs: GlyphSet): BorderGlyphs {
  return glyphs === "ascii" ? ASCII_BORDER : UNICODE_BORDERS[style];
}
