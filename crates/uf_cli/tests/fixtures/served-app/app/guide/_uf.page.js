// @flow
//
// A nested route with no parameters, so `uf build` prerenders it to
// `dist/guide/index.html` and both servers answer it from that file rather
// than by rendering. It is the half of the build that a static host could
// already serve, kept here so a test can tell the two halves apart.

import * as React from "@uniflowed/react";

export default component Guide() {
  return <h1>served-app guide</h1>;
}
