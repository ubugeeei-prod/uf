// @flow
//
// The terminal both libraries are measured against.
//
// A `Vec<u8>` with `columns` on it. Both `@uniflowed/tui` and React Ink write
// to whatever stream they are handed, so handing them the same one is what
// makes the two byte counts comparable — a real terminal would add its own
// emulator to the measurement, and a pipe would put both libraries on the path
// they take when nobody is watching, which is not the path being compared.
//
// `isTTY` is `true` because that is the claim being tested. Both libraries do
// something entirely different, and entirely sensible, for a stream nobody is
// looking at: uf writes the last frame once at the end, and Ink does the same.
// Neither of those is a rendering strategy.

import { Buffer } from "node:buffer";
import { performance } from "node:perf_hooks";

/** A stream that counts what was written to it and when. */
export type Sink = {
  readonly isTTY: boolean,
  readonly columns: number,
  readonly rows: number,
  write(chunk: string): boolean,
  on(event: string, listener: () => mixed): mixed,
  off(event: string, listener: () => mixed): mixed,
  removeListener(event: string, listener: () => mixed): mixed,
  /** How many writes have landed since the last `take`. */
  pending(): number,
  /** Bytes written since the last `take`. */
  take(): { readonly bytes: number, readonly writes: number, readonly at: number },
  /** Everything written since the last `take`, as text. */
  text(): string,
  ...
};

/** A fresh sink of the given size. */
export function createSink(columns: number, rows: number): Sink {
  let bytes = 0;
  let writes = 0;
  let at = 0;
  let chunks: Array<string> = [];

  const sink: Sink = {
    isTTY: true,
    columns,
    rows,
    write(chunk: string): boolean {
      chunks.push(chunk);
      bytes += Buffer.byteLength(chunk, "utf8");
      writes += 1;
      // Taken after the write rather than before it, because the claim being
      // measured is "from the state update to the write returning" and the
      // write is where the cost of a large frame actually lands.
      at = performance.now();
      return true;
    },
    on(): mixed {
      return undefined;
    },
    off(): mixed {
      return undefined;
    },
    removeListener(): mixed {
      return undefined;
    },
    pending() {
      return writes;
    },
    take() {
      const taken = { bytes, writes, at };
      bytes = 0;
      writes = 0;
      at = 0;
      chunks = [];
      return taken;
    },
    text() {
      return chunks.join("");
    },
  };
  return sink;
}
