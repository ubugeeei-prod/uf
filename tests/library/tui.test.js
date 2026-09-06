// @flow
//
// `@uniflowed/tui`.
//
// Four claims, and a section for each: a tree renders to the cells it should,
// flexbox behaves like flexbox, the diff writes only what changed, and a key
// press reaches the thing that has focus. They are the four things the package
// promises and the four things a refactor can break without breaking anything
// a smoke test would notice.
//
// Every assertion here is on a *frame* or on the bytes that would put one on a
// terminal, and every key arrives as the bytes a terminal actually sends —
// `\u001b[D` and not `{ name: "left" }`. That is the difference between
// testing this renderer and testing a picture of it: an escape-sequence
// decoder that has never seen an escape sequence passes any number of tests
// about `KeyEvent` objects somebody constructed by hand.
//
// The last section is not about rendering at all. It reads the Rust width
// tables and asserts the JavaScript ones are the same data, because two copies
// of a Unicode table that nothing compares are two copies that will differ.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { describe, expect, fn, it } from "@uniflowed/test";
import {
  Attributes,
  Box,
  INHERIT,
  Input,
  ScrollBox,
  Text,
  createKeyDecoder,
  decodeKeys,
  detectCapabilities,
  detectSize,
  frameRow,
  parseColor,
  render,
  testRender,
  useKeyboard,
  useTerminalSize,
} from "@uniflowed/tui";
import type { Frame } from "@uniflowed/tui";

import { HEIGHT, START, STEPS, WIDTH, lines } from "../../tools/bench/tui/workload.js";

/** The repository root, for the tests that read Rust source. */
const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");

/** The rows of a frame, so a failure prints a picture rather than one string. */
const rows = (frame: Frame): Array<string> => {
  const out = [];
  for (let y = 0; y < frame.height; y += 1) {
    out.push(frameRow(frame, y));
  }
  return out;
};

/** The style of one cell, which is what a colour assertion is really about. */
const cell = (frame: Frame, x: number, y: number) => {
  const index = y * frame.width + x;
  return {
    char: frame.chars[index],
    fg: frame.fg[index],
    bg: frame.bg[index],
    attributes: frame.attributes[index],
  };
};

describe("a tree, as cells", () => {
  it("puts a string where layout put its text node", () => {
    const handle = testRender(<Text>ready</Text>, { width: 8, height: 2 });
    expect(rows(handle.frame())).toEqual(["ready   ", "        "]);
    handle.stop();
  });

  it("stacks children in a column, which is the terminal default", () => {
    const handle = testRender(
      <Box>
        <Text>one</Text>
        <Text>two</Text>
      </Box>,
      { width: 5, height: 3 },
    );
    expect(rows(handle.frame())).toEqual(["one  ", "two  ", "     "]);
    handle.stop();
  });

  it("draws a border, a title, and the content inside both", () => {
    // The title sits in the border run with no padding around it, which is how
    // OpenTUI renders one — the border characters run up to the text.
    const handle = testRender(
      <Box border={true} borderStyle="rounded" title="uf" titleAlignment="center" padding={1}>
        <Text>ok</Text>
      </Box>,
      { width: 10, height: 5 },
    );
    expect(rows(handle.frame())).toEqual([
      "╭───uf───╮",
      "│        │",
      "│ ok     │",
      "│        │",
      "╰────────╯",
    ]);
    handle.stop();
  });

  it("carries a colour and an attribute down to the cell", () => {
    const handle = testRender(
      <Text fg="#ff0000">
        a<Text bold={true}>b</Text>
      </Text>,
      { width: 3, height: 1 },
    );
    const frame = handle.frame();
    expect(cell(frame, 0, 0)).toEqual({
      char: "a",
      fg: 0xff0000,
      bg: INHERIT,
      attributes: Attributes.NONE,
    });
    // The nested node inherited the colour and added the weight, rather than
    // replacing the style wholesale.
    expect(cell(frame, 1, 0)).toEqual({
      char: "b",
      fg: 0xff0000,
      bg: INHERIT,
      attributes: Attributes.BOLD,
    });
    handle.stop();
  });

  it("gives a two-column grapheme two cells, the second of them a continuation", () => {
    const handle = testRender(<Text>A界B</Text>, { width: 5, height: 1 });
    const frame = handle.frame();
    expect(frame.chars.slice(0, 5)).toEqual(["A", "界", "", "B", " "]);
    // The row a reader sees is four columns of text, not five characters.
    expect(frameRow(frame, 0)).toBe("A界B ");
    handle.stop();
  });

  it("wraps at a word boundary and drops the space it broke on", () => {
    const handle = testRender(<Text>alpha beta gamma</Text>, { width: 11, height: 3 });
    expect(rows(handle.frame())).toEqual(["alpha beta ", "gamma      ", "           "]);
    handle.stop();
  });

  it("breaks a word longer than the line rather than letting it overflow", () => {
    const handle = testRender(<Text>unbreakable</Text>, { width: 5, height: 3 });
    expect(rows(handle.frame())).toEqual(["unbre", "akabl", "e    "]);
    handle.stop();
  });

  it("clips a child to the box when the box hides its overflow", () => {
    const handle = testRender(
      <Box width={4} height={1} overflow="hidden">
        <Text wrap="none">overlong</Text>
      </Box>,
      { width: 8, height: 1 },
    );
    expect(rows(handle.frame())).toEqual(["over    "]);
    handle.stop();
  });

  it("fills a background over what was under it", () => {
    const handle = testRender(<Box backgroundColor="#0000ff" width={3} height={1} />, {
      width: 4,
      height: 1,
    });
    const frame = handle.frame();
    expect(cell(frame, 0, 0).bg).toBe(0x0000ff);
    expect(cell(frame, 3, 0).bg).toBe(INHERIT);
    handle.stop();
  });
});

