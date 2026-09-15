// @flow
//
// A page that states no lifetime, so the build writes it as the plain
// prerendered document it has always been. It is also what a test polls to
// learn that a server is up.

import * as React from "@uniflowed/react";

export default component Home() {
  return <h1>isr-app home</h1>;
}
