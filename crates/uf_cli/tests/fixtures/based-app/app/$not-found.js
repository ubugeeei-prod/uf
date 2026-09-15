// @flow
//
// A path under the base path with no route is this page, with a 404. A path
// outside the base path is not the application's at all, and gets the plain
// 404 every front door answers before the application is asked.

import * as React from "@uniflowed/react";

export const metadata: {| readonly title: string |} = { title: "Not found" };

export default component NotFound() {
  return <h1>based-app has no such page</h1>;
}