describe("layout is flexbox", () => {
  it("lays a row out along the main axis", () => {
    const handle = testRender(
      <Box flexDirection="row" gap={1}>
        <Text>ab</Text>
        <Text>cd</Text>
      </Box>,
      { width: 6, height: 1 },
    );
    expect(rows(handle.frame())).toEqual(["ab cd "]);
    handle.stop();
  });

  it("divides the free space between growing children, losing no column", () => {
    // Ten columns between three equal claims is 3⅓ each, and a terminal has no
    // third of a column. The parts must still sum to the whole.
    const handle = testRender(
      <Box flexDirection="row" height={1} width={10}>
        <Box flexGrow={1} backgroundColor="#111111" />
        <Box flexGrow={1} backgroundColor="#222222" />
        <Box flexGrow={1} backgroundColor="#333333" />
      </Box>,
      { width: 10, height: 1 },
    );
    const frame = handle.frame();
    const widths = [0, 0, 0];
    for (let x = 0; x < 10; x += 1) {
      const bg = cell(frame, x, 0).bg;
      if (bg === 0x111111) widths[0] += 1;
      if (bg === 0x222222) widths[1] += 1;
      if (bg === 0x333333) widths[2] += 1;
    }
    expect(widths.reduce((a, b) => a + b, 0)).toBe(10);
    expect(widths).toEqual([3, 4, 3]);
    handle.stop();
  });

  it("honours justifyContent along the main axis", () => {
    const between = testRender(
      <Box flexDirection="row" justifyContent="space-between" width={9}>
        <Text>ab</Text>
        <Text>cd</Text>
      </Box>,
      { width: 9, height: 1 },
    );
    expect(rows(between.frame())).toEqual(["ab     cd"]);
    between.stop();

    const centred = testRender(
      <Box flexDirection="row" justifyContent="center" width={8}>
        <Text>ab</Text>
      </Box>,
      { width: 8, height: 1 },
    );
    expect(rows(centred.frame())).toEqual(["   ab   "]);
    centred.stop();
  });

  it("honours alignItems on the cross axis", () => {
    const handle = testRender(
      <Box flexDirection="row" alignItems="flex-end" height={3} width={3}>
        <Text>x</Text>
      </Box>,
      { width: 3, height: 3 },
    );
    expect(rows(handle.frame())).toEqual(["   ", "   ", "x  "]);
    handle.stop();
  });

  it("resolves a percentage against the containing block", () => {
    const handle = testRender(
      <Box flexDirection="row" height={1} width={10}>
        <Box width="30%" backgroundColor="#111111" />
      </Box>,
      { width: 10, height: 1 },
    );
    const frame = handle.frame();
    let painted = 0;
    for (let x = 0; x < 10; x += 1) {
      if (cell(frame, x, 0).bg === 0x111111) painted += 1;
    }
    expect(painted).toBe(3);
    handle.stop();
  });

  it("does not shrink a child whose width was given as a number", () => {
    // Two children asking for six columns each in a row of eight. The one with
    // an explicit width keeps it, because `flexShrink` defaults to 0 for a
    // numeric dimension, and the other gives up the difference.
    const handle = testRender(
      <Box flexDirection="row" height={1} width={8}>
        <Box width={6} backgroundColor="#111111" />
        <Box flexBasis={6} backgroundColor="#222222" />
      </Box>,
      { width: 8, height: 1 },
    );
    const frame = handle.frame();
    let fixed = 0;
    let flexible = 0;
    for (let x = 0; x < 8; x += 1) {
      if (cell(frame, x, 0).bg === 0x111111) fixed += 1;
      if (cell(frame, x, 0).bg === 0x222222) flexible += 1;
    }
    expect(fixed).toBe(6);
    expect(flexible).toBe(2);
    handle.stop();
  });

  it("takes padding and a border out of the space children get", () => {
    const handle = testRender(
      <Box border={true} padding={1} width={9} height={6}>
        <Text>abcdefghij</Text>
      </Box>,
      { width: 9, height: 6 },
    );
    // Nine columns minus two of border and two of padding is five for text,
    // and six rows minus the same is two — so ten characters is exactly two
    // wrapped lines, and the box is full.
    expect(rows(handle.frame())).toEqual([
      "┌───────┐",
      "│       │",
      "│ abcde │",
      "│ fghij │",
      "│       │",
      "└───────┘",
    ]);
    handle.stop();
  });

  it("re-lays out when the terminal is resized", () => {
    const handle = testRender(<Text>alpha beta</Text>, { width: 10, height: 2 });
    expect(rows(handle.frame())).toEqual(["alpha beta", "          "]);
    handle.resize(6, 2);
    expect(rows(handle.frame())).toEqual(["alpha ", "beta  "]);
    handle.stop();
  });

  it("tells a component the size through useTerminalSize", () => {
    component Size() {
      const { width, height } = useTerminalSize();
      return <Text>{`${width}x${height}`}</Text>;
    }
    const handle = testRender(<Size />, { width: 8, height: 1 });
    expect(rows(handle.frame())).toEqual(["8x1     "]);
    handle.resize(12, 3);
    expect(frameRow(handle.frame(), 0)).toBe("12x3        ");
    handle.stop();
  });
});

