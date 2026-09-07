// @flow
//
// The two things you can point a rendered tree at: a terminal, or memory.
//
// Everything below this module is a pure function of a tree and a size.
// Everything a real terminal needs — raw mode, an alternate screen, a resize
// signal, bytes on a file descriptor — is here and nowhere else, which is what
// makes the in-memory renderer a first-class way to run an application rather
// than a mock of one. `testRender` and `render` mount the same tree through
// the same reconciler and produce the same frames; they differ in where the
// frames go and in who presses the keys.
//
// # Why the cursor is never used to draw
//
// `crates/uf_term/src/prompt/draw.rs` ends every line of its frames with
// `\r\n` and explains why: raw mode turns off the mapping that makes a bare
// line feed also return to column zero, so a menu drawn with `\n` walks off
// the right edge one row at a time. This renderer avoids that class of bug by
// never writing a newline at all. Every cell it writes is preceded by an
// absolute cursor position, so no frame depends on where the cursor was left,
// on whether the terminal wraps at the right margin, or on the line-ending
// translation the mode happens to be in.
//
// # A terminal nobody is watching gets text, not escapes
//
// Piping a TUI into a file or a CI log has one sensible answer, and it is not
// "the same escape sequences". A log is read afterwards, in order, by
// something that does not implement cursor addressing — so an incremental
// renderer writing into one produces a file full of `ESC[12;40H`. Here, a
// non-interactive stream gets no escapes and no incremental updates at all:
// the final frame is written once, as plain lines, when the application stops.
// That is the same answer `uf_term`'s progress bars give, for the same reason.

import * as React from "@uniflowed/react";

import type { Capabilities, ColorChoice, TerminalEnv } from "./capability.js";
import { FALLBACK_COLUMNS, FALLBACK_ROWS, detectCapabilities, detectSize } from "./capability.js";
import type { Frame } from "./cells.js";
import { frameText } from "./cells.js";
import type { Update } from "./diff.js";
import type { Renderer } from "./internal/host.js";
import {
  RendererContext,
  createRenderer,
  createRoot,
  nextUpdate,
  pressKey,
  pressMouse,
  renderFrame,
  resize,
} from "./internal/host.js";
import type { InputEvent } from "./keys.js";
import { createInputDecoder } from "./keys.js";

/** Enter the alternate screen buffer, so the shell's scrollback survives. */
const ENTER_ALTERNATE = "\u001b[?1049h";
/** Leave it, putting back whatever the reader was looking at. */
const LEAVE_ALTERNATE = "\u001b[?1049l";
/** Hide the terminal's own cursor; the frame draws its own where it wants one. */
const HIDE_CURSOR = "\u001b[?25l";
const SHOW_CURSOR = "\u001b[?25h";
/** Clear the screen and put the cursor at the top left. */
const CLEAR = "\u001b[2J\u001b[H";
/**
 * Ask the terminal to bracket pasted text.
 *
 * Without this a paste is indistinguishable from very fast typing, which is
 * how pasting two lines into a prompt runs the first one: the `\r` between
 * them is delivered as Enter. With it the text arrives wrapped in `ESC[200~`
 * and `ESC[201~`, and `keys.js` turns the whole block into one `"paste"`
 * event. Turned off again on the way out, because a terminal left in this mode
 * hands the *shell* its own escape sequences around every paste.
 */
const ENABLE_PASTE = "\u001b[?2004h";
const DISABLE_PASTE = "\u001b[?2004l";
/**
 * Ask the terminal to report the mouse, in the four modes that answer.
 *
 * `?1000h` turns reporting on at all — presses and releases. `?1002h` adds
 * motion while a button is held, which is what makes a drag a sequence rather
 * than a press and a release somewhere else. `?1003h` adds motion with nothing
 * held, which is the only way `over` and `out` can fire before a reader has
 * clicked anything; it is the expensive one, since crossing the screen is a
 * report per cell, and it is included because a hover that only worked
 * mid-drag would not be a hover. `?1006h` asks for the SGR encoding, which is
 * the one `mouse.js` decodes and the only one that works past column 223.
 *
 * Turned off in the reverse order on the way out, and turned on only when the
 * application asked for the mouse: a terminal in these modes stops doing its
 * own click-and-drag text selection, so an application that does not read the
 * mouse must not take that away from the reader.
 */
const ENABLE_MOUSE = "\u001b[?1000h\u001b[?1002h\u001b[?1003h\u001b[?1006h";
const DISABLE_MOUSE = "\u001b[?1006l\u001b[?1003l\u001b[?1002l\u001b[?1000l";

/** What `render` gives back. */
export type Handle = {
  /** Put the terminal back the way it was found and unmount the tree. */
  stop(): void,
  /** The frame currently on the screen. */
  frame(): Frame,
  /** That frame as text, which is what a snapshot asserts on. */
  text(): string,
};

/** What `testRender` gives back: a `Handle`, plus the terminal's side. */
export type TestHandle = {
  ...Handle,
  /** Feed raw terminal input, as a terminal would deliver it. */
  press(input: string): void,
  /** Draw the next frame and report what writing it would cost. */
  update(): Update,
  /** Resize the terminal, discarding what was on it. */
  resize(width: number, height: number): void,
  /** Every update produced since mounting, in order. */
  updates(): $ReadOnlyArray<Update>,
};

