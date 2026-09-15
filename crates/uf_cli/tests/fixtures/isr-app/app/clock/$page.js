// @flow
//
// A page that regenerates. It states a one-second lifetime, so `uf build`
// writes its document for regeneration rather than at `dist/clock/`, and every
// render prints the instant it happened. That instant is the whole assertion:
// the build's document carries the build's instant, and a document regenerated
// by a server carries a later one, with no rebuild in between.
//
// One text node rather than text beside an expression, so the instant is not
// split from its label by the comment React writes between adjacent text.

import { cacheLife, cacheTag } from "@uniflowed/server/cache";
import * as React from "@uniflowed/react";

export default component Clock() {
  cacheLife({ revalidate: 1 });
  cacheTag("clock");
  return <h1>{`rendered at ${String(Date.now())}`}</h1>;
}