describe("a window onto more than fits", () => {
  /** `count` numbered lines, which is what a log looks like to a renderer. */
  const log = (count: number) =>
    Array.from({ length: count }, (_, index) =>
      React.createElement(Text, { key: String(index), wrap: "none" }, `line ${index}`),
    );

  it("shows the rows the offset asks for and none of the others", () => {
    const handle = testRender(
      <ScrollBox height={3} width={8} scrollbar={false} scrollTop={2}>
        {log(6)}
      </ScrollBox>,
      { width: 8, height: 4 },
    );
    expect(rows(handle.frame())).toEqual(["line 2  ", "line 3  ", "line 4  ", "        "]);
    handle.stop();
  });

  it("takes the height a parent gives it, so a log fills what is left", () => {
    // The shape a real application has: a header, a footer, and a scrolling
    // region that is whatever is between them. The guide says a `ScrollBox`
    // needs a bounded height "explicitly, or with `flexGrow` inside a parent
    // that has one", and this is the second half of that sentence.
    //
    // The `flexShrink={0}` on the two lines is flexbox rather than this
    // component: a scrolling box asks for its whole content's height, so a
    // header that may shrink will shrink, exactly as it would in CSS.
    const handle = testRender(
      <Box height={4} width={8}>
        <Text flexShrink={0} wrap="none">
          head
        </Text>
        <ScrollBox flexGrow={1} scrollbar={false} scrollTop={Number.MAX_SAFE_INTEGER}>
          {log(9)}
        </ScrollBox>
        <Text flexShrink={0} wrap="none">
          foot
        </Text>
      </Box>,
      { width: 8, height: 4 },
    );
    expect(rows(handle.frame())).toEqual(["head    ", "line 7  ", "line 8  ", "foot    "]);
    handle.stop();
  });

  it("clamps the offset to the content, so the largest number means the end", () => {
    // A log that has just grown by a line follows its tail by asking for a row
    // number it cannot know. Clamping in layout is what makes that legal.
    const handle = testRender(
      <ScrollBox height={2} width={8} scrollbar={false} scrollTop={Number.MAX_SAFE_INTEGER}>
        {log(5)}
      </ScrollBox>,
      { width: 8, height: 2 },
    );
    expect(rows(handle.frame())).toEqual(["line 3  ", "line 4  "]);

    handle.stop();
  });

  it("never scrolls above the first row", () => {
    const handle = testRender(
      <ScrollBox height={2} width={8} scrollbar={false} scrollTop={-40}>
        {log(5)}
      </ScrollBox>,
      { width: 8, height: 2 },
    );
    expect(rows(handle.frame())).toEqual(["line 0  ", "line 1  "]);
    handle.stop();
  });

  it("cuts a child the window only half reaches", () => {
    // The first child is two wrapped lines; the window starts on its second.
    // A child does not know it was cut — the window cuts it — which is what
    // makes a paragraph scroll the same way a list of one-line rows does.
    const handle = testRender(
      <ScrollBox height={2} width={5} scrollbar={false} scrollTop={1}>
        <Text>a b c d</Text>
        <Text>zzz</Text>
      </ScrollBox>,
      { width: 5, height: 3 },
    );
    expect(rows(handle.frame())).toEqual(["c d  ", "zzz  ", "     "]);
    handle.stop();
  });

  it("draws a bar whose thumb says where in the content the window is", () => {
    const top = testRender(
      <ScrollBox height={4} width={8} scrollTop={0}>
        {log(16)}
      </ScrollBox>,
      { width: 8, height: 4 },
    );
    expect(rows(top.frame())).toEqual(["line 0 █", "line 1 │", "line 2 │", "line 3 │"]);
    top.stop();

    const bottom = testRender(
      <ScrollBox height={4} width={8} scrollTop={Number.MAX_SAFE_INTEGER}>
        {log(16)}
      </ScrollBox>,
      { width: 8, height: 4 },
    );
    expect(rows(bottom.frame())).toEqual(["line 12│", "line 13│", "line 14│", "line 15█"]);
    bottom.stop();
  });

  it("keeps the caller's padding and puts the bar beside it", () => {
    // The bar takes a column of its own. A `ScrollBox` that was padded by one
    // and given a bar has to end up padded by one *and* have a bar, or every
    // layout that had a border and a bar loses a column of content to it.
    const handle = testRender(
      <ScrollBox height={4} width={10} padding={1} scrollTop={0}>
        {log(8)}
      </ScrollBox>,
      { width: 10, height: 4 },
    );
    expect(rows(handle.frame())).toEqual(["          ", " line 0 █ ", " line 1 │ ", "          "]);
    handle.stop();
  });

  it("draws no bar when everything already fits", () => {
    // A full-height thumb beside content that does not scroll is a control
    // that lies about being one.
    const handle = testRender(
      <ScrollBox height={4} width={8}>
        {log(2)}
      </ScrollBox>,
      { width: 8, height: 4 },
    );
    expect(rows(handle.frame())).toEqual(["line 0  ", "line 1  ", "        ", "        "]);
    handle.stop();
  });

  it("puts the bar inside the border, not on it", () => {
    // The column the bar goes in is the one the box reserved, which is inside
    // the border and outside the content — so the track and the right-hand
    // border are two adjacent columns of the same character and are still two
    // different things.
    const handle = testRender(
      <ScrollBox border={true} borderStyle="rounded" height={5} width={10} scrollTop={0}>
        {log(9)}
      </ScrollBox>,
      { width: 10, height: 5 },
    );
    expect(rows(handle.frame())).toEqual([
      "╭────────╮",
      "│line 0 █│",
      "│line 1 ││",
      "│line 2 ││",
      "╰────────╯",
    ]);
    handle.stop();
  });

  it("draws the bar in the vocabulary the terminal has", () => {
    const handle = testRender(
      <ScrollBox height={2} width={8} scrollTop={0}>
        {log(8)}
      </ScrollBox>,
      {
        width: 8,
        height: 2,
        capabilities: { color: "none", glyphs: "ascii", tty: "interactive" },
      },
    );
    expect(rows(handle.frame())).toEqual(["line 0 #", "line 1 |"]);
    handle.stop();
  });

  it("costs a screenful for ten thousand lines, and one cell to change one", () => {
    // The claim the component exists for, measured the way the diff's own
    // claim is. Ten thousand rows in a twenty-four row terminal: the first
    // frame is the terminal, not the log, and editing a visible row is one
    // cell — a renderer that painted the whole content and clipped it would
    // pass neither.
    component Log() {
      const [marker, setMarker] = useState<string>("");
      useKeyboard(() => setMarker("!"));
      return (
        <ScrollBox height={24} width={80} scrollbar={false} scrollTop={3_990}>
          {Array.from({ length: 10_000 }, (_, index) =>
            React.createElement(
              Text,
              { key: String(index), wrap: "none" },
              index === 4_000 ? `line ${index}${marker}` : `line ${index}`,
            ),
          )}
        </ScrollBox>
      );
    }

    const handle = testRender(<Log />, { width: 80, height: 24 });
    const first = handle.update();
    expect(first.cells).toBe(80 * 24);
    expect(frameRow(handle.frame(), 10)).toBe(`line 4000${" ".repeat(71)}`);

    handle.press("x");
    const second = handle.update();
    expect(second.cells).toBe(1);
    expect(frameRow(handle.frame(), 10)).toBe(`line 4000!${" ".repeat(70)}`);
    handle.stop();
  });

  it("does not paint what the window does not reach", () => {
    // The other half of the same property, and the one a clipping renderer
    // would fail: a child outside the window is not merely invisible, it is
    // never walked into. The box under it would otherwise be drawn at last
    // frame's coordinates.
    const handle = testRender(
      <ScrollBox height={1} width={9} scrollbar={false} scrollTop={0}>
        <Text>visible</Text>
        <Box backgroundColor="#ff0000" height={1}>
          <Text>hidden</Text>
        </Box>
      </ScrollBox>,
      { width: 9, height: 2 },
    );
    const frame = handle.frame();
    expect(rows(frame)).toEqual(["visible  ", "         "]);
    for (let x = 0; x < 9; x += 1) {
      expect(cell(frame, x, 1).bg).toBe(INHERIT);
    }
    handle.stop();
  });
});

describe("the diff writes only what changed", () => {
  /** An application whose one character changes when a key is pressed. */
  component Counter() {
    const [n, setN] = useState<number>(0);
    useKeyboard(() => setN((previous) => previous + 1));
    return (
      <Box>
        <Text>{`count ${n}`}</Text>
        <Text>a line that does not change</Text>
        <Text>another line that does not change</Text>
      </Box>
    );
  }

  it("sends every cell of the first frame and one cell of the second", () => {
    const handle = testRender(<Counter />, { width: 40, height: 6 });
    const first = handle.update();
    expect(first.cells).toBe(40 * 6);

    handle.press("x");
    const second = handle.update();
    expect(second.cells).toBe(1);
    handle.stop();
  });

  it("costs a dozen bytes to change a character, not a screenful", () => {
    // The measurement behind the performance claim. A renderer that reprints
    // from the changed line to the bottom of the frame — which is what a
    // line-diffing renderer does, React Ink included — has to send everything
    // below the change; here the change is the change.
    const handle = testRender(<Counter />, { width: 80, height: 24 });
    const full = handle.update();
    handle.press("x");
    const incremental = handle.update();

    expect(full.cells).toBe(1920);
    expect(incremental.cells).toBe(1);
    // Seven bytes: `ESC [ 1 ; 7 H` and the character. The exact figure is
    // asserted rather than a bound, because a regression here is a renderer
    // that started sending more than it had to, and nobody would notice a
    // bound that said "under a hundred".
    expect(incremental.output.length).toBe(7);
    expect(full.output.length).toBeGreaterThan(1900);
    handle.stop();
  });

  it("writes nothing at all when nothing changed", () => {
    const handle = testRender(<Text>still</Text>, { width: 10, height: 2 });
    handle.update();
    const again = handle.update();
    expect(again.output).toBe("");
    expect(again.cells).toBe(0);
    handle.stop();
  });

  it("repaints in full after a resize, because the terminal reflowed itself", () => {
    const handle = testRender(<Text>abc</Text>, { width: 6, height: 2 });
    handle.update();
    handle.resize(6, 3);
    const after = handle.update();
    expect(after.cells).toBe(18);
    handle.stop();
  });

  it("emits a colour once for a run rather than once per cell", () => {
    const handle = testRender(<Text fg="#ff0000">aaaa</Text>, {
      width: 4,
      height: 1,
    });
    const update = handle.update();
    // One SGR for the run, one reset at the end.
    expect(update.output.split("\u001b[").length - 1).toBe(3);
    handle.stop();
  });

  it("writes no escape sequences at all on a terminal that takes none", () => {
    const handle = testRender(<Text fg="#ff0000">hi</Text>, {
      width: 2,
      height: 1,
      capabilities: { color: "none", glyphs: "ascii", tty: "interactive" },
    });
    const update = handle.update();
    expect(update.output.includes("m")).toBe(false);
    expect(update.output).toBe("\u001b[1;1Hhi");
    handle.stop();
  });
});

