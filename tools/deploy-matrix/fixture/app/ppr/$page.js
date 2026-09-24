// @flow
//
// `ppr`: a static shell with a hole.
//
// No parameters, and the only request-dependent read is inside `<Suspense>`,
// so the build prerenders everything around the boundary and the target fills
// the boundary per request. The shell is sent at once; the hole arrives about
// `Hole`'s delay later. A target that buffers (serverless) cannot send one
// before the other, so its build refuses this page by name — which the matrix
// checks too.

import * as React from "@uniflowed/react";

import { Hole } from "./Hole.js";

export default component Partial() {
  return (
    <article>
      <h1 id="shell">ppr shell</h1>
      <React.Suspense fallback={<p id="hole-fallback">{"ppr: waiting"}</p>}>
        <Hole />
      </React.Suspense>
    </article>
  );
}
