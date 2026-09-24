// @flow
//
// The hole in `/ppr`'s static shell: it reads a cookie, so it can only be
// rendered for a request, and it waits so its arrival is measurably after the
// shell's.

import * as React from "@uniflowed/react";
import { cookies } from "@uniflowed/server";

/** How long the hole takes once it has read the request, in milliseconds. */
const SLOW = 1200;

/** The per-request part of `/ppr`: who the `who` cookie says is asking. */
export async function Hole(): Promise<React.Node> {
  const who = cookies().get("who") ?? "nobody";
  await new Promise((resolve) => {
    setTimeout(resolve, SLOW);
  });
  return <p id="hole">{`ppr hole for ${who}`}</p>;
}
