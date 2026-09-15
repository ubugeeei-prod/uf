// @flow
//
// Internal to `@uniflowed/router`: the React that React Server Components need.
//
// The router installs beside React 19.2.3, the React that Expo SDK 57 and React
// Native 0.87 ship. Matching a URL, the native route table and a route rendered
// from its modules need nothing newer (ubugeeei-prod/uf#992). React Server
// Components do. They render through `react-server-dom-parcel`, React's own
// Flight renderer and client, which is released with React and requires the
// React it was released with: 19.3. So that package is an optional peer, and
// every module that loads it asks here first.
//
// # Why a check at run time
//
// A peer range cannot say it. The router has one range for `react`, and that
// range has to admit 19.2.3 for a native app, while an optional peer that is
// absent is never compared with anything. The failure is also not where the
// mistake is: React's Flight client imports against React 19.2 and fails later,
// inside a render, in terms of React's internals. So each entry that loads
// Flight refuses before it does anything, and names the React it found and the
// one it needs.
//
// It is a call rather than a statement at module scope, because no shipped
// module runs anything when it is imported
// (`crates/uf_lib/tests/package_surface.rs`).

import * as React from "react";

/** The oldest React that React Server Components render on. */
export const SERVER_COMPONENTS_REACT: string = "19.3.0";

/**
 * Why `entry` cannot run on React `installed`, or `null` when it can.
 *
 * The release numbers are compared and a prerelease tag is ignored, so a 19.3
 * canary counts as 19.3: React names a canary after the release it leads to.
 */
export function serverComponentsRefusal(entry: string, installed: string): string | null {
  const found = releaseOf(installed);
  const needed = releaseOf(SERVER_COMPONENTS_REACT);
  if (found != null && needed != null && !isBefore(found, needed)) {
    return null;
  }
  return (
    `@uniflowed/router: ${entry} needs React ${SERVER_COMPONENTS_REACT} or newer for React ` +
    `Server Components, and the React it loaded is ${installed}. Install react, react-dom and ` +
    "react-server-dom-parcel at ^19.3.0, or set `app.rsc: false` in uf.config.js to render " +
    "routes from their modules, which the router supports from React 19.2.3."
  );
}

/**
 * Refuse, with [`serverComponentsRefusal`]'s sentence, unless the React this
 * module loaded can render React Server Components.
 */
export function requireServerComponentsReact(entry: string): void {
  const refusal = serverComponentsRefusal(entry, React.version);
  if (refusal != null) {
    throw new Error(refusal);
  }
}

/** `[major, minor, patch]` of a version, or `null` when it does not start with one. */
function releaseOf(version: string): [number, number, number] | null {
  const match = /^(\d+)\.(\d+)\.(\d+)/.exec(version);
  return match == null ? null : [Number(match[1]), Number(match[2]), Number(match[3])];
}

/** Whether release `a` comes before release `b`. */
function isBefore(a: [number, number, number], b: [number, number, number]): boolean {
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) {
      return a[index] < b[index];
    }
  }
  return false;
}
