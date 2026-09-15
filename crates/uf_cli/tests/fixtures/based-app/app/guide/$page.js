// @flow
//
// A nested route with no parameters, so `uf build` prerenders it. Under
// `trailingSlash: "never"` the file is `dist/guide.html` rather than
// `dist/guide/index.html`, which is the spelling a static host serves at
// `/guide` without a redirect to the slash.

import * as React from "@uniflowed/react";

export default component Guide() {
  return <h1>based-app guide</h1>;
}
