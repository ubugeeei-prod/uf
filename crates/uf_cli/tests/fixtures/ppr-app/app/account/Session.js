// @flow
//
// The hole: it reads the request, and then waits for something slower than a
// shell — the stand-in for a session store or an API that answers per person.
//
// The wait is what the fixture's assertions lean on. The shell has to arrive
// before it, and this has to arrive after, so a server that rendered the whole
// page before sending a byte cannot pass by being quick. One text node, so the
// cookie's value is not split from its label by the comment React writes
// between adjacent text.

import { cookies } from "@uniflowed/server";
import * as React from "@uniflowed/react";

/** How long the hole takes, in milliseconds, once it has read the request. */
const SLOW = 400;

export async function Session(): Promise<React.Node> {
  const session = cookies().get("session") ?? "nobody";
  await new Promise((resolve) => {
    setTimeout(resolve, SLOW);
  });
  return <p id="hole">{`signed in as ${session}`}</p>;
}
