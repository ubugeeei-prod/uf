// @flow
//
// The React the router asks a project for.
//
// Two answers, because the router serves two kinds of application. Matching a
// URL, the native route table and a route rendered from its modules run on
// React 19.2.3, the React that Expo SDK 57 and React Native 0.87 ship, so the
// router's peer ranges admit it (ubugeeei-prod/uf#992). React Server Components
// render through `react-server-dom-parcel`, which requires the React it was
// released with. So that package is an optional peer, and the entries that load
// it check the React at run time (`internal/react-version.js`).
//
// The rule from ubugeeei-prod/uf#519 still holds: nothing may render Server
// Components on a React the Flight package refuses, because npm only warns about
// that package's peer and installs it anyway. What holds the rule now is the
// run-time floor, and it is read here against the Flight package this workspace
// installed.

import fs from "node:fs";

import { describe, expect, it } from "@uniflowed/test";

import { SERVER_COMPONENTS_REACT } from "./internal/react-version.js";

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

describe("the router's peer ranges", () => {
  it("admit the React that Expo SDK 57 and React Native 0.87 ship", () => {
    const { peerDependencies } = manifest("./package.json");

    for (const name of ["react", "react-dom"]) {
      expect({ name, admitted: atLeast([19, 2, 3], floor(peerDependencies[name])) }).toEqual({
        name,
        admitted: true,
      });
    }
  });

  it("leave react-dom and react-server-dom-parcel to the applications that need them", () => {
    const router = manifest("./package.json");

    // `react` stays required: every entry matches or renders with it.
    expect(Object.keys(router.peerDependencies).sort()).toEqual([
      "@react-navigation/native",
      "react",
      "react-dom",
      "react-native",
      "react-server-dom-parcel",
    ]);
    expect(router.peerDependenciesMeta).toEqual({
      "@react-navigation/native": { optional: true },
      "react-native": { optional: true },
      "react-dom": { optional: true },
      "react-server-dom-parcel": { optional: true },
    });
  });
});

describe("the React that Server Components need", () => {
  it("admits no React the Flight package the router renders with refuses", () => {
    const flight = manifest("../../node_modules/react-server-dom-parcel/package.json");

    for (const name of ["react", "react-dom"]) {
      const theirs = flight.peerDependencies[name];
      expect({ name, admitted: atLeast(floor(SERVER_COMPONENTS_REACT), floor(theirs)) }).toEqual({
        name,
        admitted: true,
      });
    }
  });

  it("is where the router's range for the Flight package starts", () => {
    const router = manifest("./package.json");

    expect(floor(router.peerDependencies["react-server-dom-parcel"])).toEqual(
      floor(SERVER_COMPONENTS_REACT),
    );
  });
});