describe("input reaches what has focus", () => {
  it("decodes the bytes a terminal actually sends", () => {
    expect(decodeKeys("\u001b[A").map((key) => key.name)).toEqual(["up"]);
    expect(decodeKeys("\u001bOB").map((key) => key.name)).toEqual(["down"]);
    expect(decodeKeys("\u001b[3~").map((key) => key.name)).toEqual(["delete"]);
    expect(decodeKeys("\u001b").map((key) => key.name)).toEqual(["escape"]);
    expect(decodeKeys("\r").map((key) => key.name)).toEqual(["return"]);
    expect(decodeKeys("\u007f").map((key) => key.name)).toEqual(["backspace"]);

    const ctrlC = decodeKeys("\u0003")[0];
    expect([ctrlC.name, ctrlC.ctrl]).toEqual(["c", true]);

    const altA = decodeKeys("\u001ba")[0];
    expect([altA.name, altA.meta]).toEqual(["a", true]);

    const ctrlUp = decodeKeys("\u001b[1;5A")[0];
    expect([ctrlUp.name, ctrlUp.ctrl, ctrlUp.shift]).toEqual(["up", true, false]);

    const shiftTab = decodeKeys("\u001b[Z")[0];
    expect([shiftTab.name, shiftTab.shift]).toEqual(["tab", true]);
  });

  it("decodes a burst as several keys, because fast typing arrives as one chunk", () => {
    expect(decodeKeys("abc").map((key) => key.sequence)).toEqual(["a", "b", "c"]);
    expect(decodeKeys("a\u001b[Db").map((key) => key.name)).toEqual(["a", "left", "b"]);
  });

  it("delivers a paste as one block of text rather than as fast typing", () => {
    // Without this the `\r` between two pasted lines is Enter, which is how
    // pasting a two-line command into a prompt runs the first line.
    const keys = decodeKeys("\u001b[200~echo one\r\necho two\u001b[201~");

    expect(keys.map((key) => key.name)).toEqual(["paste"]);
    expect(keys[0].sequence).toBe("echo one\r\necho two");
    // Verbatim, including what an escape sequence in a clipboard looks like:
    // the point of knowing it was a paste is that none of it is interpreted.
    expect(decodeKeys("\u001b[200~\u001b[Ax\u001b[201~")[0].sequence).toBe("\u001b[Ax");
  });

  it("keeps the keys around a paste, and an empty paste is still an event", () => {
    expect(decodeKeys("a\u001b[200~b\u001b[201~c").map((key) => key.sequence)).toEqual([
      "a",
      "b",
      "c",
    ]);
    expect(decodeKeys("\u001b[200~\u001b[201~").map((key) => [key.name, key.sequence])).toEqual([
      ["paste", ""],
    ]);
  });

  it("waits for the rest of a paste the operating system split in two", () => {
    // A clipboard is as long as it is, and a large paste arrives in whatever
    // pieces the read gives — including one that ends in the middle of a word.
    const decoder = createKeyDecoder();

    expect(decoder.push("\u001b[200~alpha ")).toEqual([]);
    expect(decoder.push("beta")).toEqual([]);
    const finished = decoder.push("\u001b[201~!");
    expect(finished.map((key) => [key.name, key.sequence])).toEqual([
      ["paste", "alpha beta"],
      ["!", "!"],
    ]);
    expect(decoder.flush()).toEqual([]);
  });

  it("waits when a chunk stops part-way through the paste marker itself", () => {
    // The marker is six bytes and a read can end anywhere. Decoding
    // `ESC [ 2 0 0` on its own produces Alt-and-a-bracket followed by three
    // digits, and then the paste's first line runs as a command.
    const decoder = createKeyDecoder();

    expect(decoder.push("\u001b[2")).toEqual([]);
    expect(decoder.push("00~text\u001b[201~").map((key) => [key.name, key.sequence])).toEqual([
      ["paste", "text"],
    ]);

    // A lone escape is still the Escape key, though: holding it back would
    // mean Escape never fires until the next keystroke.
    expect(decoder.push("\u001b").map((key) => key.name)).toEqual(["escape"]);
  });

  it("gives back the bytes when a held marker turns out not to be one", () => {
    const decoder = createKeyDecoder();
    decoder.push("\u001b[2");

    expect(decoder.flush().map((key) => key.name)).toEqual(["[", "2"]);
    expect(decoder.flush()).toEqual([]);
  });

  it("gives back a paste that never ended rather than swallowing it", () => {
    const decoder = createKeyDecoder();
    decoder.push("\u001b[200~half");

    expect(decoder.flush().map((key) => key.sequence)).toEqual(["half"]);
    expect(decoder.flush()).toEqual([]);
  });

  it("puts one line of a paste into an input, and no control characters", () => {
    const handle = testRender(<Input focused={true} defaultValue="" />, {
      width: 12,
      height: 1,
    });
    handle.press("\u001b[200~one\u0007two\nthree\u001b[201~");

    // The bell is gone and the second line with it: an `Input` is one line and
    // cannot hold either, and a component that pasted them anyway would ring
    // the terminal on every redraw.
    expect(frameRow(handle.frame(), 0)).toBe("onetwo      ");
    handle.stop();
  });

  it("reports a capital as shift, since that is all a terminal says", () => {
    const key = decodeKeys("A")[0];
    expect([key.name, key.sequence, key.shift]).toEqual(["a", "A", true]);
  });

  it("runs global handlers in registration order, then the focused node", () => {
    const order: Array<string> = [];
    component First() {
      useKeyboard(() => order.push("first"));
      return null;
    }
    component Second() {
      useKeyboard(() => order.push("second"));
      return null;
    }
    const handle = testRender(
      <Box>
        <First />
        <Second />
        <Box focusable={true} focused={true} onKeyDown={() => order.push("focused")} />
      </Box>,
      { width: 4, height: 2 },
    );
    handle.press("k");
    expect(order).toEqual(["first", "second", "focused"]);
    handle.stop();
  });

  it("stops the focused node and later handlers with stopPropagation", () => {
    const later = fn();
    const focused = fn();
    component Stopper() {
      useKeyboard((key) => key.stopPropagation());
      return null;
    }
    component Later() {
      useKeyboard(later);
      return null;
    }
    const handle = testRender(
      <Box>
        <Stopper />
        <Later />
        <Box focusable={true} focused={true} onKeyDown={focused} />
      </Box>,
      { width: 4, height: 2 },
    );
    handle.press("k");
    expect(later).not.toHaveBeenCalled();
    expect(focused).not.toHaveBeenCalled();
    handle.stop();
  });

  it("stops only the focused node with preventDefault", () => {
    const later = fn();
    const focused = fn();
    component Preventer() {
      useKeyboard((key) => key.preventDefault());
      return null;
    }
    component Later() {
      useKeyboard(later);
      return null;
    }
    const handle = testRender(
      <Box>
        <Preventer />
        <Later />
        <Box focusable={true} focused={true} onKeyDown={focused} />
      </Box>,
      { width: 4, height: 2 },
    );
    handle.press("k");
    expect(later).toHaveBeenCalled();
    expect(focused).not.toHaveBeenCalled();
    handle.stop();
  });

  it("delivers to the box that says it has focus, and to no other", () => {
    const first = fn();
    const second = fn();
    const handle = testRender(
      <Box>
        <Box focusable={true} focused={false} onKeyDown={first} />
        <Box focusable={true} focused={true} onKeyDown={second} />
      </Box>,
      { width: 4, height: 2 },
    );
    handle.press("k");
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalled();
    handle.stop();
  });

  it("removes a handler when its component unmounts", () => {
    const handler = fn();
    component Listener() {
      useKeyboard(handler);
      return null;
    }
    component App() {
      const [on, setOn] = useState<boolean>(true);
      useKeyboard((key) => {
        if (key.name === "escape") {
          setOn(false);
        }
      });
      return <Box>{on ? <Listener /> : null}</Box>;
    }
    const handle = testRender(<App />, { width: 4, height: 2 });
    handle.press("a");
    expect(handler).toHaveBeenCalledTimes(1);
    handle.press("\u001b");
    handle.press("b");
    // Once for `a`, once for the escape that unmounted it, and not for `b`.
    expect(handler).toHaveBeenCalledTimes(2);
    handle.stop();
  });

  it("edits a line through the real key path", () => {
    const submitted = fn();
    component Form() {
      const [value, setValue] = useState<string>("");
      return (
        <Input
          focused={true}
          onInput={setValue}
          onSubmit={submitted}
          value={value}
          width={10}
          height={1}
        />
      );
    }
    const handle = testRender(<Form />, { width: 10, height: 1 });

    handle.press("uf");
    expect(frameRow(handle.frame(), 0)).toBe("uf        ");

    handle.press("\u007f");
    expect(frameRow(handle.frame(), 0)).toBe("u         ");

    handle.press("xy");
    expect(frameRow(handle.frame(), 0)).toBe("uxy       ");

    // Left, then a character: the insertion lands at the cursor, not the end.
    handle.press("\u001b[D");
    handle.press("z");
    expect(frameRow(handle.frame(), 0)).toBe("uxzy      ");

    handle.press("\r");
    expect(submitted).toHaveBeenCalledWith("uxzy");
    handle.stop();
  });

  it("shows a placeholder until something is typed", () => {
    const handle = testRender(<Input placeholder="name" focused={false} width={8} height={1} />, {
      width: 8,
      height: 1,
    });
    expect(frameRow(handle.frame(), 0)).toBe("name    ");
    handle.stop();
  });

  it("draws the cursor as an inverse cell where the caret is", () => {
    const handle = testRender(<Input defaultValue="ab" focused={true} width={6} height={1} />, {
      width: 6,
      height: 1,
    });
    handle.press("\u001b[D");
    const frame = handle.frame();
    expect(cell(frame, 0, 0).attributes).toBe(Attributes.NONE);
    expect(cell(frame, 1, 0).attributes).toBe(Attributes.INVERSE);
    handle.stop();
  });

  it("does not insert a character for a key that carries none", () => {
    const handle = testRender(<Input defaultValue="" focused={true} width={6} height={1} />, {
      width: 6,
      height: 1,
    });
    handle.press("\u001b[A");
    handle.press("");
    expect(frameRow(handle.frame(), 0).trimEnd()).toBe("");
    handle.stop();
  });
});

