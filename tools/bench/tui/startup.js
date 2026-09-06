// @flow
//
// What a `uf` command drawn in Flow would cost before it drew anything.
//
//   uf run bench:tui:startup
//
// # Why this number exists
//
// ubugeeei-prod/uf#316 is that uf's own CLI draws with `crates/uf_term` and
// cannot reach `@uniflowed/tui`, and the way out it proposes is a uf command
// that *is* a Flow entry point, run on the Capability JS Host the way a test
// file already is. It also says the thing that has to be true first: "a
// watch-mode UI that costs a Node start-up on every run is worse than the Rust
// one it replaces". This measures that start-up.
//
// It is deliberately the whole cost and not the renderer's share of it: Node
// booting, the loader registering, `uf transform` answering for every module
// in the import graph, React and the reconciler loading, and then a frame.
// A reader deciding whether a command can be written this way is deciding
// about all of it.
//
// # How to read the two numbers
//
// The script prints the time from *this process starting* to the first frame
// being on the terminal. Run it under `time` to add what the operating system
// charges to start the process at all. Run it twice to see the difference the
// transform cache makes — `.uf/cache/transform` is keyed by source hash and by
// the `uf` binary's identity, so the first run of a new build pays for every
// module and the second pays for none.

import { Buffer } from "node:buffer";
import { performance } from "node:perf_hooks";
import process from "node:process";

import * as React from "@uniflowed/react";
import { Box, Text, render } from "@uniflowed/tui";

/** A frame with enough in it to have cost something to lay out. */
component Startup() {
  return React.createElement(
    Box,
    { border: true, borderStyle: "rounded", padding: 1, title: " uf ", width: 40 },
    React.createElement(Text, { bold: true }, "a Flow entry point on the JS host"),
    React.createElement(Text, { fg: "gray" }, "drew this before printing the time"),
  );
}

/** A terminal that keeps its bytes rather than a terminal. */
const sink = {
  isTTY: true,
  columns: 80,
  rows: 24,
  bytes: 0,
  write(chunk: string): boolean {
    this.bytes += Buffer.byteLength(chunk, "utf8");
    return true;
  },
  on() {},
  off() {},
};

const handle = render(React.createElement(Startup), {
  stdout: sink,
  stdin: { isTTY: false },
  color: "never",
  env: { COLUMNS: "80", LINES: "24" },
  alternateScreen: false,
});
const drawn = performance.now();
handle.stop();

process.stdout.write(
  `${JSON.stringify({
    node: process.version,
    msToFirstFrame: Number(drawn.toFixed(1)),
    bytesWritten: sink.bytes,
  })}\n`,
);
