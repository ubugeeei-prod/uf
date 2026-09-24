// @flow
//
// `isr`, time-based: a prerendered page whose document stays true for two
// seconds. The first request past that is answered with the old document and
// regenerates the page behind it; a later request gets the new one. The instant
// in the body is how the check tells the build's render from the target's.
//
// The instant comes from the loader rather than the render: reading the clock
// is data, and a component is kept pure.

import * as React from "@uniflowed/react";
import { cacheLife, cacheTag } from "@uniflowed/server/cache";

/** When this render happened, in epoch milliseconds. */
export async function loader(): Promise<{| readonly at: number |}> {
  return { at: Date.now() };
}

export default component TimeBased(data: {| readonly at: number |}) {
  cacheLife({ revalidate: 2 });
  cacheTag("isr-time");
  return <h1>{`isr rendered at ${String(data.at)}`}</h1>;
}