describe("what the terminal can take", () => {
  const env = (overrides: { [string]: string }) => ({ ...overrides });

  it("follows the CLI's precedence, highest first", () => {
    expect(detectCapabilities("never", "interactive", env({ COLORTERM: "truecolor" })).color).toBe(
      "none",
    );
    expect(detectCapabilities("auto", "interactive", env({ NO_COLOR: "1" })).color).toBe("none");
    expect(detectCapabilities("auto", "piped", env({ FORCE_COLOR: "3", TERM: "dumb" })).color).toBe(
      "truecolor",
    );
    expect(detectCapabilities("auto", "interactive", env({ TERM: "dumb" })).color).toBe("none");
    expect(detectCapabilities("auto", "interactive", env({ CLICOLOR: "0" })).color).toBe("none");
    expect(detectCapabilities("auto", "piped", env({ COLORTERM: "truecolor" })).color).toBe("none");
    expect(detectCapabilities("auto", "interactive", env({ COLORTERM: "truecolor" })).color).toBe(
      "truecolor",
    );
    expect(detectCapabilities("auto", "interactive", env({ TERM: "xterm-256color" })).color).toBe(
      "ansi256",
    );
    expect(detectCapabilities("auto", "interactive", env({ TERM: "xterm" })).color).toBe("ansi16");
  });

  it("keeps unicode glyphs when the locale is unset, and drops them when it says so", () => {
    expect(detectCapabilities("auto", "interactive", env({})).glyphs).toBe("unicode");
    expect(detectCapabilities("auto", "interactive", env({ LANG: "en_US.UTF-8" })).glyphs).toBe(
      "unicode",
    );
    expect(detectCapabilities("auto", "interactive", env({ LANG: "C" })).glyphs).toBe("ascii");
    expect(detectCapabilities("auto", "interactive", env({ TERM: "dumb" })).glyphs).toBe("ascii");
  });

  it("draws a box in ASCII when the terminal cannot be trusted with more", () => {
    const handle = testRender(<Box border={true} width={5} height={3} />, {
      width: 5,
      height: 3,
      capabilities: { color: "none", glyphs: "ascii", tty: "interactive" },
    });
    // The same geometry, drawn with characters a `TERM=dumb` terminal prints:
    // every glyph is one column wide in both vocabularies, so the box does not
    // change size when its characters do.
    expect(rows(handle.frame())).toEqual(["+---+", "|   |", "+---+"]);
    handle.stop();
  });

  it("downgrades a colour rather than dropping it", () => {
    const red = testRender(<Text fg="#ff0000">x</Text>, {
      width: 1,
      height: 1,
      capabilities: { color: "ansi16", glyphs: "unicode", tty: "interactive" },
    });
    expect(red.update().output).toContain("[0;91m");
    red.stop();

    const indexed = testRender(<Text fg="#ff0000">x</Text>, {
      width: 1,
      height: 1,
      capabilities: { color: "ansi256", glyphs: "unicode", tty: "interactive" },
    });
    expect(indexed.update().output).toContain("[0;38;5;196m");
    indexed.stop();
  });

  it("reads a colour written any of the three ways", () => {
    expect(parseColor("#f00")).toBe(0xff0000);
    expect(parseColor("#ff0000")).toBe(0xff0000);
    expect(parseColor("brightyellow")).toBe(0xffff00);
    expect(parseColor(0x123456)).toBe(0x123456);
    // A typo leaves the interface readable instead of stopping the program.
    expect(parseColor("chartreuse")).toBe(INHERIT);
  });

  it("takes the terminal's size from the variables first and the stream second", () => {
    // `COLUMNS` is POSIX's override and is what a `watch`, a `script` or a CI
    // wrapper sets when the stream itself cannot answer. Reading only
    // `stdout.columns` meant this package and the uf CLI could size the same
    // window differently.
    const stream = { columns: 100, rows: 30 };

    expect(detectSize(env({ COLUMNS: "40", LINES: "12" }), stream)).toEqual({
      columns: 40,
      rows: 12,
    });
    // Half an answer is still an answer for the half it covers.
    expect(detectSize(env({ COLUMNS: "40" }), stream)).toEqual({ columns: 40, rows: 30 });
    expect(detectSize(env({}), stream)).toEqual({ columns: 100, rows: 30 });
    // A stream that will not say how big it is, which is every stream that is
    // not a terminal.
    expect(detectSize(env({}), {})).toEqual({ columns: 80, rows: 24 });

    for (const value of ["", "0", "wide", "-1", "80x24"]) {
      expect(detectSize(env({ COLUMNS: value }), stream).columns).toBe(100);
    }
  });
});

