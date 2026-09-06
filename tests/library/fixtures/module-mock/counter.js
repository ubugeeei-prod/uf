// @flow
//
// Module state, so that "evaluated again" is something a test can see.
//
// `uft.resetModules` does not reach into a module that has already been
// evaluated — nothing can. What it does is make the next import evaluate the
// module afresh, and a counter that starts at zero is how that shows.

let count = 0;

/** Add one, and say what the count is now. */
export function bump(): number {
  count += 1;
  return count;
}

/** The count, without changing it. */
export function current(): number {
  return count;
}
