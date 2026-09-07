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

import { recordCount } from "../_actions/tally.js";

/** The string that proves this module is in a bundle. */
export const COUNTER_MARKER: string = "counter-marker-the-browser-needs-this";

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
