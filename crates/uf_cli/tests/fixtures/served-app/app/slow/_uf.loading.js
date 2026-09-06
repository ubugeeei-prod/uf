// @flow
//
// The fallback for everything under `/slow`.
//
// A `<Suspense>` boundary, which is the whole of what this file is: the router
// puts one around this segment's page and everything below it, so the layout
// and this can be sent while the page is still resolving. See
// ubugeeei-prod/uf#254.

import * as React from "@uniflowed/react";

export default component Loading() {
  // One text node, and one nobody could confuse with the page's: the test that
  // reads this off a socket is asserting *when* each arrived, so each has to be
  // findable in a half-received document.
  return <p>{"slow: waiting"}</p>;
}
