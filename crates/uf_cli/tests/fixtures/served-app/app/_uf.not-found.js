// @flow
//
// A path with no route must be answered by this, with a 404 — not by the home
// page with a 200, which is what a static host's SPA fallback does and what
// Vite's own preview server would have done before `appType: "custom"`.

import * as React from "@uniflowed/react";

export const metadata: {| readonly title: string |} = { title: "Not found" };

export default component NotFound() {
  return <h1>served-app has no such page</h1>;
}
