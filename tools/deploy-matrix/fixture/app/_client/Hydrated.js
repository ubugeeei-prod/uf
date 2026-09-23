// @flow
"use client";

// The proof that a page hydrated, and that it stays interactive.
//
// `useSyncExternalStore` with a server snapshot of `false` and a client
// snapshot of `true`: the server's HTML says `data-hydrated="no"`, and React
// hydrates with the server snapshot and then re-renders with the client's, so
// `"yes"` appears only in a tree React hydrated — without setting state in an
// effect. The button is the second half: a hydrated page whose event handlers
// are not attached would still say `"yes"`, and a click that moves the count is
// what an attached handler looks like from outside.

import * as React from "@uniflowed/react";
import { useState, useSyncExternalStore } from "@uniflowed/react";

/** Nothing to subscribe to: the answer changes once, at hydration. */
function subscribe(): () => void {
  return () => {};
}

/** The marker and the counter; rendered once, by the layout. */
export default component Hydrated() {
  const hydrated = useSyncExternalStore(
    subscribe,
    () => true,
    () => false,
  );
  const [clicks, setClicks] = useState<number>(0);
  return (
    <p id="hydration" data-hydrated={hydrated ? "yes" : "no"}>
      <button id="clicker" type="button" onClick={() => setClicks(clicks + 1)}>
        click
      </button>
      <output id="clicks">{String(clicks)}</output>
    </p>
  );
}
