"use client";
// @flow
//
// The client half of the fixture: state, an event handler, a server action,
// and a marker string that has to survive into the browser bundle.
//
// `"use client"` makes this a client bundle root, so `crates/uf_rsc` reports a
// boundary at `app/counter/_uf.page.js` and every module above it stays in the
// client bundle. That is the assertion the split has to keep true: dropping a
// route the browser does not need must not drop the one it does.
//
// The import of `../_actions/tally.js` is the other half. On the server it is
// the function; in the browser it is a `createServerReference`, so this module
// ships and the one it imports does not. Clicking `record` posts the id to
// this page's own URL and renders what the server answered.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";

import { expect, it } from "@uniflowed/test";

import { recordCount } from "../_actions/tally.js";

/** The string that proves this module is in a bundle. */
export const COUNTER_MARKER: string = "counter-marker-the-browser-needs-this";

/**
 * An in-source test in the one module of this fixture the browser certainly
 * gets.
 *
 * The marker above has to be in `dist/assets/*.js` and this one has to not be,
 * from the same file and the same build — which is the whole of what
 * ubugeeei-prod/uf#518 asks to be proved rather than assumed. `uf` compiles
 * `import.meta.uf.test` to `void 0` for every transform except the ones
 * `uf test` starts, so what the bundler is handed is `if (void 0) { … }`.
 *
 * A separate string rather than reusing `COUNTER_MARKER`: a test that looked
 * for the same literal twice could not tell "the block was removed" from "the
 * block was kept and the marker appears anyway".
 *
 * The bindings come from the top-level import of `@uniflowed/test` above,
 * which is the form uf recommends and the harder one to get right: the block
 * folding away is not enough on its own, because an unused import of a package
 * that spawns processes and reads `node:module` would still have to be shaken
 * out of a browser bundle. `@uniflowed/test` declares `sideEffects: false` so
 * that it can be, and this fixture is where that stops being a claim.
 */
if (import.meta.uf.test) {
  it("counts up", () => {
    expect("in-source-marker-no-build-may-ship-this").toBe(COUNTER_MARKER);
  });
}

export default component Counter() {
  const [count, setCount] = useState<number>(0);
  const [recorded, setRecorded] = useState<string>("");
  return (
    <p>
      <output data-marker={COUNTER_MARKER}>{count}</output>
      <button type="button" onClick={() => setCount(count + 1)}>
        add one
      </button>
      <button
        type="button"
        onClick={() => {
          void recordCount(count).then((answer) => {
            setRecorded(`${String(answer.total)} ${answer.visitor}`);
          });
        }}
      >
        record
      </button>
      <output data-recorded>{recorded}</output>
    </p>
  );
}
