// @flow
//
// The project's 404: a path no route matches, and `notFound()` from inside a
// render, must both be answered by this with a `404` — not by the home page
// with a `200`, which is what a host's single-page fallback would do.

import * as React from "@uniflowed/react";

export const metadata: {| readonly title: string |} = { title: "Not found" };

export default component NotFound() {
  return <h1>matrix has no such page</h1>;
}
