// @flow
//
// A module a test wants to replace: it reaches the network, so a component
// that imports it cannot be exercised without standing in for the whole
// module. The only seam is the import, which is what `uft.mock` is for.
//
// Never imported by anything but `module-mock.test.js` and the modules beside
// it here.

/** Where the real client would send its requests. */
export const BASE: string = "https://api.test";

/** Reach the network. In a test, this must not happen. */
export function send(path: string): string {
  return `real ${BASE}${path}`;
}

/** A second export, so a partial mock has something to keep. */
export function origin(): string {
  return new URL(BASE).origin;
}
