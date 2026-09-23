// @flow
//
// A page that reads nothing, so the build writes it as the plain prerendered
// document it has always been. It is also what a test polls to learn that a
// server is up.

import * as React from "@uniflowed/react";

export default component Home() {
  return <h1>ppr-app home</h1>;
}
