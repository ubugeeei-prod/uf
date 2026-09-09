// @flow
//
// What a `ScrollBox` costs as its content grows.
//
//   uf run bench:tui:window
//
// # The question
//
// `ScrollBox` claims that only the visible window is laid out, painted and
// diffed — that moving the window over a hundred thousand rows costs what
// moving it over thirty costs. `tests/library/tui.test.js` proves the shape of
// that by counting the calls layout makes into its leaves, which is exact and
// the same number on every machine and says nothing about how long anything
// takes. This says how long.
//
// The two are deliberately different in kind and neither replaces the other. A
// count cannot tell you that a constant is small; a clock cannot tell you that
// a curve is flat, because a flat curve and a gently sloping one look alike on
// a machine that is also doing something else. Together they can.
//
// # Two clocks, because there are two costs and only one of them is this one
//
// A keystroke here does two separate things, and lumping them together is how
// this benchmark was wrong the first time it ran:
//
// * **commit** is React reconciling the tree. It is proportional to how many
//   elements the *application* creates, which is the application's decision —
//   a hundred thousand `<Text>` children are a hundred thousand fibers whoever
//   is scrolling them. Nothing `ScrollBox` does can make that constant, and it
//   is reported here so that nobody reads the other column as the whole story.
// * **frame** is `layout` + `paint` + `diff`: everything between "the tree is
//   what it is" and "here are the bytes". This is the column the claim is
//   about, and the one that should not move when the content does.
//
// The row elements are built once and kept, which is what an application that
// scrolls a long log would do anyway. Rebuilding a hundred thousand elements
// on every keystroke is a cost a caller chooses, and charging it to the
// renderer would be measuring `Array.from`.
//
// # Three steps
//
// * **first** — nothing to a first frame. The one step that is *supposed* to
//   grow: the height of the content is what `scrollTop` is clamped against, so
//   every row is measured once. It is here to be seen growing, because a table
//   where every row is flat is a table nobody believes.
// * **scroll** — the offset moves by one row and nothing else changes.
// * **append** — one line is added at the end, which is what a log does. The
//   rows above have not moved, so the stack of heights is extended rather than
//   rebuilt.
//
// # Why this is not `bench.js`
//
// That one compares uf with React Ink on one frame and reports bytes as its
// deterministic half. Ink has no `ScrollBox`, so there is nothing to compare;
// and bytes are not the question either — the window sends the window whatever
// it cost to work out which rows the window is.
//
// Wall clock is noisy, so every figure is a median with the spread beside it.

import { performance } from "node:perf_hooks";
import process from "node:process";

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { ScrollBox, Text, testRender, useKeyboard } from "@uniflowed/tui";
import type { TestHandle } from "@uniflowed/tui";

/** The terminal every measurement here happens in. */
const WIDTH = 80;
const HEIGHT = 24;

/** How many rows the content has, across the four runs. */
const SIZES: $ReadOnlyArray<number> = [30, 1_000, 10_000, 100_000];

/** How many keystrokes each step is measured over. */
const SAMPLES = 20;

/** What one step came out as, on one of the two clocks. */
type Row = {
  readonly medianMs: number,
  readonly minMs: number,
  readonly maxMs: number,
};

/** Both clocks for one step, plus the cells the step re-sent. */
type Step = {
  readonly commit: Row,
  readonly frame: Row,
  readonly cells: number,
};

/**
 * The application, and the only two things a key does to it.
 *
 * `"s"` scrolls and `"a"` appends, which keeps both steps in one tree and one
 * commit path. The offset and the row count are state so that a step is a
 * React update rather than a fresh mount — the question is what an *existing*
 * application pays to move its window.
 */
const line = (index: number) =>
  React.createElement(Text, { key: String(index), wrap: "none" }, `line ${index}`);

component Log(rows: number) {
  const [top, setTop] = useState<number>(0);
  const [down, setDown] = useState<boolean>(true);
  // Whether the box is following its tail, which is what a log does while it is
  // being appended to and what makes an append move the window. A scroll takes
  // it back off the tail.
  const [following, setFollowing] = useState<boolean>(false);
  // The rows, kept as the elements themselves rather than as a count.
  // Appending is `[...lines, line(n)]`, so every element already in the list is
  // the *same object* as last render and React bails out of its subtree — which
  // is what makes this measure a log that grew by a line rather than a log that
  // was rebuilt. A benchmark that recreated all hundred thousand elements would
  // be timing `Array.from`.
  const [lines, setLines] = useState<Array<React.Node>>(() =>
    Array.from({ length: rows }, (_, index) => line(index)),
  );
  useKeyboard((key) => {
    if (key.name === "s") {
      // A bounce rather than a run to the bottom: thirty rows in a twenty-four
      // row window can only scroll six, and a step that stopped moving would be
      // timing a frame in which nothing happened while the hundred-thousand-row
      // one was timing a real scroll.
      const limit = Math.max(0, lines.length - HEIGHT);
      const next = down ? top + 1 : top - 1;
      setFollowing(false);
      if (next > limit) {
        setDown(false);
        setTop(Math.max(0, limit - 1));
      } else if (next < 0) {
        setDown(true);
        setTop(Math.min(limit, 1));
      } else {
        setTop(next);
      }
    }
    if (key.name === "a") {
      setFollowing(true);
      setLines((list) => [...list, line(list.length)]);
    }
  });
  return React.createElement(
    ScrollBox,
    {
      height: HEIGHT,
      width: WIDTH,
      scrollbar: false,
      scrollTop: following ? Number.MAX_SAFE_INTEGER : top,
    },
    lines,
  );
}