/** Anything that can be written to; `process.stdout`, or a string collector. */
export type OutputStream = {
  write(chunk: string): mixed,
  readonly columns?: number,
  readonly rows?: number,
  readonly isTTY?: boolean,
  /** A real `process.stdout` emits `"resize"`; a string collector does not. */
  on?: (event: string, listener: () => mixed) => mixed,
  off?: (event: string, listener: () => mixed) => mixed,
  ...
};

/** Anything keys arrive from; `process.stdin`. */
export type InputStream = {
  readonly isTTY?: boolean,
  setRawMode?: (raw: boolean) => mixed,
  resume?: () => mixed,
  pause?: () => mixed,
  setEncoding?: (encoding: string) => mixed,
  on?: (event: string, listener: (chunk: string) => mixed) => mixed,
  off?: (event: string, listener: (chunk: string) => mixed) => mixed,
  ...
};

/** How to mount onto a real terminal. */
export type RenderOptions = {
  readonly stdin?: InputStream,
  readonly stdout?: OutputStream,
  /** `--color`, when the application has such a flag. */
  readonly color?: ColorChoice,
  /** The environment to detect from. Defaults to the process's. */
  readonly env?: TerminalEnv,
  /**
   * Whether to take over the whole screen.
   *
   * On by default because a full-screen application that scrolls the shell's
   * history away has destroyed something it cannot put back. Off for an
   * application that wants to leave its last frame in the scrollback, which is
   * what a progress display wants.
   */
  readonly alternateScreen?: boolean,
  /**
   * Whether to ask the terminal to report the mouse.
   *
   * Off by default, which is a deliberate difference from OpenTUI's renderer.
   * Mouse reporting is not free to a *reader*: a terminal in it stops handling
   * click-and-drag itself, so selecting a line to copy out of an application
   * that ignores the mouse anyway needs a modifier key the reader has to know
   * about. An application that handles the mouse is trading that away on
   * purpose; one that does not should not trade it away by default.
   */
  readonly mouse?: boolean,
};

/**
 * Hand one decoded event to the renderer.
 *
 * The two drivers — a terminal and a test — read the same decoder and so face
 * the same union, and routing it in one place is what keeps them from drifting
 * into two answers about what a mouse report does.
 */
function deliver(renderer: Renderer, event: InputEvent): void {
  if (event.kind === "mouse") {
    pressMouse(renderer, event);
    return;
  }
  pressKey(renderer, event);
}

/** Mount a tree into a renderer and return the pieces both drivers need. */
function mount(element: React.Node, renderer: Renderer) {
  const root = createRoot(renderer);
  root.render(React.createElement(RendererContext.Provider, { value: renderer }, element));
  return root;
}

/**
 * Render into memory.
 *
 * The way an application is *tested*, and the way one is rendered anywhere
 * that is not a terminal. No environment is read, no stream is touched, and
 * the capabilities are the caller's to choose — which is the point: a test
 * asserting how a box degrades on a terminal with no colour should not have to
 * arrange for the machine running it to have no colour.
 */
export function testRender(
  element: React.Node,
  options: {
    readonly width?: number,
    readonly height?: number,
    readonly capabilities?: Capabilities,
    readonly mouse?: boolean,
  } = {},
): TestHandle {
  const width = options.width ?? FALLBACK_COLUMNS;
  const height = options.height ?? FALLBACK_ROWS;
  const capabilities: Capabilities = options.capabilities ?? {
    color: "truecolor",
    glyphs: "unicode",
    tty: "interactive",
  };
  // On by default here and off in `render`, and the difference is the whole
  // reason the option exists: what `render` weighs is a terminal it would take
  // click-and-drag selection away from, and there is no terminal here. A test
  // that presses the mouse should not have to remember to enable it, and one
  // that wants to assert an application ignores the mouse can say so.
  const renderer = createRenderer(width, height, capabilities, options.mouse ?? true);
  const root = mount(element, renderer);
  const produced: Array<Update> = [];

  // The same decoder a real terminal driver holds, for the same reason: a
  // test that delivers a paste in two `press` calls is testing what an
  // operating system does to a large one.
  const decoder = createInputDecoder();

  const handle: TestHandle = {
    press(input: string) {
      for (const event of decoder.push(input)) {
        deliver(renderer, event);
      }
    },
    update() {
      const next = nextUpdate(renderer);
      produced.push(next);
      return next;
    },
    updates() {
      return produced;
    },
    resize(nextWidth: number, nextHeight: number) {
      resize(renderer, nextWidth, nextHeight);
    },
    frame() {
      return renderFrame(renderer);
    },
    text() {
      return frameText(renderFrame(renderer));
    },
    stop() {
      root.unmount();
    },
  };
  return handle;
}

/**
 * Render onto a terminal.
 *
 * Returns as soon as the first frame is on the screen; the application keeps
 * running because stdin is open, and stops when the caller calls `stop()`.
 * That is deliberate — a `render` that never returned would make the calling
 * program unable to do anything else, including install the signal handler
 * that has to call `stop()`.
 */
