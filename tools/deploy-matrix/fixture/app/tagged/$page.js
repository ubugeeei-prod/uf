// @flow
//
// `isr`, on demand: a page whose lifetime is an hour, so only an invalidation
// replaces it during a test. `POST /api/revalidate?tag=isr-tag` expires it; the
// next request renders it again. The instant comes from the loader, as in
// `app/isr`.

import * as React from "@uniflowed/react";
import { cacheLife, cacheTag } from "@uniflowed/server/cache";

/** When this render happened, in epoch milliseconds. */
export async function loader(): Promise<{| readonly at: number |}> {
  return { at: Date.now() };
}

export default component OnDemand(at: number) {
  cacheLife({ revalidate: 3600 });
  cacheTag("isr-tag");
  return <h1>{`tagged rendered at ${String(at)}`}</h1>;
}