describe("the width tables match the CLI's", () => {
  /** Every `(low, high)` pair in a Rust range table, as numbers. */
  const rustRanges = (source: string, name: string): Array<number> => {
    const start = source.indexOf(`static ${name}: &[(u32, u32)] = &[`);
    expect(start).toBeGreaterThan(-1);
    const end = source.indexOf("\n];", start);
    const body = source.slice(start, end);
    const out = [];
    for (const match of body.matchAll(/\(0x([0-9a-fA-F]+), 0x([0-9a-fA-F]+)\)/g)) {
      out.push(Number.parseInt(match[1], 16), Number.parseInt(match[2], 16));
    }
    return out;
  };

  /** The same, from the JavaScript module, read as source rather than imported. */
  const jsRanges = (source: string, name: string): Array<number> => {
    const start = source.indexOf(`const ${name}: $ReadOnlyArray<number> = [`);
    expect(start).toBeGreaterThan(-1);
    const end = source.indexOf("\n];", start);
    const body = source.slice(start, end);
    const out = [];
    for (const match of body.matchAll(/0x([0-9a-fA-F]+)/g)) {
      out.push(Number.parseInt(match[1], 16));
    }
    return out;
  };

  it("is the same data on both sides, so a terminal agrees with itself", () => {
    // Two copies of a Unicode table is a thing to be uncomfortable about. The
    // discomfort is made mechanical here rather than moral: edit one side and
    // this fails, which is the only property that makes the copy safe.
    const rust = fs.readFileSync(path.join(REPO, "crates/uf_term/src/text/tables.rs"), "utf8");
    const js = fs.readFileSync(path.join(REPO, "packages/tui/widths.js"), "utf8");
    expect(jsRanges(js, "ZERO_WIDTH")).toEqual(rustRanges(rust, "ZERO_WIDTH"));
    expect(jsRanges(js, "WIDE")).toEqual(rustRanges(rust, "WIDE"));
  });
});

