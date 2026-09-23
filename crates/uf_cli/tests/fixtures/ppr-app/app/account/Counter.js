"use client";
// @flow
//
// A client component in the static shell, so the shell names a client module
// the way most pages' shells do, and the browser hydrates it from the
// request's payload.

import * as React from "@uniflowed/react";

export component Counter() {
  const [count, setCount] = React.useState(0);
  return (
    <button id="counter" type="button" onClick={() => setCount(count + 1)}>
      {`clicked ${String(count)} times`}
    </button>
  );
}
