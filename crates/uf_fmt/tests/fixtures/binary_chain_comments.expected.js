// @flow
// A comment on a link of a flattened `+` chain is spliced into the chain's
// parts, not wrapped around them: the printer separates the leftmost operand
// from the rest by looking for the first part that is a group, and a wrapper
// hides it. react-devtools' renderer.js lost two columns off four operands.

const operations = new Array<number>(
  // Identify which renderer this update is coming from.
  2 + // [rendererID, rootFiberID]
    // How big is the string table?
    1 + // [stringTableLength]
    // Then goes the actual string table.
    pendingStringTableLength +
    // All unmounts are batched in a single message.
    // [TREE_OPERATION_REMOVE, removedIDLength, ...ids]
    (numUnmountIDs > 0 ? 2 + numUnmountIDs : 0),
);

// The same chain in an array element, which indents the same way.
x = [
  2 +
    1 + // t2
    third,
];

// And in the places that do not indent at all, where the chain is already
// under an indent of its own.
function q() {
  return (
    2 +
    1 + // t2
    third
  );
}
const o = {
  k:
    2 +
    1 + // t2
    third,
};