export function render(element: React.Node, options: RenderOptions = {}): Handle {
  // Three casts, and the same reason for all of them: Flow's library
  // definition for `process` describes Node's classes, and these types
  // describe the three things this renderer actually needs — so that a test
  // can pass a string collector, and so that a runtime whose streams are not
  // Node's is not excluded by a type. The narrowing is checked at run time by
  // the `!= null` guards below rather than trusted.
  const stdout: OutputStream = options.stdout ?? (process.stdout: $FlowFixMe);
  const stdin: InputStream = options.stdin ?? (process.stdin: $FlowFixMe);
  const env: TerminalEnv = options.env ?? (process.env: $FlowFixMe);
  const capabilities = detectCapabilities(
    options.color ?? "auto",
    stdout.isTTY === true ? "interactive" : "piped",
    env,
  );
  const interactive = capabilities.tty === "interactive";
  const alternateScreen = (options.alternateScreen ?? true) && interactive;

  // How big the terminal is, by the same rules `uf`'s own CLI resolves it
  // with: `COLUMNS`/`LINES` first, then what the stream reports, then 80 by
  // 24. Reading `stdout.columns` alone was one of the two renderers deciding
  // the terminal's shape its own way — the thing the other duplications in
  // this package exist to prevent.
  const size = detectSize(env, stdout);
  const mouse = (options.mouse ?? false) && interactive;
  const renderer = createRenderer(size.columns, size.rows, capabilities, mouse);

  let stopped = false;
  let scheduled = false;

  const draw = () => {
    if (stopped || !interactive) {
      return;
    }
    const update = nextUpdate(renderer);
    if (update.output !== "") {
      stdout.write(update.output);
    }
  };

  // One draw per turn of the event loop, however many commits happened in it.
  // A component that sets three pieces of state in one handler commits three
  // times, and drawing three frames means writing two of them to a terminal
  // nobody ever saw.
  renderer.onCommit = () => {
    if (scheduled || stopped) {
      return;
    }
    scheduled = true;
    queueMicrotask(() => {
      scheduled = false;
      draw();
    });
  };

  if (interactive) {
    stdout.write(
      (alternateScreen ? ENTER_ALTERNATE : "") +
        HIDE_CURSOR +
        ENABLE_PASTE +
        (mouse ? ENABLE_MOUSE : "") +
        CLEAR,
    );
  }

  const decoder = createInputDecoder();
  const onData = (chunk: string) => {
    for (const event of decoder.push(String(chunk))) {
      deliver(renderer, event);
    }
    draw();
  };

  const onResize = () => {
    // Resolved again rather than read off the stream, so that an application
    // told its size explicitly keeps it. `process.env` is a snapshot taken
    // when the process started and a shell does not export `COLUMNS` anyway,
    // so this only pins the size for somebody who set it on purpose.
    const next = detectSize(env, stdout);
    resize(renderer, next.columns, next.rows);
    draw();
  };

  const root = mount(element, renderer);
  draw();

  if (interactive && stdin.on != null) {
    if (stdin.isTTY === true && stdin.setRawMode != null) {
      stdin.setRawMode(true);
    }
    if (stdin.setEncoding != null) {
      stdin.setEncoding("utf8");
    }
    if (stdin.resume != null) {
      stdin.resume();
    }
    stdin.on("data", onData);
  }
  if (interactive && stdout.on != null) {
    stdout.on("resize", onResize);
  }

  return {
    stop() {
      if (stopped) {
        return;
      }
      stopped = true;
      // The last frame, read *before* the tree comes down. Unmounting empties
      // the tree, and a frame rendered from an empty tree is a rectangle of
      // spaces — which is exactly what a redirected stream received until this
      // line existed, and exactly what no test that only drove a terminal
      // would have noticed.
      const farewell = interactive ? "" : `${frameText(renderFrame(renderer))}\n`;
      // The tree comes down before the terminal is restored, so that effect
      // cleanups run while the terminal is still in the state they were set up
      // in. Restoring first is how a cleanup that writes a farewell line ends
      // up writing it into the alternate screen, a millisecond before that
      // screen is thrown away.
      root.unmount();
      if (stdin.off != null) {
        stdin.off("data", onData);
      }
      if (stdout.off != null) {
        stdout.off("resize", onResize);
      }
      if (interactive) {
        if (stdin.isTTY === true && stdin.setRawMode != null) {
          stdin.setRawMode(false);
        }
        if (stdin.pause != null) {
          stdin.pause();
        }
        stdout.write(
          (mouse ? DISABLE_MOUSE : "") +
            DISABLE_PASTE +
            SHOW_CURSOR +
            (alternateScreen ? LEAVE_ALTERNATE : "\n"),
        );
      } else {
        // Nobody was watching, so nothing has been written yet. The last frame
        // goes out once, as text, which is what a log can carry.
        stdout.write(farewell);
      }
    },
    frame() {
      return renderFrame(renderer);
    },
    text() {
      return frameText(renderFrame(renderer));
    },
  };
}
