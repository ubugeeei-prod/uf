// @flow
//
// The React the router asks a project for.
//
// Routes render through `react-server-dom-parcel`, which requires the React it
// was released with, so a router peer range that admitted an older React would
// let a project install a pair that cannot render: npm warns about the Flight
// package's peer and installs it anyway. The router's ranges are held to the
// Flight package's own, read from the package the workspace installed. See
// ubugeeei-prod/uf#519.

import fs from "node:fs";

import { describe, expect, it } from "@uniflowed/test";

/** A manifest, by a URL relative to this file. */
function manifest(relative: string): $FlowFixMe {
  return JSON.parse(fs.readFileSync(new URL(relative, import.meta.url), "utf8"));
}

/** The lowest version a range admits, as `[major, minor, patch]`. */
function floor(range: string): [number, number, number] {
  const match = /(\d+)\.(\d+)\.(\d+)/.exec(range);
  if (match == null) {
    throw new Error(`the range ${range} names no whole version to compare`);
  }
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

/** Whether version `a` is at least version `b`. */
function atLeast(a: [number, number, number], b: [number, number, number]): boolean {
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] > b[index];
  }
  return true;
}

describe("the router's React peer ranges", () => {
  it("admit no React the Flight package the router renders with refuses", () => {
    const router = manifest("./package.json");
    const flight = manifest("../../node_modules/react-server-dom-parcel/package.json");

    for (const name of ["react", "react-dom"]) {
      const ours = router.peerDependencies[name];
      const theirs = flight.peerDependencies[name];
      expect({ name, admitted: atLeast(floor(ours), floor(theirs)) }).toEqual({
        name,
        admitted: true,
      });
    }
  });
});