describe("the capability precedence matches the CLI's", () => {
  // `packages/tui/capability.js` says its precedence list is
  // `crates/uf_term/src/capability.rs`'s, "variable for variable". That claim
  // is the whole reason the duplication is allowed, and until now nothing
  // checked it — ubugeeei-prod/uf#316 asks for the same treatment the width
  // tables get: read both files and compare the rules, rather than believe a
  // sentence about them.
  //
  // What is compared is the *order the two decision functions consult their
  // inputs in*, recovered from the source on each side rather than from the
  // prose above it. Both sides reach their inputs through small helpers, so
  // the helpers are resolved first: a Rust predicate is reduced to the
  // `TerminalEnv` fields it reads and those fields to the variables
  // `from_process` fills them from, and a JavaScript local is reduced to the
  // expression it was bound to. Swap two checks on either side and the two
  // lists stop matching.
  //
  // Colour only. The two files also disagree about glyphs — `NO_COLOR`
  // downgrades them in the CLI and not in the library — which is
  // ubugeeei-prod/uf#393, filed rather than quietly asserted either way here.

  const rust = (): string =>
    fs.readFileSync(path.join(REPO, "crates/uf_term/src/capability.rs"), "utf8");
  const js = (): string => fs.readFileSync(path.join(REPO, "packages/tui/capability.js"), "utf8");

  /** The body of `fn <name>` / `function <name>`, to its closing brace. */
  const body = (source: string, opener: string): string => {
    const start = source.indexOf(opener);
    expect(start).toBeGreaterThan(-1);
    let depth = 0;
    for (let index = source.indexOf("{", start); index < source.length; index += 1) {
      if (source[index] === "{") depth += 1;
      if (source[index] === "}") {
        depth -= 1;
        if (depth === 0) return source.slice(start, index);
      }
    }
    throw new Error(`unbalanced braces after ${opener}`);
  };

  /** Drop consecutive repeats, so `choice` tested twice is still one rule. */
  const runs = (items: Array<string>): Array<string> =>
    items.filter((item, index) => item !== items[index - 1]);

  /**
   * Which environment variables each `TerminalEnv` field is filled from.
   *
   * `locale` is filled from three, in their own precedence order, and that
   * order is part of what is being compared.
   */
  const rustFields = (source: string): { [string]: Array<string> } => {
    const out: { [string]: Array<string> } = {};
    let field = null;
    for (const line of body(source, "fn from_process()").split("\n")) {
      const named = line.match(/^\s{12}(\w+): /);
      if (named != null) {
        field = named[1];
        out[field] = [];
      }
      if (field == null) continue;
      for (const variable of line.matchAll(/var\("([A-Z_]+)"\)/g)) {
        out[field].push(variable[1]);
      }
    }
    return out;
  };

  /** Which variables each `impl TerminalEnv` predicate reads, in order. */
  const rustHelpers = (source: string): { [string]: Array<string> } => {
    const fields = rustFields(source);
    const out: { [string]: Array<string> } = {};
    for (const match of source.matchAll(/\n    fn (\w+)\(&self\)/g)) {
      const name = match[1];
      const read = [];
      for (const use of body(source, `fn ${name}(&self)`).matchAll(/self\.(\w+)/g)) {
        read.push(...(fields[use[1]] ?? []));
      }
      out[name] = runs(read);
    }
    return out;
  };

  /** Which variables a JavaScript helper reads, in order. */
  const jsHelpers = (source: string): { [string]: Array<string> } => {
    const out: { [string]: Array<string> } = {};
    for (const match of source.matchAll(/\nfunction (\w+)\(env: TerminalEnv\)/g)) {
      const name = match[1];
      const read = [];
      for (const use of body(source, `function ${name}(env: TerminalEnv)`).matchAll(
        /env\.([A-Z_]+)/g,
      )) {
        read.push(use[1]);
      }
      out[name] = runs(read);
    }
    return out;
  };

  it("consults the same inputs in the same order", () => {
    const rustSource = rust();
    const jsSource = js();

    const helpers = rustHelpers(rustSource);
    const rustInputs = (expression: string): Array<string> => {
      const found = [];
      for (const token of expression.matchAll(/env\.(\w+)\(\)|(ColorChoice|Tty)::|\bchoice\b/g)) {
        if (token[1] != null) found.push(...(helpers[token[1]] ?? []));
        else if (token[2] === "Tty") found.push("tty");
        else found.push("choice");
      }
      return found;
    };
    // Every guard in `detect_color`, in order: a `match` on its scrutinee and
    // an `if` on its condition. A `return` payload is a result and not a rule.
    const rustOrder = [];
    for (const line of body(rustSource, "fn detect_color(").split("\n")) {
      const guard = line.match(/^\s+(?:match|if) (.+?) \{\s*$/);
      if (guard != null) rustOrder.push(...rustInputs(guard[1]));
    }

    const functions = jsHelpers(jsSource);
    // `dumb` is computed in `detectCapabilities` and passed in, so the
    // parameter has to be resolved to the variable behind it.
    const bindings: { [string]: Array<string> } = {
      dumb: [(jsSource.match(/const dumb = env\.([A-Z_]+)/) ?? [])[1] ?? "?"],
    };
    const jsInputs = (expression: string): Array<string> => {
      const found = [];
      for (const token of expression.matchAll(/env\.([A-Z_]+)|\b(\w+)\b/g)) {
        if (token[1] != null) found.push(token[1]);
        else if (functions[token[2]] != null) found.push(...functions[token[2]]);
        else if (bindings[token[2]] != null) found.push(...bindings[token[2]]);
        else if (token[2] === "choice" || token[2] === "tty") found.push(token[2]);
      }
      return found;
    };
    const jsOrder = [];
    for (const line of body(jsSource, "function detectColor(").split("\n")) {
      const bound = line.match(/^\s+const (\w+) = (.+);\s*$/);
      if (bound != null) {
        bindings[bound[1]] = jsInputs(bound[2]);
        continue;
      }
      const guard = line.match(/^\s+if \((.+)\) \{\s*$/);
      if (guard != null) jsOrder.push(...jsInputs(guard[1]));
    }

    // The order the docs on both sides claim, spelled out so a failure says
    // which rule moved rather than only that something did.
    expect(runs(rustOrder)).toEqual([
      "choice",
      "NO_COLOR",
      "FORCE_COLOR",
      "CLICOLOR_FORCE",
      "TERM",
      "CLICOLOR",
      "tty",
    ]);
    expect(runs(jsOrder)).toEqual(runs(rustOrder));
  });

  it("falls back to the same two variables when no switch applies", () => {
    // The bottom of both functions: whatever `COLORTERM` and `TERM` advertise.
    // A side that stopped asking would pass the order check above, because the
    // fallback is not a guard.
    expect(rustHelpers(rust()).declared_level).toEqual(["COLORTERM", "TERM"]);
    expect(jsHelpers(js()).declaredLevel).toEqual(["COLORTERM", "TERM"]);

    const tail = (source: string, opener: string): string => {
      const lines = body(source, opener)
        .split("\n")
        .filter((line) => line.trim() !== "");
      return lines[lines.length - 1].trim();
    };
    expect(tail(rust(), "fn detect_color(")).toContain("env.declared_level()");
    expect(tail(js(), "function detectColor(")).toContain("declaredLevel(env)");
  });

  it("reads the same environment variables in the first place", () => {
    // A variable added to one side and not the other is a disagreement the
    // order check cannot see, because a rule that only one file has is a rule
    // only one file consults.
    const rustNames = new Set();
    for (const names of Object.values(rustFields(rust()))) {
      for (const name of names) rustNames.add(name);
    }
    const jsNames = new Set(
      [...body(js(), "export type TerminalEnv = ").matchAll(/readonly ([A-Z_]+)\?/g)].map(
        (match) => match[1],
      ),
    );
    expect([...jsNames].sort()).toEqual([...rustNames].sort());
  });

  it("resolves how big the terminal is from the same inputs, in the same order", () => {
    // The other thing two renderers can disagree about, and the one that had
    // no guard at all: the CLI assumed seventy-two columns and this package
    // read `stdout.columns`, so on a forty-column window one of them drew a
    // frame the terminal wrapped. Both now walk the same chain, and this
    // reads that chain out of each file rather than out of the prose above it.
    const snakeToCamel = (name) => name.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
    const rustChain = [
      ...body(rust(), "fn detect_size(").matchAll(
        /\.(declared_\w+)\(\)|\b(reported_\w+)\(|\b(FALLBACK_\w+)\b/g,
      ),
    ].map((match) => snakeToCamel(match[1] ?? match[2] ?? match[3]));
    const jsChain = [
      ...body(js(), "function detectSize(").matchAll(
        /\b(declared[A-Z]\w*|reported[A-Z]\w*)\(|\b(FALLBACK_\w+)\b/g,
      ),
    ].map((match) => match[1] ?? match[2]);

    // Spelled out, so a failure says which rule moved rather than only that
    // one did. Columns and rows are resolved separately on both sides: a
    // wrapper that cares about width sets `COLUMNS` and not `LINES`.
    expect(rustChain).toEqual([
      "declaredColumns",
      "reportedColumns",
      "FALLBACK_COLUMNS",
      "declaredRows",
      "reportedRows",
      "FALLBACK_ROWS",
    ]);
    expect(jsChain).toEqual(rustChain);
  });

  it("reads the size out of the same two variables", () => {
    // The chain above compares the order of the rules; this compares what the
    // first rule in it actually looks at. A side that renamed its helper and
    // kept the shape would pass one and fail the other.
    expect(rustHelpers(rust()).declared_columns).toEqual(["COLUMNS"]);
    expect(rustHelpers(rust()).declared_rows).toEqual(["LINES"]);
    expect(jsHelpers(js()).declaredColumns).toEqual(["COLUMNS"]);
    expect(jsHelpers(js()).declaredRows).toEqual(["LINES"]);
  });
});

describe("the manual is not a screenshot", () => {
  /**
   * The application `docs/app/guide/tui/_uf.page.mdx` shows, transcribed.
   *
   * The transcription is the weak point and it is deliberate: importing the
   * page's code block needs the docs build, and the alternative to both is a
   * picture of a terminal that stops being true the first time somebody
   * changes a border character. A copied component and an asserted frame catch
   * the change that matters — the drawing — and the copy is a dozen lines a
   * reader can compare by eye.
   */
  component Workers() {
    const [running, setRunning] = useState<number>(3);
    useKeyboard((key) => {
      if (key.name === "up") {
        setRunning((n) => n + 1);
      }
      if (key.name === "down") {
        setRunning((n) => Math.max(0, n - 1));
      }
    });
    return (
      <Box
        border={true}
        borderStyle="rounded"
        padding={1}
        title=" uf test "
        titleAlignment="center"
        width={28}
      >
        <Text bold={true}>Workers</Text>
        <Box flexDirection="row" justifyContent="space-between">
          <Text fg="gray">running</Text>
          <Text>{String(running)}</Text>
        </Box>
      </Box>
    );
  }

  /** The guide page, read as text. */
  const page = (): string =>
    fs.readFileSync(path.join(REPO, "docs/app/guide/tui/_uf.page.mdx"), "utf8");

  it("draws the frame the guide prints", () => {
    const fence = page().match(/```text\n([\s\S]*?)```/);
    expect(fence).not.toBe(null);
    const documented = (fence?.[1] ?? "").replace(/\n$/, "").split("\n");

    const handle = testRender(<Workers />, { width: 28, height: 6 });
    expect(rows(handle.frame())).toEqual(documented);
    handle.stop();
  });

  it("costs what the guide's table says it costs", () => {
    // The two numbers in the guide, measured here, so publishing a different
    // pair means changing this test on purpose.
    const handle = testRender(<Workers />, { width: 28, height: 6 });
    const first = handle.update();
    expect([first.cells, first.output.length]).toEqual([168, 239]);

    handle.press("\u001b[A");
    const next = handle.update();
    expect([next.cells, next.output.length]).toEqual([1, 8]);
    expect(frameRow(handle.frame(), 3)).toBe("│ running                4 │");

    const table = page().match(/\| the first frame \| (\d+) \| (\d+) \|/);
    const keystroke = page().match(/\| pressing the up arrow \| (\d+) \| (\d+) \|/);
    expect(table?.slice(1, 3)).toEqual([String(first.cells), String(first.output.length)]);
    expect(keystroke?.slice(1, 3)).toEqual([String(next.cells), String(next.output.length)]);
    handle.stop();
  });
});

describe("the guide's comparison against React Ink", () => {
  // `tools/bench/tui/` renders this workload through `@uniflowed/tui` and
  // through Ink, and the guide prints both columns. Ink's half is a recorded
  // measurement — Ink is a dependency of the benchmark and not of this
  // repository — but uf's half is this renderer, so it is asserted here rather
  // than trusted, the same way the small table further up the page is.
  //
  // The workload is imported rather than copied. A benchmark and a test that
  // disagree about what was rendered are two numbers about two different
  // things, and the number in the guide would be whichever of them was written
  // down.

  /** A stand-in terminal that counts bytes the way the benchmark's does. */
  const sink = () => {
    const encoder = new TextEncoder();
    let bytes = 0;
    return {
      columns: 8,
      rows: 2,
      isTTY: true,
      write(chunk: string) {
        bytes += encoder.encode(chunk).length;
        return true;
      },
      on() {},
      off() {},
      take(): number {
        const taken = bytes;
        bytes = 0;
        return taken;
      },
    };
  };

  /** A stdin that delivers exactly the keys a test types at it. */
  const keyboard = () => {
    let listener: ((chunk: string) => mixed) | null = null;
    return {
      isTTY: true,
      setRawMode() {},
      setEncoding() {},
      resume() {},
      pause() {},
      on(event: string, next: (chunk: string) => mixed) {
        if (event === "data") {
          listener = next;
        }
      },
      off() {
        listener = null;
      },
      type(bytes: string) {
        if (listener == null) {
          throw new Error("nothing is listening for input");
        }
        listener(bytes);
      },
    };
  };

  /**
   * The benchmark's application, stepped by a key rather than by a store.
   *
   * The benchmark drives both libraries through `useSyncExternalStore` so that
   * one line is the "state update" both stopwatches start on. A key press is
   * the same commit through the same renderer and is synchronous, which is
   * what a test wants — and the bytes are a function of the frames, not of
   * what moved between them.
   */
  component Bench() {
    const [step, setStep] = useState<number>(-1);
    useKeyboard(() => setStep((index) => index + 1));
    const state = step < 0 ? START : STEPS[step].to;
    return React.createElement(
      Box,
      { flexDirection: "column" },
      ...lines(state).map((line, index) =>
        React.createElement(Text, { key: String(index), wrap: "none" }, line),
      ),
    );
  }

  /** The guide's row for uf, as numbers. */
  const published = (): Array<number> => {
    const page = fs.readFileSync(path.join(REPO, "docs/app/guide/tui/_uf.page.mdx"), "utf8");
    const row = page.match(/\n\| `@uniflowed\/tui` \|([^\n]*)\|\n/);
    expect(row).not.toBe(null);
    return (row?.[1] ?? "")
      .split("|")
      .map((cell) => cell.replaceAll(/[^\d]/g, ""))
      .filter((cell) => cell !== "")
      .map(Number);
  };

  it("costs what the guide says, step for step", () => {
    const stdout = sink();
    const stdin = keyboard();
    const app = render(<Bench />, {
      stdout,
      stdin,
      color: "never",
      env: { COLUMNS: String(WIDTH), LINES: String(HEIGHT) },
      alternateScreen: false,
    });

    const measured = [stdout.take()];
    for (const _step of STEPS) {
      stdin.type("x");
      measured.push(stdout.take());
    }
    app.stop();

    // The frame is 80×24 and addressed cell by cell, so the first one is the
    // expensive one — and then one character of a status line at the *top* of
    // the frame costs eight bytes, which is the whole claim.
    expect(measured).toEqual([2102, 8, 345, 534]);
    expect(published()).toEqual(measured);
  });
});

describe("a real terminal, or something that is not one", () => {
  /** A stand-in for `process.stdout` that keeps what was written to it. */
  const output = (options: { isTTY: boolean }) => {
    const chunks: Array<string> = [];
    const listeners: Array<() => mixed> = [];
    return {
      chunks,
      listeners,
      columns: 8,
      rows: 2,
      isTTY: options.isTTY,
      write(chunk: string) {
        chunks.push(chunk);
        return true;
      },
      on(event: string, listener: () => mixed) {
        listeners.push(listener);
      },
      off() {},
      text(): string {
        return chunks.join("");
      },
    };
  };

  /** A stand-in for `process.stdin`, with a handle on the key listener. */
  const input = () => {
    const modes: Array<boolean> = [];
    let listener: ((chunk: string) => mixed) | null = null;
    return {
      modes,
      isTTY: true,
      setRawMode(raw: boolean) {
        modes.push(raw);
      },
      setEncoding() {},
      resume() {},
      pause() {},
      on(event: string, next: (chunk: string) => mixed) {
        if (event === "data") {
          listener = next;
        }
      },
      off() {
        listener = null;
      },
      type(bytes: string) {
        if (listener == null) {
          throw new Error("nothing is listening for input");
        }
        listener(bytes);
      },
    };
  };

  it("takes the screen, hides the cursor, and gives both back", () => {
    const stdout = output({ isTTY: true });
    const stdin = input();
    const app = render(<Text>hi</Text>, { stdin, stdout, env: { COLORTERM: "truecolor" } });

    const opened = stdout.text();
    expect(opened).toContain("\u001b[?1049h"); // the alternate screen
    expect(opened).toContain("\u001b[?25l"); // and no cursor of the terminal's own
    expect(opened).toContain("\u001b[?2004h"); // and pasted text, bracketed
    expect(opened).toContain("hi");
    // Raw mode, because a menu needs the keystroke rather than the line.
    expect(stdin.modes).toEqual([true]);

    app.stop();
    const closed = stdout.text().slice(opened.length);
    expect(closed).toContain("\u001b[?25h");
    expect(closed).toContain("\u001b[?1049l");
    // A terminal left in bracketed paste mode hands the *shell* the brackets.
    expect(closed).toContain("\u001b[?2004l");
    expect(stdin.modes).toEqual([true, false]);
  });

  it("draws the frame a keystroke produces, through the whole path", () => {
    const stdout = output({ isTTY: true });
    const stdin = input();

    component Typed() {
      const [seen, setSeen] = useState<string>("-");
      useKeyboard((key) => setSeen(key.sequence === "" ? key.name : key.sequence));
      return <Text wrap="none">{seen}</Text>;
    }

    const app = render(<Typed />, { stdin, stdout, env: { COLORTERM: "truecolor" } });
    const before = stdout.text().length;

    // A terminal delivers bytes, so the test delivers bytes: this is the
    // escape-sequence decoder, the key router, React's scheduler and the diff,
    // all of them, and none of them stubbed.
    stdin.type("\u001b[A");
    const written = stdout.text().slice(before);
    expect(written).toContain("up");
    expect(app.text().split("\n")[0]).toBe("up      ");

    app.stop();
  });

  it("lays out for the size the environment declares, not the stream's guess", () => {
    // The stand-in stream says eight columns; `COLUMNS` says four. A terminal
    // multiplexer, a `watch`, and a CI wrapper all set the variable and leave
    // the stream saying whatever it said before — so believing the stream
    // alone draws a frame four columns wider than the window, and every line
    // of it wraps.
    const stdout = output({ isTTY: true });
    const app = render(<Text wrap="none">abcdefghijkl</Text>, {
      stdin: input(),
      stdout,
      env: { COLUMNS: "4", LINES: "1" },
    });

    expect(app.text()).toBe("abcd");
    app.stop();
  });

  it("writes plain lines once when nobody is watching", () => {
    const stdout = output({ isTTY: false });
    const app = render(<Text>logged</Text>, { stdout, env: {} });

    // Nothing yet: cursor addressing in a file is noise, so a redirected
    // stream gets no incremental updates at all.
    expect(stdout.text()).toBe("");

    app.stop();
    expect(stdout.text()).toBe("logged  \n        \n");
    expect(stdout.text()).not.toContain("\u001b");
  });
});
