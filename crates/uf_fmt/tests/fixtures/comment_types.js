// @flow

// Flow's comment types: annotations that a file can carry and still run
// under bare `node`. Normalising them away is not a layout decision, it is a
// file that needed no build step and now needs one. See ubugeeei-prod/uf#126.

function greet(name /*: string */) /*: string */ {
  return "hi " + name;
}

const total /*: number */ = 1;
const plain: number = 2;

function mixed(a /*: string */, b: number, c /*: ?Array<string> */) {
  return a;
}

class Holder {
  field /*: string */ = "x";
  method(x /*: number */) /*: void */ {}
}

const arrow = (x /*: number */) /*: number */ => x;

// The bytes come back untouched, spacing included. A real annotation would be
// re-printed as `{ [string]: string }`; inside the comment it is text.
const table /*: {[string]: string} */ = {};
const wide /*: Array<{product: string, packagePath: string, packageName: string}> */ = [];

// Ordinary annotations next to them are still normalised.
const spaced: {[string]: string} = {};
