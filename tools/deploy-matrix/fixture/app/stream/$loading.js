// @flow
//
// The `<Suspense>` fallback for everything under `/stream`: what a streaming
// target sends before the page below it has resolved.

import * as React from "@uniflowed/react";

export default component Loading() {
  // One text node nobody could confuse with the page's: the check reads it out
  // of a half-received body.
  return <p>{"stream: waiting"}</p>;
}
