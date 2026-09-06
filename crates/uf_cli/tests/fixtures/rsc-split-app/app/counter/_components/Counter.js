"use client";
// @flow
//
// The client half of the fixture: state, an event handler, and a marker string
// that has to survive into the browser bundle.
//
// `"use client"` makes this a client bundle root, so `crates/uf_rsc` reports a
// boundary at `app/counter/_uf.page.js` and every module above it stays in the
// client bundle. That is the assertion the split has to keep true: dropping a
// route the browser does not need must not drop the one it does.

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";

/** The string that proves this module is in a bundle. */
export const COUNTER_MARKER: string = "counter-marker-the-browser-needs-this";

export default component Counter() {
  const [count, setCount] = useState<number>(0);
  return (
    <p>
      <output data-marker={COUNTER_MARKER}>{count}</output>
      <button type="button" onClick={() => setCount(count + 1)}>
        add one
      </button>
    </p>
  );
}
