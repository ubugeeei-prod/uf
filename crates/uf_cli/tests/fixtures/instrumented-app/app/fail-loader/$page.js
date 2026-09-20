// @flow
import * as React from "@uniflowed/react";

export function loader() {
  throw new Error("loader failure");
}

export default component Page() {
  return <main>unreachable</main>;
}