/** The median of a list this function may reorder. */
function median(values: Array<number>): number {
  const sorted = values.slice().sort((a, b) => a - b);
  const middle = sorted.length >> 1;
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

const summarise = (times: Array<number>): Row => ({
  medianMs: median(times),
  minMs: Math.min(...times),
  maxMs: Math.max(...times),
});

/** Mount `rows` rows, and time the first frame — the pass that is allowed to grow. */
function mount(rows: number): { handle: TestHandle, first: number } {
  const handle = testRender(React.createElement(Log, { rows }), {
    width: WIDTH,
    height: HEIGHT,
  });
  const started = performance.now();
  handle.update();
  return { handle, first: performance.now() - started };
}

/**
 * One step, measured `SAMPLES` times on one mounted application.
 *
 * The same application throughout rather than a fresh one per sample, because
 * a fresh one would pay the first frame's measurement again and that is the
 * cost this is trying to hold separate. The first keystroke is taken off the
 * clock for the same reason every other benchmark here warms up.
 */
function step(handle: TestHandle, key: string): Step {
  handle.press(key);
  handle.update();

  const commits = [];
  const frames = [];
  let cells = 0;
  for (let sample = 0; sample < SAMPLES; sample += 1) {
    const beforeCommit = performance.now();
    handle.press(key);
    const beforeFrame = performance.now();
    const update = handle.update();
    const after = performance.now();
    commits.push(beforeFrame - beforeCommit);
    frames.push(after - beforeFrame);
    cells = update.cells;
  }
  return { commit: summarise(commits), frame: summarise(frames), cells };
}

const pad = (text: string, width: number) => text.padStart(width);
const ms = (row: Row) => `${pad(row.medianMs.toFixed(2), 8)} ms`;
const spread = (row: Row) => `${row.minMs.toFixed(2)}–${row.maxMs.toFixed(2)}`;

function main() {
  // A whole run that is thrown away. The first mount in a process pays for
  // React's module initialisation and every shape V8 has not seen yet, and
  // charging that to the first size measured would make the smallest content
  // look like the most expensive one — which it did, the first time this was
  // written.
  const warm = mount(SIZES[0]).handle;
  step(warm, "s");
  step(warm, "a");
  warm.stop();

  const table = SIZES.map((rows) => {
    // The first frame is reported as the fastest of three rather than as a
    // median, for the reason `startup.js` gives about start-up: it allocates a
    // row's worth of everything per row, so it is the one step a garbage
    // collection can land in the middle of, and contention can only make it
    // slower. The steps below it run on the last of the three.
    let first = Number.POSITIVE_INFINITY;
    let handle = null;
    for (let attempt = 0; attempt < 3; attempt += 1) {
      if (handle != null) {
        handle.stop();
      }
      const mounted = mount(rows);
      first = Math.min(first, mounted.first);
      handle = mounted.handle;
    }
    const live = handle;
    if (live == null) {
      throw new Error("unreachable: three mounts leave one handle");
    }
    const scroll = step(live, "s");
    const append = step(live, "a");
    live.stop();
    return { rows, first, scroll, append };
  });

  const size = (rows: number) => pad(rows.toLocaleString("en-US"), 8);

  process.stdout.write(`${WIDTH}×${HEIGHT}, median of ${SAMPLES}\n\n`);
  process.stdout.write("layout, paint and diff — what this renderer costs\n");
  process.stdout.write(
    `${pad("rows", 8)} ${pad("first frame", 11)} ${pad("scroll", 11)} ${pad("append", 11)}\n`,
  );
  for (const row of table) {
    process.stdout.write(
      `${size(row.rows)} ${pad(`${row.first.toFixed(2)} ms`, 11)} ${ms(row.scroll.frame)} ${ms(row.append.frame)}\n`,
    );
  }

  process.stdout.write("\nReact reconciling the tree — the application's cost, not this one's\n");
  process.stdout.write(`${pad("rows", 8)} ${pad("scroll", 11)} ${pad("append", 11)}\n`);
  for (const row of table) {
    process.stdout.write(`${size(row.rows)} ${ms(row.scroll.commit)} ${ms(row.append.commit)}\n`);
  }

  process.stdout.write("\ncells re-sent, which is deterministic and is checked by tests\n");
  process.stdout.write(`${pad("rows", 8)} ${pad("scroll", 11)} ${pad("append", 11)}\n`);
  for (const row of table) {
    process.stdout.write(
      `${size(row.rows)} ${pad(String(row.scroll.cells), 11)} ${pad(String(row.append.cells), 11)}\n`,
    );
  }

  process.stdout.write("\nspread of the frame column, low to high, in milliseconds\n");
  for (const row of table) {
    process.stdout.write(
      `${size(row.rows)} ${pad(spread(row.scroll.frame), 16)} ${pad(spread(row.append.frame), 16)}\n`,
    );
  }
}

main();
