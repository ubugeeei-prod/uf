// @flow
//
// The application's root, which under a base path is the base itself: `/docs`,
// and `/docs/` is a `308` to it under `trailingSlash: "never"`.
//
// The `Link` is the assertion. `to` is an application path, and the anchor it
// writes is the address, so the prerendered document has `href="/docs/guide"`
// before any script has run.

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";

export default component Home() {
  return (
    <>
      <h1>based-app home</h1>
      <Link to="/guide">guide</Link>
    </>
  );
}
